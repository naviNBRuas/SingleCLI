//! the coordinator: a goal comes in, a deterministic scheduler drives a
//! self-correcting pool of agents and streams progress back. everything
//! below `task::run` (adapters, fallback, isolated homes, worktrees,
//! MCP/LSP, secrets, sqlite) is unchanged — this sits on top of it.
//!
//! design: `docs/superpowers/plans/2026-09-06-e27-coordinator.md` and
//! nbr-workspace `docs/queue/E27-singlecli-followups/02-coordinator-redesign.md`
//! (that spec is authoritative; §11 decisions are all resolved).
//!
//! module split mirrors spec §8: `session` / `goal` / `graph` / `scheduler`
//! / `brain` / `routing` / `events`. the scheduler core is pure so it can
//! be unit-tested with a fake capacity map and scripted task outcomes — no
//! LLM, no subprocess.

pub mod brain;
pub mod events;
pub mod goal;
pub mod graph;
pub mod routing;
pub mod scheduler;
pub mod session;

use crate::context::Context;
use crate::coordinator::routing::{CoordinatorConfig, PoolHealth, RoutingTable};
use anyhow::{Context as _, Result};
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};

/// creates every coordinator table if absent. purely additive — never
/// touches the existing `tasks` / `memory` / `events` tables. safe to call
/// on every daemon start and on every handler that opens a connection,
/// same discipline as `task::ensure_schema`.
pub fn ensure_coordinator_schema(conn: &Connection) -> anyhow::Result<()> {
    session::ensure_schema(conn)?;
    goal::ensure_schema(conn)?; // goals + graph_nodes
    events::ensure_schema(conn)?;
    Ok(())
}

fn load_env(ctx: &Context, conn: &Connection) -> (CoordinatorConfig, RoutingTable, PoolHealth) {
    let cfg = CoordinatorConfig::load(&ctx.dirs);
    let table = RoutingTable::load(&ctx.dirs);
    let health = PoolHealth::probe(&ctx.registry, conn);
    (cfg, table, health)
}

/// The DONE sentinel a `careful`-mode agent emits to end its loop, and the
/// instruction appended to every iteration's prompt.
pub(crate) const CAREFUL_DONE_LINE: &str = "DONE";
const CAREFUL_INSTRUCTION: &str =
    "\n\nWork toward this goal. When it is fully complete, reply with a line containing only DONE.";

/// plans a goal still in `planning` with no graph and moves it to
/// `running`. `careful` mode (`single loop`) skips the LLM planner and
/// builds a single iterating node; every other mode runs the planner.
/// `agent`, when set, pins every node to that agent instead of routing.
/// Safe to call repeatedly — a no-op once a graph exists.
pub fn plan_goal(
    ctx: &Context,
    conn: &mut Connection,
    goal_id: &str,
    agent: Option<&str>,
) -> Result<()> {
    let goal = goal::get(conn, goal_id)?.context("no such goal")?;
    if !goal::load_graph(conn, goal_id)?.nodes.is_empty() {
        return Ok(());
    }
    let (_cfg, table, health) = load_env(ctx, conn);
    let cwd = session::get(conn, &goal.session_id)?
        .map(|s| s.cwd)
        .unwrap_or_else(|| ".".into());

    let graph = if goal.mode == graph::GoalMode::Careful {
        let pinned = agent
            .map(str::to_string)
            .or_else(|| routing::select_agent(&table, graph::NodeKind::Code, graph::Effort::Standard, &health))
            .unwrap_or_default();
        let node = graph::Node {
            id: "s1".into(),
            desc: format!("{}{CAREFUL_INSTRUCTION}", goal.text),
            kind: graph::NodeKind::Code,
            effort: graph::Effort::Standard,
            agent: pinned,
            depends_on: vec![],
            status: graph::NodeStatus::Pending,
            task_id: None,
            attempts: 0,
            worktree: false, // loop iterates in the goal cwd, like the prototype
            output_ref: None,
            earliest_retry_at_ms: None,
        };
        events::append(conn, &goal.session_id, Some(goal_id), events::EventKind::Plan, "careful mode: 1 iterating node")?;
        graph::TaskGraph { nodes: vec![node] }
    } else {
        let mut g = brain::plan(conn, ctx, &goal.text, std::path::Path::new(&cwd), &table, &health)?;
        if let Some(a) = agent {
            for n in &mut g.nodes {
                n.agent = a.to_string();
            }
        }
        events::append(
            conn,
            &goal.session_id,
            Some(goal_id),
            events::EventKind::Plan,
            &format!("planned {} node(s)", g.nodes.len()),
        )?;
        g
    };

    goal::save_graph(conn, goal_id, &graph)?;
    goal::set_status(conn, goal_id, graph::GoalStatus::Running)?;
    Ok(())
}

