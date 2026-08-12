//! Multi-agent orchestration (spec sections 17-22): let several agent CLIs
//! work on one goal, each seeing real output from the one before it.
//!
//! **Honest scope**: this is a *sequential relay*, not the full spec's
//! parallel task-graph/DAG with live bidirectional agent-to-agent
//! messaging. That would need the runtime to hold long-lived process state
//! across requests (still deferred, see ADR 0001) and a live inter-agent
//! protocol none of these five CLIs actually speak. What's built here is
//! real and useful within that honest limit: every agent in the list runs
//! in turn, in the *same* git worktree (so agent 2 sees agent 1's actual
//! file changes on disk, not just a text summary) and is handed agent 1's
//! real captured output as part of its own prompt. That's genuine
//! hand-off, not a fabricated live conversation — each step is
//! `task::run` under the hood, so it gets the exact same real worktree
//! isolation, artifact capture, and "learn from errors" memory recording
//! as a standalone task.

use crate::context::Context;
use crate::task::{self, RunTaskOptions};
use anyhow::{bail, Context as _, Result};
use rusqlite::Connection;
use single_protocol::{TaskRecord, TaskStatus};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct OrchestrateOptions<'a> {
    pub goal: &'a str,
    pub agents: &'a [String],
    pub cwd: &'a Path,
    pub use_worktree: bool,
    pub timeout: Duration,
}

/// Caps how much of one agent's output gets quoted into the next agent's
/// prompt — real CLIs have finite context, and an unbounded relay could
/// otherwise balloon every subsequent prompt with the full transcript of
/// everything before it.
const MAX_HANDOFF_CHARS: usize = 4000;

pub fn run(conn: &Connection, ctx: &Context, opts: OrchestrateOptions) -> Result<Vec<TaskRecord>> {
    if opts.agents.is_empty() {
        bail!("orchestrate needs at least one --agent");
    }

    let shared_cwd = prepare_shared_cwd(ctx, &opts)?;

    crate::state::record_event(
        conn,
        "orchestrate.started",
        &format!("agents={} goal=\"{}\"", opts.agents.join(","), opts.goal),
    )?;

    let mut records = Vec::new();
    let mut previous: Option<(String, String)> = None; // (agent name, captured output)

    for (i, agent) in opts.agents.iter().enumerate() {
        let description = build_prompt(opts.goal, previous.as_ref());

        crate::state::record_event(conn, "orchestrate.step_started", &format!("agent={agent} step={}/{}", i + 1, opts.agents.len()))?;

        let record = task::run(conn, ctx, RunTaskOptions { description: &description, agent, cwd: &shared_cwd, use_worktree: false, account: None, timeout: opts.timeout })?;

        let failed = record.status == TaskStatus::Failed;
        crate::state::record_event(
            conn,
            if failed { "orchestrate.step_failed" } else { "orchestrate.step_completed" },
            &format!("agent={agent} task=#{}", record.id),
        )?;

        previous = record
            .artifact_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|output| (agent.clone(), output));

        let stop_here = failed;
        records.push(record);
        if stop_here {
            crate::state::record_event(conn, "orchestrate.stopped_early", &format!("after step {}/{}", i + 1, opts.agents.len()))?;
            break;
        }
    }

    crate::state::record_event(conn, "orchestrate.finished", &format!("steps_completed={}", records.len()))?;
    Ok(records)
}

fn prepare_shared_cwd(ctx: &Context, opts: &OrchestrateOptions) -> Result<PathBuf> {
    if !opts.use_worktree {
        return Ok(opts.cwd.to_path_buf());
    }
    let repo_ctx = single_core::project_context::resolve(opts.cwd);
    let repo_root = repo_ctx.repo_root.context("not a git repository; --worktree requires one")?;
    let relay_id = chrono::Utc::now().timestamp_millis();
    let worktree_path = ctx.dirs.state_dir().join("worktrees").join(format!("relay-{relay_id}"));
    let branch = format!("single/relay-{relay_id}");
    single_core::worktree::add(Path::new(&repo_root), &worktree_path, &branch)?;
    Ok(worktree_path)
}

fn build_prompt(goal: &str, previous: Option<&(String, String)>) -> String {
    match previous {
        None => goal.to_string(),
        Some((agent, output)) => {
            let truncated = truncate(output, MAX_HANDOFF_CHARS);
            format!(
                "Overall goal: {goal}\n\n--- Output from the previous agent ({agent}) in this relay ---\n{truncated}\n--- end previous output ---\n\nContinue this work toward the overall goal above."
            )
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut truncated: String = s.chars().take(max_chars).collect();
    truncated.push_str("\n[...truncated...]");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_uses_goal_verbatim_for_first_step() {
        let prompt = build_prompt("add a .gitignore", None);
        assert_eq!(prompt, "add a .gitignore");
    }

    #[test]
    fn build_prompt_includes_previous_output_and_goal() {
        let prompt = build_prompt("add tests", Some(&("claude".to_string(), "created foo.rs".to_string())));
        assert!(prompt.contains("add tests"));
        assert!(prompt.contains("claude"));
        assert!(prompt.contains("created foo.rs"));
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_caps_long_strings() {
        let long = "x".repeat(10_000);
        let result = truncate(&long, 50);
        assert!(result.starts_with(&"x".repeat(50)));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn run_rejects_empty_agent_list() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = Connection::open_in_memory().unwrap();
        task::ensure_schema(&conn).unwrap();
        crate::state::ensure_events_schema(&conn).unwrap();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() };

        let opts = OrchestrateOptions { goal: "x", agents: &[], cwd: dir.path(), use_worktree: false, timeout: Duration::from_secs(1) };
        assert!(run(&conn, &ctx, opts).is_err());
    }
}
