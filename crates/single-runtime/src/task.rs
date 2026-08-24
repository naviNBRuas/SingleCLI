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
use single_protocol::{MemoryScope, MemorySource, TaskRecord, TaskStatus};
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
    // Added for workspace-scoped tasks: a task predating this migration
    // gets '' for both (never NULL — `TaskRecord::cwd`/`workspace_id` are
    // plain `String`s, not `Option`), which the TUI shows as an explicit
    // "(unknown workspace)" bucket rather than silently misattributing it.
    add_column_if_missing(conn, "tasks", "cwd", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "tasks", "workspace_id", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        (),
    )?;
    Ok(())
}

/// SQLite has no `ADD COLUMN IF NOT EXISTS`; check `PRAGMA table_info`
/// first so re-running `ensure_schema` against an already-migrated
/// database (every daemon startup) doesn't error trying to re-add a
/// column that's already there.
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>("name"))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"), ())?;
    }
    Ok(())
}

/// Records (or refreshes) a workspace's last-known display path — called
/// every time a task runs against it, so a project that's been moved on
/// disk self-heals back to showing its current location without anyone
/// having to fix it up by hand.
fn upsert_workspace(conn: &Connection, id: &str, path: &str, name: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO workspaces (id, name, path, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, path = excluded.path, updated_at = excluded.updated_at",
        params![id, name, path, now],
    )?;
    Ok(())
}