/// one full scheduling pass across every active goal — reconcile is the
/// caller's job (daemon start). called by the daemon tick timer, after
/// `GoalSubmit`, and after any task finishes.
pub fn drive(ctx: &Context, conn: &mut Connection, registry: &crate::registry::TaskRegistry) -> Result<()> {
    ensure_coordinator_schema(conn)?;
    let (cfg, table, health) = load_env(ctx, conn);
    let dispatcher = scheduler::RealDispatcher { ctx, registry: registry.clone() };
    scheduler::tick(ctx, conn, &cfg, &table, &health, &dispatcher)
}

/// E28 spec §10 (Part F): called once from `server.rs`'s startup block,
/// after the existing `scheduler::reconcile` (which already turns a
/// `graph_nodes.status = 'running'` row with a dead backing task into
/// `failed`/`done` — a crash signature). This catches what that reconcile
/// doesn't:
/// - a `Paused` goal (a *clean* prior `single daemon stop` marked it,
///   distinguishing it from a crash) → back to `running`;
/// - a `Planning`/`Running`/`WaitingOnCapacity` goal with an empty graph
///   → the planner call was interrupted before it ever wrote one →
///   `plan_goal` re-runs it;
/// - any other active, not-all-terminal goal → nothing further to do
///   here: its `Pending` nodes are already visible to the next real
///   `scheduler::tick()` (the periodic timer, moments away), which
///   re-admits them the same way it would on any ordinary tick — no
///   separate re-dispatch logic needed.
///
/// Every goal actually touched gets one `session_resumed` event. Returns
/// the touched count (logged by the caller).
pub fn resume_interrupted(ctx: &Context, conn: &mut Connection) -> Result<usize> {
    ensure_coordinator_schema(conn)?;
    let mut candidates = goal::active(conn)?; // planning/running/queued/waiting_on_capacity
    candidates.extend(goal::list_by_status(conn, graph::GoalStatus::Paused)?);

    let mut touched = 0usize;
    for g in candidates {
        let mut this_touched = false;

        if g.status == graph::GoalStatus::Paused {
            goal::set_status(conn, &g.id, graph::GoalStatus::Running)?;
            this_touched = true;
        }

        let graph = goal::load_graph(conn, &g.id)?;
        if graph.nodes.is_empty() {
            // the planner call was interrupted before a graph ever landed.
            // Best-effort: a re-plan failure here shouldn't abort resuming
            // every other goal, so it's logged and skipped, not propagated.
            match plan_goal(ctx, conn, &g.id, None) {
                Ok(()) => this_touched = true,
                Err(e) => tracing::warn!(goal = %g.id, error = %e, "resume_interrupted: re-plan failed"),
            }
        } else if !graph.is_all_terminal() {
            // Pending nodes (including a capacity-stamped one, whose real
            // cooldown state is untouched) are already visible to the next
            // scheduler tick -- nothing more to do but record the resume.
            this_touched = true;
        }

        if this_touched {
            touched += 1;
            events::append(conn, &g.session_id, Some(&g.id), events::EventKind::SessionResumed, &format!("{}: resumed on daemon start", g.id))?;
        }
    }
    Ok(touched)
}

