//! `goals` + `graph_nodes` tables (spec §3.2 / §3.3). one goal = one unit
//! of "do this", submitted into a session. the `TaskGraph` lives both as
//! `goals.plan_json` (human-readable mirror) and as `graph_nodes` rows
//! (authoritative for status, so the scheduler can query ready-sets with
//! SQL and cross-session status reads stay cheap).

use crate::coordinator::graph::{Effort, GoalMode, GoalStatus, Node, NodeKind, NodeStatus, TaskGraph};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct Goal {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub mode: GoalMode,
    pub status: GoalStatus,
    pub max_dispatches: u32,
    pub max_minutes: u32,
    pub dispatches: u32,
    pub supervisor_patches: u32,
    pub plan_json: String,
    pub result_summary: Option<String>,
    pub blocked_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// E28 spec §8: which providers/pools are spent — set alongside
    /// `WaitingOnCapacity`, cleared on resume.
    pub capacity_reason: Option<String>,
    /// E28 spec §8: unix-ms ETA for the earliest-recovering candidate;
    /// paired with `capacity_reason`.
    pub earliest_retry_at_ms: Option<i64>,
    /// E28 spec §8: how many times this goal has entered
    /// `WaitingOnCapacity` — checked against `max_capacity_waits_per_goal`
    /// (or `capacity_budget_override`) before finally giving up to `Blocked`.
    pub capacity_waits: u32,
    /// E28 spec §8: `single goal amend <id> capacity-budget=N` override of
    /// `CoordinatorConfig::max_capacity_waits_per_goal` for this goal only.
    pub capacity_budget_override: Option<u32>,
}

pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            text TEXT NOT NULL,
            mode TEXT NOT NULL DEFAULT 'auto',
            status TEXT NOT NULL DEFAULT 'planning',
            max_dispatches INTEGER NOT NULL DEFAULT 25,
            max_minutes INTEGER NOT NULL DEFAULT 60,
            dispatches INTEGER NOT NULL DEFAULT 0,
            supervisor_patches INTEGER NOT NULL DEFAULT 0,
            plan_json TEXT NOT NULL DEFAULT '{}',
            result_summary TEXT,
            blocked_reason TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        (),
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS graph_nodes (
            goal_id TEXT NOT NULL,
            id TEXT NOT NULL,
            desc TEXT NOT NULL,
            kind TEXT NOT NULL,
            effort TEXT NOT NULL,
            agent TEXT NOT NULL DEFAULT '',
            depends_on TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'pending',
            task_id INTEGER,
            attempts INTEGER NOT NULL DEFAULT 0,
            worktree INTEGER NOT NULL DEFAULT 0,
            output_ref TEXT,
            PRIMARY KEY (goal_id, id)
        )",
        (),
    )?;
    // E28 spec §8/§12 (Part D, auto-continue): additive columns, via
    // `add_column_if_missing` per the plan (same helper `task.rs`'s
    // `tasks` table migrations use) — no rewrite of either table.
    crate::task::add_column_if_missing(conn, "goals", "capacity_reason", "TEXT")?;
    crate::task::add_column_if_missing(conn, "goals", "earliest_retry_at_ms", "INTEGER")?;
    crate::task::add_column_if_missing(conn, "goals", "capacity_waits", "INTEGER NOT NULL DEFAULT 0")?;
    crate::task::add_column_if_missing(conn, "goals", "capacity_budget_override", "INTEGER")?;
    crate::task::add_column_if_missing(conn, "graph_nodes", "earliest_retry_at_ms", "INTEGER")?;
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    conn: &Connection,
    session_id: &str,
    text: &str,
    mode: GoalMode,
    max_dispatches: u32,
    max_minutes: u32,
) -> Result<Goal> {
    let id = super::short_id("goal");
    let ts = now();
    conn.execute(
        "INSERT INTO goals
         (id, session_id, text, mode, status, max_dispatches, max_minutes, dispatches,
          supervisor_patches, plan_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'planning', ?5, ?6, 0, 0, '{}', ?7, ?7)",
        params![id, session_id, text, mode.as_str(), max_dispatches, max_minutes, ts],
    )?;
    super::session::set_title_if_empty(conn, session_id, &truncate(text, 80))?;
    super::events::append(
        conn,
        session_id,
        Some(&id),
        super::events::EventKind::Message,
        &format!("goal submitted: {}", truncate(text, 200)),
    )?;
    get(conn, &id)?.context("goal disappeared right after insert")
}

