//! Task orchestration (spec sections 17-21), Phase 4 scope: a single task
//! delegated to a single named agent, run synchronously (the request that
//! starts it blocks until the agent finishes or times out), optionally
//! isolated in its own git worktree.
//!
//! This is honestly narrower than the full spec: no task graph/DAG, no
//! automatic agent selection, no parallel multi-agent coordination, no
//! background/cancellable execution. Those all need the runtime to hold
//! live process state across multiple requests (spec section 3's daemon
//! lifecycle, deferred since Phase 1's ADR 0001) or an actual reasoning
//! step to pick agents (spec section 20) that SingleCLI itself doesn't
//! have. What's here is real: a real agent subprocess, real git worktree
//! isolation, a real captured artifact, and a real persisted record —
//! just for one agent at a time, one task at a time.

use crate::context::Context;
use crate::integrations;
use crate::memory;
use anyhow::{Context as _, Result};
use rusqlite::{params, Connection, OptionalExtension};
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::{MemoryScope, MemorySource, TaskStatus, TaskRecord};
use std::path::Path;
use std::time::Duration;

pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            description TEXT NOT NULL,
            agent TEXT NOT NULL,
            status TEXT NOT NULL,
            worktree_path TEXT,
            artifact_path TEXT,
            exit_code INTEGER,
            timed_out INTEGER NOT NULL DEFAULT 0,
            summary TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        (),
    )?;
    Ok(())
}

fn status_as_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Created => "created",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
    }
}