/// E28 spec §10: a clean `single daemon stop` — called from the
/// `Request::Shutdown` handler, before the process actually exits — marks
/// every currently active goal `Paused` so `resume_interrupted` (not the
/// crash-oriented `scheduler::reconcile`) picks it back up next start.
pub fn pause_active_goals(conn: &Connection) -> Result<usize> {
    ensure_coordinator_schema(conn)?;
    goal::pause_all_active(conn)
}

/// E28 spec §10: `single goal resume <id>` — manual re-tick of a
/// `Blocked`/`Failed`/`Paused`/`WaitingOnCapacity` goal a human judges
/// recoverable. Unlike `resume_interrupted`, this clears any stale
/// per-node capacity stamp (the human is overriding the hold, not
/// discovering the daemon restarted) and re-plans an empty graph the same
/// way. Always writes a `session_resumed` event, even if the goal turns
/// out to already be running (a manual no-op is still worth a record).
pub fn resume_goal(ctx: &Context, conn: &mut Connection, goal_id: &str) -> Result<()> {
    let g = goal::get(conn, goal_id)?.context("no such goal")?;
    goal::resume_status(conn, goal_id)?;
    goal::clear_node_retry_stamps(conn, goal_id)?;

    let graph = goal::load_graph(conn, goal_id)?;
    if graph.nodes.is_empty() {
        plan_goal(ctx, conn, goal_id, None)?;
    }
    events::append(conn, &g.session_id, Some(goal_id), events::EventKind::SessionResumed, &format!("{goal_id}: resumed via `single goal resume`"))?;
    Ok(())
}

/// maps a finished task back to its coordinator node (no-op if it isn't
/// one) and advances the goal.
pub fn notify_task_finished(
    ctx: &Context,
    conn: &mut Connection,
    registry: &crate::registry::TaskRegistry,
    task_id: i64,
) -> Result<()> {
    ensure_coordinator_schema(conn)?;
    let (cfg, table, health) = load_env(ctx, conn);
    let dispatcher = scheduler::RealDispatcher { ctx, registry: registry.clone() };
    scheduler::on_task_finished(ctx, conn, &cfg, &table, &health, &dispatcher, task_id)
}

/// monotonic-ish id like `sess_lz4f9k0q_0007`. not a security token — just
/// needs to be unique across a daemon's lifetime and roughly sortable.
/// time component gives cross-restart uniqueness; the process-local
/// counter disambiguates ids minted in the same nanosecond. no `ulid` /
/// `nanoid` / `rand` dependency exists in this workspace and the spec
/// forbids adding one, so this is hand-rolled.
pub(crate) fn short_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{}_{:04x}", radix36(nanos), n & 0xffff)
}