fn row_to_goal(row: &rusqlite::Row) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        text: row.get("text")?,
        mode: GoalMode::parse(&row.get::<_, String>("mode")?).unwrap_or(GoalMode::Auto),
        status: GoalStatus::parse(&row.get::<_, String>("status")?).unwrap_or(GoalStatus::Planning),
        max_dispatches: row.get("max_dispatches")?,
        max_minutes: row.get("max_minutes")?,
        dispatches: row.get("dispatches")?,
        supervisor_patches: row.get("supervisor_patches")?,
        plan_json: row.get("plan_json")?,
        result_summary: row.get("result_summary")?,
        blocked_reason: row.get("blocked_reason")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        capacity_reason: row.get("capacity_reason")?,
        earliest_retry_at_ms: row.get("earliest_retry_at_ms")?,
        capacity_waits: row.get("capacity_waits")?,
        capacity_budget_override: row.get("capacity_budget_override")?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Goal>> {
    Ok(conn.query_row("SELECT * FROM goals WHERE id = ?1", [id], row_to_goal).optional()?)
}

/// all goals, optionally scoped to one session, newest first.
pub fn list(conn: &Connection, session_id: Option<&str>) -> Result<Vec<Goal>> {
    let mut stmt = match session_id {
        Some(_) => conn.prepare("SELECT * FROM goals WHERE session_id = ?1 ORDER BY created_at DESC")?,
        None => conn.prepare("SELECT * FROM goals ORDER BY created_at DESC")?,
    };
    let rows = match session_id {
        Some(s) => stmt.query_map([s], row_to_goal)?,
        None => stmt.query_map([], row_to_goal)?,
    };
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// non-terminal goals — the scheduler tick iterates these. Includes
/// `waiting_on_capacity` (E28 spec §8) so a held goal keeps getting
/// ticked and re-admits its node once the retry stamp passes, instead of
/// going stale the way a genuinely terminal status would.
pub fn active(conn: &Connection) -> Result<Vec<Goal>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM goals WHERE status IN ('planning','running','queued','waiting_on_capacity') ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_goal)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_status(conn: &Connection, id: &str, status: GoalStatus) -> Result<()> {
    conn.execute(
        "UPDATE goals SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now()],
    )?;
    Ok(())
}

pub fn set_blocked(conn: &Connection, id: &str, reason: &str) -> Result<()> {
    conn.execute(
        "UPDATE goals SET status = 'blocked', blocked_reason = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, reason, now()],
    )?;
    Ok(())
}

/// E28 spec §8: moves a goal into `waiting_on_capacity`, recording why and
/// the earliest recovery time, and bumps the `capacity_waits` counter the
/// resume budget is checked against. Never touches node status — the
/// caller (`scheduler::settle_finished_node`) stamps the specific node
/// via `stamp_node_retry` in the same operation.
pub fn set_waiting_on_capacity(conn: &Connection, id: &str, reason: &str, earliest_retry_at_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE goals SET status = 'waiting_on_capacity', capacity_reason = ?2, earliest_retry_at_ms = ?3,
                          capacity_waits = capacity_waits + 1, updated_at = ?4
         WHERE id = ?1",
        params![id, reason, earliest_retry_at_ms, now()],
    )?;
    Ok(())
}

/// E28 spec §8: clears the capacity-wait bookkeeping and moves the goal
/// back to `running` — called when a stamped node's retry time passes and
/// the scheduler actually re-dispatches it (`capacity_resumed`).
pub fn clear_waiting_on_capacity(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE goals SET status = 'running', capacity_reason = NULL, earliest_retry_at_ms = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(())
}

/// E28 spec §8: `single goal amend <id> capacity-budget=N` — raises this
/// goal's own `max_capacity_waits_per_goal` override (reuses the existing
/// `budget=N` amend-text parsing precedent, extended to a second key).
pub fn raise_capacity_budget(conn: &Connection, id: &str, new_budget: u32) -> Result<()> {
    conn.execute(
        "UPDATE goals SET capacity_budget_override = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, new_budget, now()],
    )?;
    Ok(())
}