/// Every workspace with at least one task, newest activity first — the
/// grouping the TUI's Tasks tab shows before drilling into one workspace's
/// tasks (see `single_protocol::WorkspaceInfo`).
pub fn list_workspaces(conn: &Connection) -> Result<Vec<single_protocol::WorkspaceInfo>> {
    let mut stmt = conn.prepare(
        "SELECT w.id, w.name, w.path, COUNT(t.id) AS task_count, MAX(t.updated_at) AS last_activity_at
         FROM workspaces w JOIN tasks t ON t.workspace_id = w.id
         GROUP BY w.id ORDER BY last_activity_at DESC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(single_protocol::WorkspaceInfo {
                id: row.get("id")?,
                name: row.get("name")?,
                path: row.get("path")?,
                task_count: row.get("task_count")?,
                last_activity_at: row.get("last_activity_at")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn status_as_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Created => "created",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn parse_status(s: &str) -> Result<TaskStatus> {
    Ok(match s {
        "created" => TaskStatus::Created,
        "running" => TaskStatus::Running,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        "cancelled" => TaskStatus::Cancelled,
        other => anyhow::bail!("unknown task status: {other}"),
    })
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<TaskRecord> {
    let status_str: String = row.get("status")?;
    Ok(TaskRecord {
        id: row.get("id")?,
        description: row.get("description")?,
        agent: row.get("agent")?,
        status: parse_status(&status_str).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?,
        worktree_path: row.get("worktree_path")?,
        artifact_path: row.get("artifact_path")?,
        exit_code: row.get("exit_code")?,
        timed_out: row.get::<_, i64>("timed_out")? != 0,
        summary: row.get("summary")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        cwd: row.get("cwd")?,
        workspace_id: row.get("workspace_id")?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<TaskRecord>> {
    conn.query_row(
        "SELECT * FROM tasks WHERE id = ?1",
        params![id],
        row_to_task,
    )
    .optional()
    .context("querying task by id")
}

pub fn list(conn: &Connection) -> Result<Vec<TaskRecord>> {
    let mut stmt = conn.prepare("SELECT * FROM tasks ORDER BY created_at DESC")?;
    let rows = stmt
        .query_map([], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Per-agent activity for the Usage page's "connected agents" table (see
/// `single_protocol::AgentLocalStats`) — every OAuth-authenticated agent
/// has no billing-API `$` data, so this is the only real signal available
/// for them: how often they've actually run, for how long, and when last.
/// Computed in Rust from `list()` rather than SQL date arithmetic, since
/// `created_at`/`updated_at` are RFC3339 strings and task volume here is
/// small enough that loading them all first is simpler than getting
/// SQLite's date functions to agree with `chrono`'s formatting.
pub fn local_stats_by_agent(conn: &Connection) -> Result<Vec<single_protocol::AgentLocalStats>> {
    use std::collections::BTreeMap;

    let tasks = list(conn)?;
    let mut by_agent: BTreeMap<String, Vec<&TaskRecord>> = BTreeMap::new();
    for task in &tasks {
        by_agent.entry(task.agent.clone()).or_default().push(task);
    }

    let mut stats = Vec::new();
    for (agent, records) in by_agent {
        let run_count = records.len() as u64;
        let mut total_ms: u64 = 0;
        let mut counted = 0u64;
        let mut last_run_at: Option<String> = None;
        for record in &records {
            if let (Ok(created), Ok(updated)) = (
                chrono::DateTime::parse_from_rfc3339(&record.created_at),
                chrono::DateTime::parse_from_rfc3339(&record.updated_at),
            ) {
                let delta = (updated - created).num_milliseconds();
                if delta >= 0 {
                    total_ms += delta as u64;
                    counted += 1;
                }
            }
            if last_run_at
                .as_deref()
                .is_none_or(|last| record.updated_at.as_str() > last)
            {
                last_run_at = Some(record.updated_at.clone());
            }
        }
        let avg_duration_ms = if counted > 0 { total_ms / counted } else { 0 };
        stats.push(single_protocol::AgentLocalStats {
            agent,
            run_count,
            avg_duration_ms,
            last_run_at,
        });
    }
    Ok(stats)
}

fn create(conn: &Connection, description: &str, agent: &str, cwd: &str, workspace_id: &str) -> Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (description, agent, status, timed_out, created_at, updated_at, cwd, workspace_id)
         VALUES (?1, ?2, ?3, 0, ?4, ?4, ?5, ?6)",
        params![description, agent, status_as_str(TaskStatus::Created), now, cwd, workspace_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// `create()` plus the workspace bookkeeping every real call site needs:
/// resolves `cwd`'s stable workspace identity (survives the project
/// directory moving — see `stable_workspace_id`'s doc comment) and
/// records/refreshes that workspace's last-known display path.
fn create_for_cwd(conn: &Connection, description: &str, agent: &str, cwd: &Path) -> Result<i64> {
    let workspace_id = single_core::project_context::stable_workspace_id(cwd);
    let display_path = single_core::project_context::resolve(cwd).repo_root.unwrap_or_else(|| cwd.display().to_string());
    let name = single_core::project_context::workspace_display_name(&display_path);
    let id = create(conn, description, agent, &cwd.display().to_string(), &workspace_id)?;
    upsert_workspace(conn, &workspace_id, &display_path, &name)?;
    Ok(id)
}

/// Persists a graph node that deliberately did not run. `Cancelled` is used
/// rather than inventing a graph-only status; the summary makes clear this
/// was a dependency condition, not an explicit user cancellation.
pub fn record_conditionally_skipped(
    conn: &Connection,
    description: &str,
    agent: &str,
    cwd: &Path,
    summary: &str,
) -> Result<TaskRecord> {
    let id = create_for_cwd(conn, description, agent, cwd)?;
    finish(
        conn,
        id,
        TaskStatus::Cancelled,
        None,
        None,
        None,
        false,
        Some(summary),
    )?;
    crate::state::record_event(conn, "task.cancelled", &format!("#{id} {summary}"))?;
    get(conn, id)?.context("task disappeared after being recorded as skipped")
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
    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status_as_str(status), now, id],
    )?;
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
    /// Skips the relevant-memory + unread-notes prompt preamble (on by
    /// default — see `build_context_preamble`).
    pub no_memory_context: bool,
    pub timeout: Duration,
    /// Opt-in (default off): fail over to the next entry in a configured
    /// fallback chain (`single_core::fallback`) when this run's failure
    /// looks like a rate limit — see `execute`'s `maybe_fail_over`.
    pub allow_fallback: bool,
}

/// Cap on the injected memory/notes/knowledge preamble so it can't dwarf
/// the actual prompt — same discipline as `orchestrate::MAX_HANDOFF_CHARS`.
const MAX_CONTEXT_CHARS: usize = 4000;
/// How many past memory entries get pulled into the preamble, at most.
const MAX_CONTEXT_MEMORIES: usize = 5;
/// How many knowledge-graph entities get pulled into the preamble, at most.
const MAX_CONTEXT_KG_ENTITIES: usize = 5;

/// Builds the prompt actually sent to the agent: relevant memory, relevant
/// knowledge-graph entities, and any unread notes addressed to it in this
/// project, prepended ahead of `description`. Best-effort and additive — a
/// lookup failure here must never block the task itself, so errors are
/// swallowed and just result in a smaller (or absent) preamble. Delivered
/// notes are marked read as part of this call so the same note isn't
/// re-delivered on the next run. Works for every agent, including ones
/// with no MCP support, since it needs no cooperation from the agent CLI
/// itself — the single-mcp gateway's live `notes_leave`/`notes_read` tools
/// (v0.1.17) are the complementary on-demand path for agents that want to
/// pull/push notes mid-session instead of only at the start of a run.
fn build_context_preamble(
    conn: &Connection,
    description: &str,
    agent: &str,
    project: Option<&str>,
) -> String {
    let mut sections = String::new();

    let mut memories = Vec::new();
    for keyword in significant_keywords(description) {
        if let Ok(hits) = memory::search(conn, &keyword, None, project) {
            for hit in hits {
                if !memories
                    .iter()
                    .any(|m: &single_protocol::MemoryEntry| m.id == hit.id)
                {
                    memories.push(hit);
                }
            }
        }
    }
    memories.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    memories.truncate(MAX_CONTEXT_MEMORIES);
    if !memories.is_empty() {
        sections.push_str("--- Relevant memory (from past sessions) ---\n");
        for m in &memories {
            sections.push_str(&format!(
                "- [{}] {}: {}\n",
                m.created_at,
                m.title,
                crate::orchestrate::truncate(&m.content, 400)
            ));
        }
        sections.push_str("--- end memory ---\n\n");
    }

    // Shared blackboard state: the knowledge graph is written manually
    // (`single memory graph create-entity`/`add-observation`) by humans or
    // agents that choose to, not auto-populated from task output — see
    // knowledge_graph.rs's module doc. This is the read half: whatever's
    // there that's relevant gets surfaced to every agent automatically,
    // the same way memory/notes already are.
    let mut kg_entities = Vec::new();
    for keyword in significant_keywords(description) {
        if let Ok(hits) = crate::knowledge_graph::query(conn, &keyword) {
            for hit in hits {
                if !kg_entities
                    .iter()
                    .any(|e: &single_protocol::KgEntity| e.name == hit.name)
                {
                    kg_entities.push(hit);
                }
            }
        }
    }
    kg_entities.truncate(MAX_CONTEXT_KG_ENTITIES);
    if !kg_entities.is_empty() {
        sections.push_str("--- Shared knowledge (from the team's knowledge graph) ---\n");
        for e in &kg_entities {
            let observations = e.observations.join("; ");
            sections.push_str(&format!(
                "- {} ({}): {}\n",
                e.name,
                e.entity_type,
                crate::orchestrate::truncate(&observations, 300)
            ));
        }
        sections.push_str("--- end shared knowledge ---\n\n");
    }

    if let Ok(notes) = single_core::notes::inbox(conn, project, agent, true) {
        if !notes.is_empty() {
            sections.push_str("--- Notes left by other agents ---\n");
            for n in &notes {
                sections.push_str(&format!(
                    "- from {} [{}]: {}\n",
                    n.from_agent,
                    n.topic,
                    crate::orchestrate::truncate(&n.content, 400)
                ));
                let _ = single_core::notes::mark_read(conn, n.id);
            }
            sections.push_str("--- end notes ---\n\n");
        }
    }

    if sections.is_empty() {
        return description.to_string();
    }
    format!(
        "{}Task: {description}",
        crate::orchestrate::truncate(&sections, MAX_CONTEXT_CHARS)
    )
}

/// Pulls a few distinctive words out of a task description to drive the
/// (substring, not semantic — see `memory` module docs) memory search: too
/// short/common a word matches everything and defeats the point of
/// "relevant". Capped at 3 so `build_context_preamble` doesn't fan out
/// into an unbounded number of searches.
fn significant_keywords(description: &str) -> Vec<String> {
    description
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(|w| w.to_lowercase())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .take(3)
        .collect()
}

/// Runs a task to completion synchronously: creates the record, optionally
/// isolates it in a new git worktree off `cwd`'s repo, invokes the agent's
/// real non-interactive mode, captures the output as an artifact file, and
/// records the final status. Every stage emits an event via
/// `crate::state::record_event` so the run is auditable (spec section 32)
/// even though there's no live event *stream* yet (spec section 25) — only
/// a persisted log.
pub fn run(conn: &Connection, ctx: &Context, opts: RunTaskOptions) -> Result<TaskRecord> {
    let Some(adapter) = for_agent_with_custom(opts.agent, &ctx.dirs.agents_dir(), &ctx.registry)
    else {
        anyhow::bail!("unknown agent: {}", opts.agent);
    };
    if !adapter.discover().detected {
        anyhow::bail!(
            "agent '{}' is not installed; run `single setup --yes` first",
            opts.agent
        );
    }

    let id = create_for_cwd(conn, opts.description, opts.agent, opts.cwd)?;
    crate::state::record_event(
        conn,
        "task.created",
        &format!("#{id} agent={} \"{}\"", opts.agent, opts.description),
    )?;
    execute(conn, ctx, id, &opts, None)
}

/// Owned counterpart to `RunTaskOptions`, needed because a `background`
/// run's actual work happens on a detached `std::thread` outlasting the
/// request handler's stack frame — the borrowed strings/paths in
/// `RunTaskOptions<'a>` can't cross that boundary, so `run_background`
/// takes this instead and borrows from it fresh inside the spawned thread.
pub struct OwnedRunTaskOptions {
    pub description: String,
    pub agent: String,
    pub cwd: std::path::PathBuf,
    pub use_worktree: bool,
    pub account: Option<String>,
    pub real_home: bool,
    pub no_memory_context: bool,
    pub timeout: Duration,
    pub allow_fallback: bool,
}

impl OwnedRunTaskOptions {
    fn as_borrowed(&self) -> RunTaskOptions<'_> {
        RunTaskOptions {
            description: &self.description,
            agent: &self.agent,
            cwd: &self.cwd,
            use_worktree: self.use_worktree,
            account: self.account.as_deref(),
            real_home: self.real_home,
            no_memory_context: self.no_memory_context,
            timeout: self.timeout,
            allow_fallback: self.allow_fallback,
        }
    }
}

/// Non-blocking counterpart to `run`: creates the task record (fast — one
/// INSERT) and returns it immediately with status `Running`, having handed
/// the actual agent invocation to a detached thread with its own SQLite
/// connection (SQLite connections aren't shareable across threads — same
/// discipline `orchestrate::run_parallel` already uses). Registers a
/// cancel flag in `registry` before spawning so a later `TaskCancel{id}`
/// can find and flip it; `single-agent-sdk::run::run_command_live`'s poll
/// loop notices the flip the same way it already notices a timeout.
pub fn run_background(
    ctx: &Context,
    opts: OwnedRunTaskOptions,
    registry: crate::registry::TaskRegistry,
) -> Result<TaskRecord> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    ensure_schema(&conn)?;

    let Some(adapter) = for_agent_with_custom(&opts.agent, &ctx.dirs.agents_dir(), &ctx.registry)
    else {
        anyhow::bail!("unknown agent: {}", opts.agent);
    };
    if !adapter.discover().detected {
        anyhow::bail!(
            "agent '{}' is not installed; run `single setup --yes` first",
            opts.agent
        );
    }

    let id = create_for_cwd(&conn, &opts.description, &opts.agent, &opts.cwd)?;
    crate::state::record_event(
        &conn,
        "task.created",
        &format!(
            "#{id} agent={} \"{}\" (background)",
            opts.agent, opts.description
        ),
    )?;
    let cancel_flag = registry.register(id);
    set_status(&conn, id, TaskStatus::Running)?;

    let ctx = ctx.clone();
    std::thread::spawn(move || {
        if let Ok(thread_conn) = crate::state::open(&ctx.dirs.db_path()) {
            let _ = execute(
                &thread_conn,
                &ctx,
                id,
                &opts.as_borrowed(),
                Some(cancel_flag.as_ref()),
            );
        }
        registry.unregister(id);
    });

    get(&conn, id)?.context("task disappeared after being created")
}

/// Removes a finished task's git worktree and any leftover live-output
/// file — never done automatically, since a worktree might still be worth
/// inspecting right after a run. Errors on a task that's still `Running`
/// rather than silently killing a worktree out from under a live agent.
pub fn cleanup(conn: &Connection, ctx: &Context, id: i64) -> Result<()> {
    let task = get(conn, id)?.context("no such task")?;
    if task.status == TaskStatus::Running || task.status == TaskStatus::Created {
        anyhow::bail!(
            "task #{id} is still {}; cancel it first",
            status_as_str(task.status)
        );
    }
    if let Some(worktree_path) = &task.worktree_path {
        let path = Path::new(worktree_path);
        if let Some(repo_ctx) = single_core::project_context::resolve(path).repo_root {
            let _ = single_core::worktree::remove(Path::new(&repo_ctx), path, true);
        }
    }
    let _ = std::fs::remove_file(ctx.dirs.task_live_output_path(id));
    Ok(())
}

/// The actual work of running one task against an already-created `id`:
/// optionally isolates it in a new git worktree, invokes the agent's real
/// non-interactive mode (threading `cancel` through so a background run
/// can be stopped early), captures the output as an artifact file, and
/// records the final status. Shared by both the synchronous `run` (always
/// `cancel: None`, since nothing outlives the blocking call that could
/// flip it) and `run_background`.
fn execute(
    conn: &Connection,
    ctx: &Context,
    id: i64,
    opts: &RunTaskOptions,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<TaskRecord> {
    let Some(adapter) = for_agent_with_custom(opts.agent, &ctx.dirs.agents_dir(), &ctx.registry)
    else {
        anyhow::bail!("unknown agent: {}", opts.agent);
    };

    let run_cwd: std::path::PathBuf = if opts.use_worktree {
        let repo_ctx = single_core::project_context::resolve(opts.cwd);
        let Some(repo_root) = repo_ctx.repo_root else {
            finish(
                conn,
                id,
                TaskStatus::Failed,
                None,
                None,
                None,
                false,
                Some("not a git repository; --worktree requires one"),
            )?;
            crate::state::record_event(
                conn,
                "task.failed",
                &format!("#{id} not a git repository"),
            )?;
            remember_failure(
                conn,
                id,
                opts.agent,
                None,
                opts.description,
                "not a git repository; --worktree requires one",
            );
            return get(conn, id)?.context("task disappeared after being created");
        };
        let worktree_path = ctx
            .dirs
            .state_dir()
            .join("worktrees")
            .join(format!("task-{id}"));
        let branch = format!("single/task-{id}");
        if let Err(e) = single_core::worktree::add(Path::new(&repo_root), &worktree_path, &branch) {
            finish(
                conn,
                id,
                TaskStatus::Failed,
                None,
                None,
                None,
                false,
                Some(&format!("worktree setup failed: {e:#}")),
            )?;
            crate::state::record_event(
                conn,
                "task.failed",
                &format!("#{id} worktree setup failed: {e:#}"),
            )?;
            remember_failure(
                conn,
                id,
                opts.agent,
                Some(repo_root.clone()),
                opts.description,
                &format!("worktree setup failed: {e:#}"),
            );
            return get(conn, id)?.context("task disappeared after being created");
        }
        worktree_path
    } else {
        opts.cwd.to_path_buf()
    };

    // Resolved once and reused by every `remember_failure` call below so a
    // task's failures land in the same project scope its later memory
    // searches will be filtered by (`single memory search --project ...`,
    // and the memory/notes preamble task runs inject going forward).
    let project = single_core::project_context::resolve(&run_cwd).repo_root;

    set_status(conn, id, TaskStatus::Running)?;
    crate::state::record_event(
        conn,
        "task.started",
        &format!("#{id} cwd={}", run_cwd.display()),
    )?;

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
                .and_then(|home| {
                    single_core::account::ensure_isolated_home(
                        &ctx.dirs.accounts_dir(),
                        &home,
                        opts.agent,
                        name,
                    )
                })
                .map_err(|e| {
                    format!("failed to materialize isolated home for account '{name}': {e:#}")
                }),
            None => integrations::home_dir()
                .and_then(|home| {
                    single_core::agent_home::ensure_bootstrapped(
                        &ctx.dirs.homes_dir(),
                        &home,
                        opts.agent,
                    )
                })
                .map_err(|e| {
                    format!(
                        "failed to materialize isolated home for agent '{}': {e:#}",
                        opts.agent
                    )
                }),
        };
        match resolved {
            Ok(dir) => Some(dir),
            Err(detail) => {
                finish(
                    conn,
                    id,
                    TaskStatus::Failed,
                    None,
                    None,
                    None,
                    false,
                    Some(&detail),
                )?;
                crate::state::record_event(conn, "task.failed", &format!("#{id} {detail}"))?;
                remember_failure(
                    conn,
                    id,
                    opts.agent,
                    project.clone(),
                    opts.description,
                    &detail,
                );
                return get(conn, id)?.context("task disappeared after being created");
            }
        }
    };

    let prompt = if opts.no_memory_context {
        opts.description.to_string()
    } else {
        build_context_preamble(conn, opts.description, opts.agent, project.as_deref())
    };

    // Opt-in Docker execution (single_core::docker) — only reachable when
    // the isolated-home path above actually ran (real_home skips both).
    let docker_container = if let Some(home) = &home {
        match single_core::docker::is_enabled(
            &ctx.dirs.docker_registry_file(),
            opts.agent,
            opts.account,
        ) {
            Ok(true) => {
                let container = single_core::docker::container_name(opts.agent, opts.account);
                match crate::docker::ensure_started(
                    &container,
                    crate::docker::DEFAULT_IMAGE,
                    home,
                    &run_cwd,
                ) {
                    Ok(()) => Some(container),
                    Err(e) => {
                        let detail = format!(
                            "failed to start docker container for '{}': {e:#}",
                            opts.agent
                        );
                        finish(
                            conn,
                            id,
                            TaskStatus::Failed,
                            None,
                            None,
                            None,
                            false,
                            Some(&detail),
                        )?;
                        crate::state::record_event(
                            conn,
                            "task.failed",
                            &format!("#{id} {detail}"),
                        )?;
                        remember_failure(
                            conn,
                            id,
                            opts.agent,
                            project.clone(),
                            opts.description,
                            &detail,
                        );
                        return get(conn, id)?.context("task disappeared after being created");
                    }
                }
            }
            Ok(false) => None,
            Err(_) => None, // no docker.toml / unreadable — treat as not configured, don't fail the task
        }
    } else {
        None
    };
    // Provider API keys (single_core::provider_keys) an agent that
    // authenticates via a plain env var — not OAuth — needs to actually
    // see. Resolved fresh per run rather than cached: cheap (local
    // keychain lookups only), and picks up a key the user just added
    // without needing a daemon restart.
    let extra_env = single_core::provider_keys::resolve_env_for_agent(&ctx.dirs, opts.agent);
    let backend = match &docker_container {
        Some(container) => single_agent_sdk::backend::ExecBackend::Docker {
            container,
            workdir: &run_cwd,
            extra_env: Some(&extra_env),
        },
        None => single_agent_sdk::backend::ExecBackend::host_with_env(home.as_deref(), &extra_env),
    };

    std::fs::create_dir_all(ctx.dirs.artifacts_dir())?;
    let live_output_path = ctx.dirs.task_live_output_path(id);
    let outcome = adapter.run_prompt(
        &run_cwd,
        &prompt,
        &backend,
        Some(&live_output_path),
        opts.timeout,
        cancel,
    );
    // Left in place (not deleted here) so a task's live output stays
    // inspectable right after it finishes, not just while it's running —
    // an explicit `TaskCleanup{id}` removes it once the caller is done
    // looking, see `cleanup` above.

    let worktree_path_str = opts.use_worktree.then(|| run_cwd.display().to_string());

    // Set inside the failure arms below (never on success/cancellation),
    // then checked once after the match — see `maybe_fail_over`.
    let mut failure_text: Option<String> = None;

    match outcome {
        Ok(outcome) => {
            let artifact_path = ctx.dirs.task_artifact_path(id);
            std::fs::write(
                &artifact_path,
                format!("{}\n--- stderr ---\n{}", outcome.stdout, outcome.stderr),
            )?;

            let status = if outcome.cancelled {
                TaskStatus::Cancelled
            } else if outcome.success {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed
            };
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
            let event = if outcome.cancelled {
                "task.cancelled"
            } else if outcome.success {
                "task.completed"
            } else {
                "task.failed"
            };
            crate::state::record_event(conn, event, &format!("#{id} {summary}"))?;
            // A cancellation was requested, not a real failure — no
            // lesson to learn from it, so it skips `remember_failure`.
            if !outcome.success && !outcome.cancelled {
                remember_failure(
                    conn,
                    id,
                    opts.agent,
                    project.clone(),
                    opts.description,
                    &summary,
                );
                failure_text = Some(format!("{}\n{}\n{summary}", outcome.stdout, outcome.stderr));
            }
        }
        Err(e) => {
            finish(
                conn,
                id,
                TaskStatus::Failed,
                worktree_path_str.as_deref(),
                None,
                None,
                false,
                Some(&format!("{e:#}")),
            )?;
            crate::state::record_event(conn, "task.failed", &format!("#{id} {e:#}"))?;
            remember_failure(
                conn,
                id,
                opts.agent,
                project.clone(),
                opts.description,
                &format!("{e:#}"),
            );
            failure_text = Some(format!("{e:#}"));
        }
    }

    if opts.allow_fallback {
        if let Some(text) = failure_text {
            maybe_fail_over(conn, ctx, id, opts, &text, cancel);
        }
    }

    get(conn, id)?.context("task disappeared after finishing")
}

/// One hop of `task run --allow-fallback`: if `id`'s failure looks like a
/// rate limit (the account was already marked `rate_limited`, or the
/// output matches `single_core::ratelimit`'s generic signals) and a
/// fallback chain has an entry after `opts.agent`/`opts.account`, marks
/// the account rate-limited (if not already) and runs a linked follow-up
/// task against the chain's next entry. The follow-up also carries
/// `allow_fallback: true`, so a chain of several hops walks itself one
/// link at a time via this same function — bounded automatically, since
/// `single_core::fallback::next_after` only ever advances forward through
/// a finite saved chain (see its doc comment for why that can't loop).
/// Errors are logged as an event rather than propagated — a failed
/// failover attempt must never mask the original task's own recorded
/// failure.
fn maybe_fail_over(conn: &Connection, ctx: &Context, id: i64, opts: &RunTaskOptions, failure_text: &str, cancel: Option<&std::sync::atomic::AtomicBool>) {
    let already_rate_limited = opts.account.is_some_and(|account_name| {
        single_core::account::list(&ctx.dirs.accounts_dir(), Some(opts.agent))
            .unwrap_or_default()
            .into_iter()
            .any(|a| a.name == account_name && a.status == single_protocol::AccountStatus::RateLimited)
    });
    if !already_rate_limited && !single_core::ratelimit::looks_like_rate_limit(failure_text) {
        return;
    }

    let current = single_protocol::AgentAccountRef { agent: opts.agent.to_string(), account: opts.account.map(String::from) };
    let fallback_path = ctx.dirs.fallback_registry_file();
    let next = match single_core::fallback::next_after(&fallback_path, &current) {
        Ok(next) => next,
        Err(e) => {
            let _ = crate::state::record_event(conn, "task.fallback_error", &format!("#{id} reading fallback registry: {e:#}"));
            return;
        }
    };
    let Some(next) = next else { return };
    let detected = for_agent_with_custom(&next.agent, &ctx.dirs.agents_dir(), &ctx.registry).is_some_and(|a| a.discover().detected);
    if !detected {
        let _ = crate::state::record_event(conn, "task.fallback_error", &format!("#{id} fallback target '{}' isn't installed", next.agent));
        return;
    }

    if let Some(account) = &opts.account {
        if let Err(e) = single_core::account::set_status(&ctx.dirs.accounts_dir(), opts.agent, account, single_protocol::AccountStatus::RateLimited) {
            let _ = crate::state::record_event(conn, "task.fallback_error", &format!("#{id} marking {}/{account} rate_limited: {e:#}", opts.agent));
        }
    }

    let description = format!("[fallback from #{id}, {} looked rate-limited] {}", opts.agent, opts.description);
    let next_opts = RunTaskOptions {
        description: &description,
        agent: &next.agent,
        cwd: opts.cwd,
        use_worktree: opts.use_worktree,
        account: next.account.as_deref(),
        real_home: opts.real_home,
        no_memory_context: opts.no_memory_context,
        timeout: opts.timeout,
        allow_fallback: true,
    };
    match create_for_cwd(conn, next_opts.description, next_opts.agent, next_opts.cwd) {
        Ok(next_id) => {
            let _ = crate::state::record_event(conn, "task.fallback_started", &format!("#{id} -> #{next_id} agent={}", next.agent));
            let _ = execute(conn, ctx, next_id, &next_opts, cancel);
        }
        Err(e) => {
            let _ = crate::state::record_event(conn, "task.fallback_error", &format!("#{id} starting follow-up task: {e:#}"));
        }
    }
}

/// "Learn from errors": every task failure is also written to the memory
/// store (project-scoped, source=tool_output, since it's this project's
/// own tooling reporting what went wrong rather than something a user or
/// agent asserted) so `single memory search`/`single memory list` surface
/// past failures for whoever — human or agent — looks next. Best-effort:
/// a memory-write failure must never mask the real task failure it's
/// trying to record, so errors here are swallowed, not propagated.
fn remember_failure(
    conn: &Connection,
    id: i64,
    agent: &str,
    project: Option<String>,
    description: &str,
    detail: &str,
) {
    let _ = memory::ensure_schema(conn);
    let _ = memory::store(
        conn,
        memory::NewMemory {
            scope: Some(MemoryScope::Project),
            source: Some(MemorySource::ToolOutput),
            project,
            agent: Some(agent.to_string()),
            task: Some(id.to_string()),
            title: format!("task #{id} failed ({agent})"),
            content: format!("prompt: {description}\nfailure: {detail}"),
            confidence: Some(1.0),
            expires_in_seconds: None,
            ..Default::default()
        },
    );
}

fn summarize(stdout: &str, timed_out: bool, exit_code: Option<i32>) -> String {
    if timed_out {
        return "timed out".to_string();
    }
    let first_line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
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
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        ensure_schema(&conn).unwrap();
        crate::state::ensure_events_schema(&conn).unwrap();
        memory::ensure_schema(&conn).unwrap();
        single_core::notes::ensure_schema(&conn).unwrap();
        crate::knowledge_graph::ensure_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn context_preamble_includes_relevant_memory_and_marks_notes_read() {
        let conn = test_conn();
        memory::store(
            &conn,
            memory::NewMemory {
                scope: Some(MemoryScope::Project),
                project: Some("proj".into()),
                title: "auth bug".into(),
                content: "root cause was a token refresh race".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let note_id = single_core::notes::leave(
            &conn,
            Some("proj".into()),
            "claude",
            Some("codex"),
            "heads up",
            "watch the flaky test",
        )
        .unwrap();

        let prompt =
            build_context_preamble(&conn, "fix the token refresh bug", "codex", Some("proj"));
        assert!(prompt.contains("root cause was a token refresh race"));
        assert!(prompt.contains("watch the flaky test"));
        assert!(prompt.ends_with("Task: fix the token refresh bug"));

        let note = single_core::notes::get(&conn, note_id).unwrap().unwrap();
        assert!(
            note.read_at.is_some(),
            "a note delivered in the preamble should be marked read"
        );
    }

    #[test]
    fn context_preamble_includes_relevant_knowledge_graph_entities() {
        let conn = test_conn();
        crate::knowledge_graph::create_entity(&conn, "token-refresh-race", "bug").unwrap();
        crate::knowledge_graph::add_observation(
            &conn,
            "token-refresh-race",
            "fixed by adding a mutex around refresh",
        )
        .unwrap();
        crate::knowledge_graph::create_entity(&conn, "unrelated-thing", "note").unwrap();

        let prompt =
            build_context_preamble(&conn, "investigate the token refresh race", "codex", None);
        assert!(prompt.contains("token-refresh-race"));
        assert!(prompt.contains("fixed by adding a mutex around refresh"));
        assert!(!prompt.contains("unrelated-thing"));
    }

    #[test]
    fn context_preamble_is_plain_description_when_nothing_relevant_exists() {
        let conn = test_conn();
        let prompt = build_context_preamble(&conn, "some totally unrelated task", "codex", None);
        assert_eq!(prompt, "some totally unrelated task");
    }

    #[test]
    fn significant_keywords_filters_short_words_and_dedupes() {
        let words = significant_keywords("fix the auth auth bug in login flow");
        assert!(words.contains(&"auth".to_string()));
        assert!(words.contains(&"login".to_string()));
        assert!(!words.iter().any(|w| w == "the" || w == "fix" || w == "bug"));
    }

    #[test]
    fn create_then_get_round_trips_as_created() {
        let conn = test_conn();
        let id = create(&conn, "test task", "claude", "/tmp/test-cwd", "workspace-1").unwrap();
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
        let ctx = Context {
            dirs,
            resolved: single_core::ResolvedConfig::default(),
            registry: single_core::builtin_registry(),
        };
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
        let ctx = Context {
            dirs,
            resolved: single_core::ResolvedConfig::default(),
            registry: single_core::builtin_registry(),
        };

        // "claude" is used here only as a registry entry; if it's not
        // actually installed this test would fail at the detection check
        // before ever reaching the worktree logic, so skip cleanly.
        if !single_agent_sdk::adapters::for_agent("claude")
            .unwrap()
            .discover()
            .detected
        {
            return;
        }
        let opts = RunTaskOptions {
            description: "x",
            agent: "claude",
            cwd: dir.path(),
            use_worktree: true,
            account: None,
            real_home: false,
            no_memory_context: false,
            timeout: Duration::from_secs(1),
            allow_fallback: false,
        };
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

    fn task_run_result(
        conn: &Connection,
        ctx: &Context,
        cwd: &Path,
        agent: &str,
    ) -> Result<TaskRecord> {
        run(
            conn,
            ctx,
            RunTaskOptions {
                description: "x",
                agent,
                cwd,
                use_worktree: false,
                account: None,
                real_home: false,
                no_memory_context: false,
                timeout: Duration::from_secs(1),
                allow_fallback: false,
            },
        )
    }

    #[test]
    fn remember_failure_records_the_resolved_project_when_one_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = test_conn();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context {
            dirs,
            resolved: single_core::ResolvedConfig::default(),
            registry: single_core::builtin_registry(),
        };

        if !single_agent_sdk::adapters::for_agent("claude")
            .unwrap()
            .discover()
            .detected
        {
            return;
        }
        // Run with cwd inside this crate's own (real) git repo, and a
        // nonexistent account so home materialization fails before any
        // subprocess spawns — exercises remember_failure's project
        // threading without needing a real agent invocation.
        let this_repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let opts = RunTaskOptions {
            description: "x",
            agent: "claude",
            cwd: &this_repo,
            use_worktree: false,
            account: Some("definitely-not-a-real-account"),
            real_home: false,
            no_memory_context: false,
            timeout: Duration::from_secs(1),
            allow_fallback: false,
        };
        let task = run(&conn, &ctx, opts).unwrap();
        assert_eq!(task.status, TaskStatus::Failed);

        let memories = memory::search(&conn, "definitely-not-a-real-account", None, None).unwrap();
        assert_eq!(memories.len(), 1);
        assert!(
            memories[0].project.is_some(),
            "expected the resolved repo root to be recorded as the memory's project"
        );
    }

    #[test]
    fn fail_over_starts_a_linked_follow_up_task_when_the_chain_has_one() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = test_conn();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context {
            dirs,
            resolved: single_core::ResolvedConfig::default(),
            registry: single_core::builtin_registry(),
        };

        if !single_agent_sdk::adapters::for_agent("claude").unwrap().discover().detected {
            return;
        }
        // Two entries for the same agent (no named account, just the
        // default isolated home) — exercises the chain-walk and follow-up
        // creation without needing a real captured account profile, which
        // `set_status` (called only when `opts.account` is `Some`) would
        // otherwise require to already exist.
        single_core::fallback::set(
            &ctx.dirs.fallback_registry_file(),
            vec![
                single_protocol::AgentAccountRef { agent: "claude".into(), account: None },
                single_protocol::AgentAccountRef { agent: "codex".into(), account: None },
            ],
        )
        .unwrap();

        let this_repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let id = create_for_cwd(&conn, "do the thing", "claude", &this_repo).unwrap();
        let opts = RunTaskOptions {
            description: "do the thing",
            agent: "claude",
            cwd: &this_repo,
            use_worktree: false,
            account: None,
            real_home: false,
            no_memory_context: true,
            timeout: Duration::from_secs(1),
            allow_fallback: true,
        };
        maybe_fail_over(&conn, &ctx, id, &opts, "Error: rate limit exceeded, try again later", None);

        let tasks = list(&conn).unwrap();
        if !single_agent_sdk::adapters::for_agent("codex").unwrap().discover().detected {
            // The chain's next hop isn't installed either — maybe_fail_over
            // correctly declines rather than starting a doomed task.
            assert_eq!(tasks.len(), 1);
            return;
        }
        assert_eq!(tasks.len(), 2, "the original task plus one linked follow-up");
        let follow_up = tasks.iter().find(|t| t.id != id).unwrap();
        assert_eq!(follow_up.agent, "codex");
        assert!(follow_up.description.contains(&format!("fallback from #{id}")));
    }

    #[test]
    fn fail_over_does_nothing_when_the_failure_does_not_look_like_a_rate_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let conn = test_conn();
        let dirs = single_core::SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let ctx = Context {
            dirs,
            resolved: single_core::ResolvedConfig::default(),
            registry: single_core::builtin_registry(),
        };
        single_core::fallback::set(
            &ctx.dirs.fallback_registry_file(),
            vec![
                single_protocol::AgentAccountRef { agent: "claude".into(), account: None },
                single_protocol::AgentAccountRef { agent: "codex".into(), account: None },
            ],
        )
        .unwrap();

        let this_repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let id = create_for_cwd(&conn, "do the thing", "claude", &this_repo).unwrap();
        let opts = RunTaskOptions {
            description: "do the thing",
            agent: "claude",
            cwd: &this_repo,
            use_worktree: false,
            account: None,
            real_home: false,
            no_memory_context: true,
            timeout: Duration::from_secs(1),
            allow_fallback: true,
        };
        maybe_fail_over(&conn, &ctx, id, &opts, "error: file not found", None);

        assert_eq!(list(&conn).unwrap().len(), 1, "an ordinary failure must not trigger a follow-up task");
    }
}