fn parse_status(s: &str) -> Result<TaskStatus> {
    Ok(match s {
        "created" => TaskStatus::Created,
        "running" => TaskStatus::Running,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        other => anyhow::bail!("unknown task status: {other}"),
    })
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<TaskRecord> {
    let status_str: String = row.get("status")?;
    Ok(TaskRecord {
        id: row.get("id")?,
        description: row.get("description")?,
        agent: row.get("agent")?,
        status: parse_status(&status_str)
            .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?,
        worktree_path: row.get("worktree_path")?,
        artifact_path: row.get("artifact_path")?,
        exit_code: row.get("exit_code")?,
        timed_out: row.get::<_, i64>("timed_out")? != 0,
        summary: row.get("summary")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<TaskRecord>> {
    conn.query_row("SELECT * FROM tasks WHERE id = ?1", params![id], row_to_task)
        .optional()
        .context("querying task by id")
}

pub fn list(conn: &Connection) -> Result<Vec<TaskRecord>> {
    let mut stmt = conn.prepare("SELECT * FROM tasks ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], row_to_task)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn create(conn: &Connection, description: &str, agent: &str) -> Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (description, agent, status, timed_out, created_at, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?4)",
        params![description, agent, status_as_str(TaskStatus::Created), now],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
fn finish(
    conn: &Connection,
    id: i64,
    status: TaskStatus,
    worktree_path: Option<&str>,
    artifact_path: Option<&str>,
    exit_code: Option<i32>,
    timed_out: bool,
    summary: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE tasks SET status = ?1, worktree_path = ?2, artifact_path = ?3, exit_code = ?4, timed_out = ?5, summary = ?6, updated_at = ?7 WHERE id = ?8",
        params![status_as_str(status), worktree_path, artifact_path, exit_code, timed_out as i64, summary, now, id],
    )?;
    Ok(())
}

fn set_status(conn: &Connection, id: i64, status: TaskStatus) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3", params![status_as_str(status), now, id])?;
    Ok(())
}

pub struct RunTaskOptions<'a> {
    pub description: &'a str,
    pub agent: &'a str,
    pub cwd: &'a Path,
    pub use_worktree: bool,
    /// If set, runs against this captured account's isolated `$HOME`
    /// (`single_core::account::ensure_isolated_home`) instead of the real
    /// one — lets multiple accounts of the same agent run concurrently.
    /// Ignored when `real_home` is set.
    pub account: Option<&'a str>,
    /// Skips isolated-home materialization entirely and runs the agent
    /// against the real, ambient `$HOME` — for tasks that need to
    /// actually modify the real system (dotfiles, installed packages,
    /// desktop config), not a SingleCLI-managed sandbox copy. An
    /// explicit opt-in: the agent gets full access to real credentials
    /// and files, which is exactly what the isolated-home default exists
    /// to avoid in the ordinary case.
    pub real_home: bool,
    pub timeout: Duration,
}

/// Runs a task to completion synchronously: creates the record, optionally
/// isolates it in a new git worktree off `cwd`'s repo, invokes the agent's
/// real non-interactive mode, captures the output as an artifact file, and
/// records the final status. Every stage emits an event via
/// `crate::state::record_event` so the run is auditable (spec section 32)
/// even though there's no live event *stream* yet (spec section 25) — only
/// a persisted log.
pub fn run(conn: &Connection, ctx: &Context, opts: RunTaskOptions) -> Result<TaskRecord> {
    let Some(adapter) = for_agent_with_custom(opts.agent, &ctx.dirs.agents_dir()) else {
        anyhow::bail!("unknown agent: {}", opts.agent);
    };
    if !adapter.discover().detected {
        anyhow::bail!("agent '{}' is not installed; run `single setup --yes` first", opts.agent);
    }

    let id = create(conn, opts.description, opts.agent)?;
    crate::state::record_event(conn, "task.created", &format!("#{id} agent={} \"{}\"", opts.agent, opts.description))?;

    let run_cwd: std::path::PathBuf = if opts.use_worktree {
        let repo_ctx = single_core::project_context::resolve(opts.cwd);
        let Some(repo_root) = repo_ctx.repo_root else {
            finish(conn, id, TaskStatus::Failed, None, None, None, false, Some("not a git repository; --worktree requires one"))?;
            crate::state::record_event(conn, "task.failed", &format!("#{id} not a git repository"))?;
            remember_failure(conn, id, opts.agent, opts.description, "not a git repository; --worktree requires one");
            return get(conn, id)?.context("task disappeared after being created");
        };
        let worktree_path = ctx.dirs.state_dir().join("worktrees").join(format!("task-{id}"));
        let branch = format!("single/task-{id}");
        if let Err(e) = single_core::worktree::add(Path::new(&repo_root), &worktree_path, &branch) {
            finish(conn, id, TaskStatus::Failed, None, None, None, false, Some(&format!("worktree setup failed: {e:#}")))?;
            crate::state::record_event(conn, "task.failed", &format!("#{id} worktree setup failed: {e:#}"))?;
            remember_failure(conn, id, opts.agent, opts.description, &format!("worktree setup failed: {e:#}"));
            return get(conn, id)?.context("task disappeared after being created");
        }
        worktree_path
    } else {
        opts.cwd.to_path_buf()
    };

    set_status(conn, id, TaskStatus::Running)?;
    crate::state::record_event(conn, "task.started", &format!("#{id} cwd={}", run_cwd.display()))?;

    // Every run goes against a SingleCLI-managed home, never the real
    // ambient $HOME (single_core::agent_home docs) — either the default
    // per-agent isolated home, or, when --account is given, that named
    // account's own isolated home (single_core::account docs) — unless
    // `real_home` explicitly opts out (see RunTaskOptions::real_home
    // docs), in which case `home` stays None and the agent inherits the
    // daemon's own real environment.
    let home: Option<std::path::PathBuf> = if opts.real_home {
        None
    } else {
        let resolved = match opts.account {
            Some(name) => integrations::home_dir()
                .and_then(|home| single_core::account::ensure_isolated_home(&ctx.dirs.accounts_dir(), &home, opts.agent, name))
                .map_err(|e| format!("failed to materialize isolated home for account '{name}': {e:#}")),
            None => integrations::home_dir()
                .and_then(|home| single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &home, opts.agent))
                .map_err(|e| format!("failed to materialize isolated home for agent '{}': {e:#}", opts.agent)),
        };
        match resolved {
            Ok(dir) => Some(dir),
            Err(detail) => {
                finish(conn, id, TaskStatus::Failed, None, None, None, false, Some(&detail))?;
                crate::state::record_event(conn, "task.failed", &format!("#{id} {detail}"))?;
                remember_failure(conn, id, opts.agent, opts.description, &detail);
                return get(conn, id)?.context("task disappeared after being created");
            }
        }
    };

    std::fs::create_dir_all(ctx.dirs.artifacts_dir())?;
    let live_output_path = ctx.dirs.task_live_output_path(id);
    let outcome = adapter.run_prompt(&run_cwd, opts.description, home.as_deref(), Some(&live_output_path), opts.timeout);
    let _ = std::fs::remove_file(&live_output_path); // superseded by the final artifact below; best-effort cleanup

    let worktree_path_str = opts.use_worktree.then(|| run_cwd.display().to_string());

    match outcome {
        Ok(outcome) => {
            let artifact_path = ctx.dirs.task_artifact_path(id);
            std::fs::write(&artifact_path, format!("{}\n--- stderr ---\n{}", outcome.stdout, outcome.stderr))?;

            let status = if outcome.success { TaskStatus::Completed } else { TaskStatus::Failed };
            let summary = summarize(&outcome.stdout, outcome.timed_out, outcome.exit_code);
            finish(
                conn,
                id,
                status,
                worktree_path_str.as_deref(),
                Some(&artifact_path.display().to_string()),
                outcome.exit_code,
                outcome.timed_out,
                Some(&summary),
            )?;
            crate::state::record_event(conn, if outcome.success { "task.completed" } else { "task.failed" }, &format!("#{id} {summary}"))?;
            if !outcome.success {
                remember_failure(conn, id, opts.agent, opts.description, &summary);
            }
        }
        Err(e) => {
            finish(conn, id, TaskStatus::Failed, worktree_path_str.as_deref(), None, None, false, Some(&format!("{e:#}")))?;
            crate::state::record_event(conn, "task.failed", &format!("#{id} {e:#}"))?;
            remember_failure(conn, id, opts.agent, opts.description, &format!("{e:#}"));
        }
    }

    get(conn, id)?.context("task disappeared after finishing")
}

/// "Learn from errors": every task failure is also written to the memory
/// store (project-scoped, source=tool_output, since it's this project's
/// own tooling reporting what went wrong rather than something a user or
/// agent asserted) so `single memory search`/`single memory list` surface
/// past failures for whoever — human or agent — looks next. Best-effort:
/// a memory-write failure must never mask the real task failure it's
/// trying to record, so errors here are swallowed, not propagated.
fn remember_failure(conn: &Connection, id: i64, agent: &str, description: &str, detail: &str) {
    let _ = memory::ensure_schema(conn);
    let _ = memory::store(conn, memory::NewMemory {
        scope: Some(MemoryScope::Project),
        source: Some(MemorySource::ToolOutput),
        agent: Some(agent.to_string()),
        task: Some(id.to_string()),
        title: format!("task #{id} failed ({agent})"),
        content: format!("prompt: {description}\nfailure: {detail}"),
        confidence: Some(1.0),
        expires_in_seconds: None,
        ..Default::default()
    });
}

fn summarize(stdout: &str, timed_out: bool, exit_code: Option<i32>) -> String {
    if timed_out {
        return "timed out".to_string();
    }
    let first_line = stdout.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let truncated: String = first_line.chars().take(200).collect();
    if truncated.is_empty() {
        format!("exit code {:?}, no output", exit_code)
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        crate::state::ensure_events_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn create_then_get_round_trips_as_created() {
        let conn = test_conn();
        let id = create(&conn, "test task", "claude").unwrap();
        let task = get(&conn, id).unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Created);
        assert_eq!(task.agent, "claude");
    }

    #[test]
    fn run_fails_cleanly_for_unknown_agent() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = test_conn();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() };
        let result = task_run_result(&conn, &ctx, dir.path(), "ghost-agent");
        assert!(result.is_err());
        // No task row should be left dangling from an agent-name check that failed before creation.
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn run_fails_cleanly_when_worktree_requested_outside_a_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = test_conn();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() };

        // "claude" is used here only as a registry entry; if it's not
        // actually installed this test would fail at the detection check
        // before ever reaching the worktree logic, so skip cleanly.
        if !single_agent_sdk::adapters::for_agent("claude").unwrap().discover().detected {
            return;
        }
        let opts = RunTaskOptions { description: "x", agent: "claude", cwd: dir.path(), use_worktree: true, account: None, real_home: false, timeout: Duration::from_secs(1) };
        let task = run(&conn, &ctx, opts).unwrap();
        assert_eq!(task.status, TaskStatus::Failed);
        assert!(task.summary.unwrap().contains("not a git repository"));

        // "Learn from errors": the failure should also have been written
        // to the memory store, not just the task record.
        let memories = memory::search(&conn, "not a git repository", None, None).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].source, MemorySource::ToolOutput);
        assert_eq!(memories[0].agent.as_deref(), Some("claude"));
    }

    fn task_run_result(conn: &Connection, ctx: &Context, cwd: &Path, agent: &str) -> Result<TaskRecord> {
        run(conn, ctx, RunTaskOptions { description: "x", agent, cwd, use_worktree: false, account: None, real_home: false, timeout: Duration::from_secs(1) })
    }
}