/// E28 spec §8: stamps (or clears, when `None`) one node's
/// `earliest_retry_at_ms` and sets it back to `Pending` so the next tick's
/// `ready_set_at` re-evaluates it once the stamp passes. A plain `SET`
/// (not `update_node`'s `COALESCE`) since clearing the stamp on eventual
/// success is a real requirement, not just "leave it alone".
pub fn stamp_node_retry(conn: &Connection, goal_id: &str, node_id: &str, earliest_retry_at_ms: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE graph_nodes SET status = 'pending', earliest_retry_at_ms = ?3 WHERE goal_id = ?1 AND id = ?2",
        params![goal_id, node_id, earliest_retry_at_ms],
    )?;
    Ok(())
}

pub fn set_summary(conn: &Connection, id: &str, summary: &str) -> Result<()> {
    conn.execute(
        "UPDATE goals SET result_summary = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, summary, now()],
    )?;
    Ok(())
}

/// raise the per-goal dispatch cap (spec §4.5, via `GoalAmend`) and re-open
/// a blocked goal so the next tick re-evaluates it.
pub fn raise_dispatch_cap(conn: &Connection, id: &str, new_cap: u32) -> Result<()> {
    conn.execute(
        "UPDATE goals SET max_dispatches = ?2,
                          status = CASE WHEN status = 'blocked' THEN 'running' ELSE status END,
                          blocked_reason = NULL, updated_at = ?3
         WHERE id = ?1",
        params![id, new_cap, now()],
    )?;
    Ok(())
}