/// base-36 encode, lowercase — keeps ids compact and copy-pasteable.
fn radix36(mut v: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if v == 0 {
        return "0".into();
    }
    let mut buf = Vec::new();
    while v > 0 {
        buf.push(DIGITS[(v % 36) as usize]);
        v /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn ensure_coordinator_schema_is_idempotent_and_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_coordinator_schema(&conn).unwrap();
        // a second call must not error (every daemon start re-runs it)
        ensure_coordinator_schema(&conn).unwrap();
        for t in ["sessions", "goals", "graph_nodes", "coordinator_events"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {t} missing after ensure_coordinator_schema");
        }
    }

    // ---- E28 spec §10 (Part F): resume_interrupted / pause_active_goals ----

    fn test_ctx(root: &std::path::Path) -> Context {
        let dirs = single_core::SingleDirs::from_root(root.to_path_buf());
        dirs.ensure_created().unwrap();
        Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() }
    }

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_coordinator_schema(&conn).unwrap();
        crate::task::ensure_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn running_goal_with_pending_node_and_no_live_task_is_reticked_and_events_session_resumed() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test_ctx(tmp.path());
        let mut conn = test_conn();
        let s = session::new_session(&conn, tmp.path()).unwrap();
        let g = goal::create(&conn, &s.id, "g", graph::GoalMode::Auto, 25, 60).unwrap();
        let node = graph::Node {
            id: "s1".into(),
            desc: "do it".into(),
            kind: graph::NodeKind::Code,
            effort: graph::Effort::Standard,
            agent: "grok".into(),
            depends_on: vec![],
            status: graph::NodeStatus::Pending,
            task_id: None,
            attempts: 0,
            worktree: false,
            output_ref: None,
            earliest_retry_at_ms: None,
        };
        goal::save_graph(&mut conn, &g.id, &graph::TaskGraph { nodes: vec![node] }).unwrap();
        goal::set_status(&conn, &g.id, graph::GoalStatus::Running).unwrap();

        let touched = resume_interrupted(&ctx, &mut conn).unwrap();
        assert_eq!(touched, 1);

        let events = events::since(&conn, &s.id, 0).unwrap();
        assert!(events.iter().any(|e| e.kind == "session_resumed"), "expected a session_resumed event, got {events:?}");
    }

    #[test]
    fn planning_goal_with_empty_graph_reruns_plan() {
        // Careful mode's `plan_goal` path builds its single-node graph
        // synchronously with no LLM/subprocess call -- a deterministic
        // stand-in for "the planner call was interrupted, re-run it" that
        // doesn't need a real agent installed to test.
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test_ctx(tmp.path());
        let mut conn = test_conn();
        let s = session::new_session(&conn, tmp.path()).unwrap();
        let g = goal::create(&conn, &s.id, "loop it", graph::GoalMode::Careful, 10, 60).unwrap();
        assert!(goal::load_graph(&conn, &g.id).unwrap().nodes.is_empty());

        let touched = resume_interrupted(&ctx, &mut conn).unwrap();
        assert_eq!(touched, 1);

        let reloaded = goal::load_graph(&conn, &g.id).unwrap();
        assert_eq!(reloaded.nodes.len(), 1, "expected the interrupted planner call to be re-run");
        assert_eq!(goal::get(&conn, &g.id).unwrap().unwrap().status, graph::GoalStatus::Running);
    }

    #[test]
    fn paused_goal_is_reticked_on_resume_interrupted() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test_ctx(tmp.path());
        let mut conn = test_conn();
        let s = session::new_session(&conn, tmp.path()).unwrap();
        let g = goal::create(&conn, &s.id, "g", graph::GoalMode::Careful, 10, 60).unwrap();
        plan_goal(&ctx, &mut conn, &g.id, None).unwrap(); // gives it a real graph first
        goal::set_status(&conn, &g.id, graph::GoalStatus::Paused).unwrap();

        let touched = resume_interrupted(&ctx, &mut conn).unwrap();
        assert_eq!(touched, 1);
        assert_eq!(goal::get(&conn, &g.id).unwrap().unwrap().status, graph::GoalStatus::Running);
    }

    #[test]
    fn clean_stop_marks_nonterminal_goals_paused() {
        let conn = test_conn();
        let s = session::new_session(&conn, std::path::Path::new("/tmp")).unwrap();
        let running = goal::create(&conn, &s.id, "r", graph::GoalMode::Auto, 25, 60).unwrap();
        goal::set_status(&conn, &running.id, graph::GoalStatus::Running).unwrap();
        let done = goal::create(&conn, &s.id, "d", graph::GoalMode::Auto, 25, 60).unwrap();
        goal::set_status(&conn, &done.id, graph::GoalStatus::Done).unwrap();

        let touched = pause_active_goals(&conn).unwrap();
        assert_eq!(touched, 1);
        assert_eq!(goal::get(&conn, &running.id).unwrap().unwrap().status, graph::GoalStatus::Paused);
        assert_eq!(goal::get(&conn, &done.id).unwrap().unwrap().status, graph::GoalStatus::Done, "a terminal goal must never be paused");
    }

    #[test]
    fn short_id_is_unique_and_prefixed() {
        let a = short_id("goal");
        let b = short_id("goal");
        assert!(a.starts_with("goal_"));
        assert_ne!(a, b);
    }
}