pub fn bump_dispatches(conn: &Connection, id: &str) -> Result<u32> {
    conn.execute(
        "UPDATE goals SET dispatches = dispatches + 1, updated_at = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(conn.query_row("SELECT dispatches FROM goals WHERE id = ?1", [id], |r| r.get(0))?)
}

pub fn bump_supervisor_patches(conn: &Connection, id: &str) -> Result<u32> {
    conn.execute(
        "UPDATE goals SET supervisor_patches = supervisor_patches + 1, updated_at = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(conn.query_row("SELECT supervisor_patches FROM goals WHERE id = ?1", [id], |r| r.get(0))?)
}

/// writes `plan_json` and replaces every `graph_nodes` row for this goal in
/// one transaction. delete-all-then-insert is fine here — a goal's graph is
/// small (2–8 nodes) and only the coordinator writes it.
pub fn save_graph(conn: &mut Connection, goal_id: &str, graph: &TaskGraph) -> Result<()> {
    let plan_json = serde_json::to_string(graph)?;
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE goals SET plan_json = ?2, updated_at = ?3 WHERE id = ?1",
        params![goal_id, plan_json, now()],
    )?;
    tx.execute("DELETE FROM graph_nodes WHERE goal_id = ?1", [goal_id])?;
    for n in &graph.nodes {
        tx.execute(
            "INSERT INTO graph_nodes
             (goal_id, id, desc, kind, effort, agent, depends_on, status, task_id, attempts, worktree, output_ref, earliest_retry_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                goal_id,
                n.id,
                n.desc,
                n.kind.as_str(),
                n.effort.as_str(),
                n.agent,
                serde_json::to_string(&n.depends_on)?,
                n.status.as_str(),
                n.task_id,
                n.attempts,
                n.worktree as i64,
                n.output_ref,
                n.earliest_retry_at_ms,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn load_graph(conn: &Connection, goal_id: &str) -> Result<TaskGraph> {
    let mut stmt = conn.prepare(
        "SELECT id, desc, kind, effort, agent, depends_on, status, task_id, attempts, worktree, output_ref, earliest_retry_at_ms
         FROM graph_nodes WHERE goal_id = ?1 ORDER BY id ASC",
    )?;
    let nodes = stmt
        .query_map([goal_id], |row| {
            let depends_on: String = row.get("depends_on")?;
            Ok(Node {
                id: row.get("id")?,
                desc: row.get("desc")?,
                kind: NodeKind::parse(&row.get::<_, String>("kind")?).unwrap_or(NodeKind::Code),
                effort: Effort::parse(&row.get::<_, String>("effort")?).unwrap_or(Effort::Standard),
                agent: row.get("agent")?,
                depends_on: serde_json::from_str(&depends_on).unwrap_or_default(),
                status: NodeStatus::parse(&row.get::<_, String>("status")?).unwrap_or(NodeStatus::Pending),
                task_id: row.get("task_id")?,
                attempts: row.get("attempts")?,
                worktree: row.get::<_, i64>("worktree")? != 0,
                output_ref: row.get("output_ref")?,
                earliest_retry_at_ms: row.get("earliest_retry_at_ms")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(TaskGraph { nodes })
}

/// updates one node's mutable fields in place. `None` leaves a field
/// untouched; `status` is always written.
pub fn update_node(
    conn: &Connection,
    goal_id: &str,
    node_id: &str,
    status: NodeStatus,
    task_id: Option<i64>,
    output_ref: Option<&str>,
    attempts: Option<u32>,
) -> Result<()> {
    conn.execute(
        "UPDATE graph_nodes SET
            status = ?3,
            task_id = COALESCE(?4, task_id),
            output_ref = COALESCE(?5, output_ref),
            attempts = COALESCE(?6, attempts)
         WHERE goal_id = ?1 AND id = ?2",
        params![goal_id, node_id, status.as_str(), task_id, output_ref, attempts],
    )?;
    Ok(())
}

/// Rewrites a node's prompt and clears its `task_id`. Used by `careful`
/// mode (`single loop`) to feed each iteration the previous output.
pub fn set_node_desc(conn: &Connection, goal_id: &str, node_id: &str, desc: &str) -> Result<()> {
    conn.execute(
        "UPDATE graph_nodes SET desc = ?3, task_id = NULL WHERE goal_id = ?1 AND id = ?2",
        params![goal_id, node_id, desc],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::graph::Node;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::coordinator::ensure_coordinator_schema(&conn).unwrap();
        conn
    }

    fn node(id: &str) -> Node {
        Node {
            id: id.into(),
            desc: format!("do {id}"),
            kind: NodeKind::Code,
            effort: Effort::Standard,
            agent: String::new(),
            depends_on: vec![],
            status: NodeStatus::Pending,
            task_id: None,
            attempts: 0,
            worktree: true,
            output_ref: None,
            earliest_retry_at_ms: None,
        }
    }

    #[test]
    fn create_goal_persists_and_titles_session() {
        let mut conn = mem();
        let s = super::super::session::new_session(&conn, std::path::Path::new("/tmp/p")).unwrap();
        let g = create(&conn, &s.id, "wire up the widget", GoalMode::Auto, 25, 60).unwrap();
        assert_eq!(g.status, GoalStatus::Planning);
        assert_eq!(g.max_dispatches, 25);

        let title = super::super::session::get(&conn, &s.id).unwrap().unwrap().title;
        assert_eq!(title, "wire up the widget");

        let _ = &mut conn;
    }

    #[test]
    fn save_then_load_graph_roundtrips_node_status() {
        let mut conn = mem();
        let s = super::super::session::new_session(&conn, std::path::Path::new("/tmp/p")).unwrap();
        let g = create(&conn, &s.id, "g", GoalMode::Auto, 25, 60).unwrap();

        let graph = TaskGraph { nodes: vec![node("s1"), node("s2")] };
        save_graph(&mut conn, &g.id, &graph).unwrap();

        update_node(&conn, &g.id, "s1", NodeStatus::Done, Some(42), Some("/tmp/out"), Some(1)).unwrap();
        let loaded = load_graph(&conn, &g.id).unwrap();
        let s1 = loaded.find("s1").unwrap();
        assert_eq!(s1.status, NodeStatus::Done);
        assert_eq!(s1.task_id, Some(42));
        assert_eq!(s1.output_ref.as_deref(), Some("/tmp/out"));
        assert_eq!(loaded.find("s2").unwrap().status, NodeStatus::Pending);
    }

    #[test]
    fn bump_counters_increment_and_cap_raise_unblocks() {
        let conn = mem();
        let s = super::super::session::new_session(&conn, std::path::Path::new("/tmp/p")).unwrap();
        let g = create(&conn, &s.id, "g", GoalMode::Auto, 1, 60).unwrap();

        assert_eq!(bump_dispatches(&conn, &g.id).unwrap(), 1);
        assert_eq!(bump_supervisor_patches(&conn, &g.id).unwrap(), 1);

        set_blocked(&conn, &g.id, "hit the cap").unwrap();
        assert_eq!(get(&conn, &g.id).unwrap().unwrap().status, GoalStatus::Blocked);
        raise_dispatch_cap(&conn, &g.id, 10).unwrap();
        let g2 = get(&conn, &g.id).unwrap().unwrap();
        assert_eq!(g2.status, GoalStatus::Running);
        assert_eq!(g2.max_dispatches, 10);
        assert!(g2.blocked_reason.is_none());
    }
}
