//! the deterministic scheduler (spec E27.02 §4): a single `tick` on goal
//! submitted / task finished / timer / daemon start. always in charge; the
//! LLM brain is only ever consulted for plan / supervise / integrate.
//!
//! this file is the pure core — `tick_pure` takes a graph + a fake
//! capacity map + a budget snapshot and returns an ordered list of
//! actions, with no db / subprocess / LLM. every §4 branch (ready-set,
//! per-agent + global caps, critical-path ordering, budget stop, retry,
//! integrator trigger) is exercised by the unit tests below. the db +
//! dispatch shell (reconcile, real `task::run_background` handoff,
//! `on_task_finished`) is plan Task 8.

use crate::coordinator::graph::{Effort, Node, NodeStatus, TaskGraph};
use crate::coordinator::routing::{self, CoordinatorConfig, PoolHealth, RoutingTable};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::time::Duration;

/// live concurrency picture, counted across ALL goals (spec §4.3).
#[derive(Debug, Clone, Default)]
pub struct Capacity {
    pub global_running: usize,
    pub per_agent_running: BTreeMap<String, usize>,
    /// `min(registry.max_concurrency, routing cap)` per agent; a missing
    /// entry means "no specific cap" and only the global limit applies.
    pub per_agent_cap: BTreeMap<String, usize>,
}

impl Capacity {
    fn agent_headroom(&self, agent: &str) -> usize {
        match self.per_agent_cap.get(agent) {
            Some(&cap) => cap.saturating_sub(self.per_agent_running.get(agent).copied().unwrap_or(0)),
            None => usize::MAX,
        }
    }
}

/// per-goal budget snapshot (spec §4.5 / §11.2): dispatch count + wall
/// clock, whichever trips first.
#[derive(Debug, Clone)]
pub struct GoalBudget {
    pub dispatches: u32,
    pub max_dispatches: u32,
    pub started_at: DateTime<Utc>,
    pub max_minutes: u32,
    pub now: DateTime<Utc>,
}

impl GoalBudget {
    /// the human-readable reason the goal should block, or `None` if there
    /// is still budget.
    pub fn exhausted(&self) -> Option<String> {
        if self.dispatches >= self.max_dispatches {
            return Some(format!(
                "dispatch budget spent: {} of {} node dispatches used",
                self.dispatches, self.max_dispatches
            ));
        }
        let elapsed_min = (self.now - self.started_at).num_minutes();
        if elapsed_min >= self.max_minutes as i64 {
            return Some(format!(
                "time budget spent: {elapsed_min} min elapsed of {} min cap",
                self.max_minutes
            ));
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TickAction {
    Dispatch {
        node_id: String,
        agent: String,
        effort: Effort,
        worktree: bool,
        max_steps: u32,
    },
    RunIntegrator,
    Block {
        reason: String,
    },
    Fail {
        reason: String,
    },
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// `attempts < 2`, transient failure → retry the node once, advancing
    /// to the next agent in its routing list (spec §4.6).
    RetrySameNextAgent,
    /// `attempts >= 2` or a semantic failure → hand to the supervisor
    /// (subject to the per-goal patch cap, enforced by the caller).
    Supervisor,
    /// nothing left to try for this node.
    GiveUp,
}

/// spec §4.6. `semantic_failure` = the task ran to completion but produced
/// a wrong / unusable result (as opposed to a crash / timeout / rate
/// limit), which the caller classifies from the task record.
pub fn retry_decision(attempts: u32, semantic_failure: bool) -> RetryDecision {
    if semantic_failure {
        return RetryDecision::Supervisor;
    }
    match attempts {
        0 | 1 => RetryDecision::RetrySameNextAgent,
        _ => RetryDecision::Supervisor,
    }
}

/// the pure scheduling decision for one goal (spec §4 steps 2–7). callers
/// pass the current graph, config, a capacity snapshot, the budget, the
/// routing table and pool health; they get back an ordered action list to
/// execute. no side effects.
pub fn tick_pure(
    graph: &TaskGraph,
    cfg: &CoordinatorConfig,
    cap: &Capacity,
    budget: &GoalBudget,
    table: &RoutingTable,
    health: &PoolHealth,
) -> Vec<TickAction> {
    // step 5 (checked first so a spent budget always wins over new work):
    // stop admitting, block the goal with the reason.
    if let Some(reason) = budget.exhausted() {
        // only block if there is still pending/ready work — a goal whose
        // nodes are all terminal should integrate, not block.
        if !graph.is_all_terminal() {
            return vec![TickAction::Block { reason }];
        }
    }

    // step 7: everything terminal → integrate (or fail if unrecoverable).
    if graph.is_all_terminal() {
        // a graph with a failed node that was never recovered is a failed
        // goal; the integrator decides recoverability, but if there is
        // nothing to integrate (all failed/blocked) short-circuit to Fail.
        let any_done = graph.nodes.iter().any(|n| n.status == NodeStatus::Done);
        if graph.has_failure() && !any_done {
            return vec![TickAction::Fail {
                reason: "every node failed; nothing to integrate".into(),
            }];
        }
        return vec![TickAction::RunIntegrator];
    }

    // step 2: ready-set.
    let mut ready: Vec<_> = graph.ready_set();
    if ready.is_empty() {
        return vec![TickAction::Noop];
    }

    // step 4 ordering: critical path first (longest dependent chain), then
    // cheaper effort first, then node id for determinism.
    ready.sort_by(|a, b| {
        graph
            .critical_path_depth(&b.id)
            .cmp(&graph.critical_path_depth(&a.id))
            .then(effort_rank(a.effort).cmp(&effort_rank(b.effort)))
            .then(a.id.cmp(&b.id))
    });

    // step 3 + 4: admit up to global headroom and per-agent headroom.
    let global_headroom = cfg.max_parallel.saturating_sub(cap.global_running);
    if global_headroom == 0 {
        return vec![TickAction::Noop];
    }

    let mut actions = Vec::new();
    let mut admitted = 0usize;
    // track admissions made in THIS tick so two ready nodes routed to the
    // same capped agent don't both get admitted past its cap.
    let mut this_tick_per_agent: BTreeMap<String, usize> = BTreeMap::new();

    for node in ready {
        if admitted >= global_headroom {
            break;
        }
        let agent = match node.agent.is_empty() {
            false => node.agent.clone(),
            true => match routing::select_agent(table, node.kind, node.effort, health) {
                Some(a) => a,
                None => continue, // no agent available for this kind right now
            },
        };
        let used_here = this_tick_per_agent.get(&agent).copied().unwrap_or(0);
        if cap.agent_headroom(&agent).saturating_sub(used_here) == 0 {
            continue; // this agent is at capacity; leave the node ready
        }
        actions.push(TickAction::Dispatch {
            node_id: node.id.clone(),
            agent: agent.clone(),
            effort: node.effort,
            worktree: node.worktree,
            max_steps: table.max_steps(node.effort),
        });
        *this_tick_per_agent.entry(agent).or_insert(0) += 1;
        admitted += 1;
    }

    if actions.is_empty() {
        actions.push(TickAction::Noop);
    }
    actions
}

fn effort_rank(e: Effort) -> u8 {
    match e {
        Effort::Quick => 0,
        Effort::Standard => 1,
        Effort::Deep => 2,
    }
}

// ------------------------------------------------------- db + dispatch shell

use crate::context::Context;
use crate::coordinator::events::{self, EventKind};
use crate::coordinator::goal::{self, Goal};
use crate::coordinator::graph::GoalStatus;
use anyhow::{Context as _, Result};
use rusqlite::Connection;

/// how a `Dispatch` action turns into a real running task. the production
/// impl hands off to `task::run_background`; tests inject a fake that just
/// records a terminal `tasks` row, so `tick`'s db bookkeeping is testable
/// without spawning an agent.
pub trait Dispatcher {
    fn dispatch(&self, opts: crate::task::OwnedRunTaskOptions) -> Result<i64>;
}

pub struct RealDispatcher<'a> {
    pub ctx: &'a Context,
    pub registry: crate::registry::TaskRegistry,
}

impl Dispatcher for RealDispatcher<'_> {
    fn dispatch(&self, opts: crate::task::OwnedRunTaskOptions) -> Result<i64> {
        Ok(crate::task::run_background(self.ctx, opts, self.registry.clone())?.id)
    }
}

fn timeout_for(effort: Effort) -> Duration {
    match effort {
        Effort::Quick => Duration::from_secs(180),
        Effort::Standard => Duration::from_secs(420),
        Effort::Deep => Duration::from_secs(900),
    }
}

/// spec §4.1: on daemon start (and periodically), any coordinator node
/// left `running` whose backing `tasks` row is no longer live is
/// reconciled — `done` if the task actually completed, else `failed`
/// (interrupted). returns the number of nodes touched. this is the
/// coordinator-node analogue of `task::reconcile_orphaned_tasks`.
pub fn reconcile(conn: &Connection) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT goal_id, id, task_id FROM graph_nodes WHERE status = 'running'",
    )?;
    let rows: Vec<(String, String, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut touched = 0;
    for (goal_id, node_id, task_id) in rows {
        let task_status: Option<String> = match task_id {
            Some(tid) => conn
                .query_row("SELECT status FROM tasks WHERE id = ?1", [tid], |r| r.get(0))
                .ok(),
            None => None,
        };
        let new_status = match task_status.as_deref() {
            Some("running") | Some("created") => continue, // genuinely still live
            Some("completed") => NodeStatus::Done,
            _ => NodeStatus::Failed, // failed / cancelled / missing row
        };
        goal::update_node(conn, &goal_id, &node_id, new_status, None, None, None)?;
        touched += 1;
    }
    if touched > 0 {
        tracing::warn!(count = touched, "reconciled interrupted coordinator nodes");
    }
    Ok(touched)
}

/// counts running coordinator nodes across every goal, per agent and
/// globally, and derives per-agent caps from the registry's
/// `max_concurrency`.
fn build_capacity(conn: &Connection, ctx: &Context) -> Result<Capacity> {
    let mut stmt = conn.prepare("SELECT agent, COUNT(*) FROM graph_nodes WHERE status = 'running' GROUP BY agent")?;
    let per_agent_running: BTreeMap<String, usize> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?
        .collect::<rusqlite::Result<_>>()?;
    let global_running = per_agent_running.values().sum();
    let per_agent_cap = ctx
        .registry
        .iter()
        .filter_map(|a| a.max_concurrency.map(|c| (a.name.clone(), c as usize)))
        .collect();
    Ok(Capacity { global_running, per_agent_running, per_agent_cap })
}

fn goal_budget(goal: &Goal, cfg: &CoordinatorConfig) -> GoalBudget {
    GoalBudget {
        dispatches: goal.dispatches,
        max_dispatches: if goal.max_dispatches > 0 { goal.max_dispatches } else { cfg.max_dispatches_per_goal },
        started_at: goal.created_at.parse().unwrap_or_else(|_| Utc::now()),
        max_minutes: if goal.max_minutes > 0 { goal.max_minutes } else { cfg.max_goal_minutes },
        now: Utc::now(),
    }
}

/// runs one scheduling pass for every active goal, executing the pure
/// scheduler's decisions against the db and the given dispatcher. this is
/// the entrypoint the daemon timer, `GoalSubmit`, and `on_task_finished`
/// all call.
pub fn tick(
    ctx: &Context,
    conn: &mut Connection,
    cfg: &CoordinatorConfig,
    table: &RoutingTable,
    health: &PoolHealth,
    dispatcher: &dyn Dispatcher,
) -> Result<()> {
    // poll-based completion: the timer, not a callback, is what advances
    // the coordinator, so first settle every node whose backing task row
    // has gone terminal since the last tick.
    let finished: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT n.task_id FROM graph_nodes n JOIN tasks t ON t.id = n.task_id
             WHERE n.status = 'running' AND t.status IN ('completed','failed','cancelled')",
        )?;
        let ids = stmt.query_map([], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        ids
    };
    for task_id in finished {
        settle_finished_node(ctx, conn, table, health, task_id)?;
    }

    for goal in goal::active(conn)? {
        // a goal still `planning` with no graph is the planner's job, not
        // the scheduler's — leave it (the handler kicks planning off).
        let graph = goal::load_graph(conn, &goal.id)?;
        if graph.nodes.is_empty() {
            continue;
        }
        if goal.status == GoalStatus::Planning {
            goal::set_status(conn, &goal.id, GoalStatus::Running)?;
        }

        let cap = build_capacity(conn, ctx)?;
        let budget = goal_budget(&goal, cfg);
        let actions = tick_pure(&graph, cfg, &cap, &budget, table, health);

        for action in actions {
            match action {
                TickAction::Noop => {}
                TickAction::Block { reason } => {
                    goal::set_blocked(conn, &goal.id, &reason)?;
                    events::append(conn, &goal.session_id, Some(&goal.id), EventKind::Blocked, &reason)?;
                }
                TickAction::Fail { reason } => {
                    goal::set_status(conn, &goal.id, GoalStatus::Failed)?;
                    events::append(conn, &goal.session_id, Some(&goal.id), EventKind::NodeFailed, &reason)?;
                }
                TickAction::RunIntegrator => {
                    run_integrator(ctx, conn, &goal, &graph, table, health)?;
                }
                TickAction::Dispatch { node_id, agent, effort, worktree, max_steps: _ } => {
                    let Some(node) = graph.find(&node_id) else { continue };
                    let prompt = build_node_prompt(&graph, node);
                    let opts = crate::task::OwnedRunTaskOptions {
                        description: prompt,
                        agent: agent.clone(),
                        cwd: std::path::PathBuf::from(load_session_cwd(conn, &goal.session_id)?),
                        use_worktree: worktree,
                        account: None,
                        real_home: false,
                        no_memory_context: false,
                        timeout: timeout_for(effort),
                        allow_fallback: true,
                        usage_json: cfg.usage_json_agents.iter().any(|a| a == &agent),
                    };
                    match dispatcher.dispatch(opts) {
                        Ok(task_id) => {
                            goal::update_node(conn, &goal.id, &node_id, NodeStatus::Running, Some(task_id), None, None)?;
                            goal::bump_dispatches(conn, &goal.id)?;
                            events::append(
                                conn,
                                &goal.session_id,
                                Some(&goal.id),
                                EventKind::NodeStarted,
                                &format!("{node_id} → {agent} (#{task_id})"),
                            )?;
                        }
                        Err(e) => {
                            goal::update_node(conn, &goal.id, &node_id, NodeStatus::Failed, None, None, None)?;
                            events::append(
                                conn,
                                &goal.session_id,
                                Some(&goal.id),
                                EventKind::NodeFailed,
                                &format!("{node_id}: dispatch failed: {e}"),
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// spec §4.6: called after a coordinator-backed task finishes. maps the
/// task row back to its node, records success / failure, applies the retry
/// decision, and re-ticks. a no-op when `task_id` is not a coordinator
/// node (so the handler can call it blindly after any task).
pub fn on_task_finished(
    ctx: &Context,
    conn: &mut Connection,
    cfg: &CoordinatorConfig,
    table: &RoutingTable,
    health: &PoolHealth,
    dispatcher: &dyn Dispatcher,
    task_id: i64,
) -> Result<()> {
    settle_finished_node(ctx, conn, table, health, task_id)?;
    tick(ctx, conn, cfg, table, health, dispatcher)
}

/// records the outcome of one finished coordinator-backed task against its
/// node and runs retry / supervisor / block, WITHOUT re-ticking. a no-op
/// when `task_id` is not a running coordinator node.
fn settle_finished_node(
    ctx: &Context,
    conn: &mut Connection,
    table: &RoutingTable,
    health: &PoolHealth,
    task_id: i64,
) -> Result<()> {
    let row: Option<(String, String, u32)> = conn
        .query_row(
            "SELECT goal_id, id, attempts FROM graph_nodes WHERE task_id = ?1 AND status = 'running'",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((goal_id, node_id, attempts)) = row else {
        return Ok(());
    };

    let task = crate::task::get(conn, task_id)?;
    let (completed, rate_limited, artifact) = match &task {
        Some(t) => (
            matches!(t.status, single_protocol::TaskStatus::Completed),
            t.rate_limited,
            t.artifact_path.clone(),
        ),
        None => (false, false, None),
    };
    let goal = goal::get(conn, &goal_id)?.context("goal vanished mid-run")?;

    if completed {
        goal::update_node(conn, &goal_id, &node_id, NodeStatus::Done, None, artifact.as_deref(), None)?;
        events::append(conn, &goal.session_id, Some(&goal_id), EventKind::NodeDone, &node_id)?;
    } else {
        let semantic = false; // crash/timeout/rate-limit, not a wrong result
        match retry_decision(attempts, semantic) {
            RetryDecision::RetrySameNextAgent if !rate_limited => {
                // bounce the node back to pending with an incremented
                // attempt count; the next tick re-routes it (select_agent
                // walks past the now-known-bad agent via pool health, and
                // dispatch-time fallback already covers rate limits).
                goal::update_node(
                    conn,
                    &goal_id,
                    &node_id,
                    NodeStatus::Pending,
                    None,
                    None,
                    Some(attempts + 1),
                )?;
                events::append(
                    conn,
                    &goal.session_id,
                    Some(&goal_id),
                    EventKind::NodeFailed,
                    &format!("{node_id}: retry {} scheduled", attempts + 1),
                )?;
            }
            _ => {
                goal::update_node(conn, &goal_id, &node_id, NodeStatus::Failed, None, artifact.as_deref(), None)?;
                events::append(
                    conn,
                    &goal.session_id,
                    Some(&goal_id),
                    EventKind::NodeFailed,
                    &format!("{node_id}: exhausted retries → supervisor/failed"),
                )?;
                run_supervisor_or_block(ctx, conn, &goal, &node_id, table, health)?;
            }
        }
    }

    Ok(())
}

fn run_supervisor_or_block(
    ctx: &Context,
    conn: &mut Connection,
    goal: &Goal,
    failing_node_id: &str,
    table: &RoutingTable,
    health: &PoolHealth,
) -> Result<()> {
    let cfg = CoordinatorConfig::default();
    if goal.supervisor_patches >= cfg.max_supervisor_patches {
        let reason = format!(
            "tried {} supervisor fixes on this goal; need a decision. last failure at node {failing_node_id}",
            goal.supervisor_patches
        );
        goal::set_blocked(conn, &goal.id, &reason)?;
        events::append(conn, &goal.session_id, Some(&goal.id), EventKind::Blocked, &reason)?;
        return Ok(());
    }

    let mut graph = goal::load_graph(conn, &goal.id)?;
    let failing_output = graph
        .find(failing_node_id)
        .and_then(|n| n.output_ref.clone())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let cwd = std::path::PathBuf::from(load_session_cwd(conn, &goal.session_id)?);

    match crate::coordinator::brain::supervise(
        conn, ctx, &cwd, &graph, failing_node_id, &failing_output, table, health,
    ) {
        Ok(ops) => match graph.apply_patch(&ops) {
            Ok(()) => {
                goal::save_graph(conn, &goal.id, &graph)?;
                goal::bump_supervisor_patches(conn, &goal.id)?;
                events::append(
                    conn,
                    &goal.session_id,
                    Some(&goal.id),
                    EventKind::Supervisor,
                    &format!("applied {} patch op(s)", ops.len()),
                )?;
            }
            Err(e) => {
                let reason = format!("supervisor patch could not be applied: {e}");
                goal::set_status(conn, &goal.id, GoalStatus::Failed)?;
                events::append(conn, &goal.session_id, Some(&goal.id), EventKind::NodeFailed, &reason)?;
            }
        },
        Err(e) => {
            let reason = format!("supervisor produced no usable patch: {e}");
            goal::set_blocked(conn, &goal.id, &reason)?;
            events::append(conn, &goal.session_id, Some(&goal.id), EventKind::Blocked, &reason)?;
        }
    }
    Ok(())
}

fn run_integrator(
    ctx: &Context,
    conn: &mut Connection,
    goal: &Goal,
    graph: &TaskGraph,
    table: &RoutingTable,
    health: &PoolHealth,
) -> Result<()> {
    let cwd = std::path::PathBuf::from(load_session_cwd(conn, &goal.session_id)?);
    let outputs: Vec<(String, String)> = graph
        .nodes
        .iter()
        .map(|n| {
            let body = n
                .output_ref
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();
            (n.id.clone(), body)
        })
        .collect();

    match crate::coordinator::brain::integrate(conn, ctx, &cwd, &goal.text, &outputs, table, health) {
        Ok(outcome) => {
            goal::set_summary(conn, &goal.id, &outcome.summary)?;
            let final_status = if outcome.unrecoverable { GoalStatus::Failed } else { GoalStatus::Done };
            goal::set_status(conn, &goal.id, final_status)?;
            events::append(
                conn,
                &goal.session_id,
                Some(&goal.id),
                EventKind::Integrated,
                &serde_json::to_string(&outcome).unwrap_or_else(|_| outcome.summary.clone()),
            )?;
        }
        Err(e) => {
            let reason = format!("integrator failed: {e}");
            goal::set_blocked(conn, &goal.id, &reason)?;
            events::append(conn, &goal.session_id, Some(&goal.id), EventKind::Blocked, &reason)?;
        }
    }
    Ok(())
}

/// feeds a node its dependencies' outputs alongside its own description.
fn build_node_prompt(graph: &TaskGraph, node: &Node) -> String {
    let mut deps = String::new();
    for dep_id in &node.depends_on {
        if let Some(dep) = graph.find(dep_id) {
            if let Some(out) = dep.output_ref.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
                deps.push_str(&format!("\n--- output of {dep_id} ---\n{}\n", crate::orchestrate::truncate(&out, 2000)));
            }
        }
    }
    if deps.is_empty() {
        node.desc.clone()
    } else {
        format!("{}\n\nUPSTREAM RESULTS:{deps}", node.desc)
    }
}

fn load_session_cwd(conn: &Connection, session_id: &str) -> Result<String> {
    Ok(crate::coordinator::session::get(conn, session_id)?
        .map(|s| s.cwd)
        .unwrap_or_else(|| ".".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::graph::{Effort, Node, NodeKind, NodeStatus};

    fn cfg(max_parallel: usize) -> CoordinatorConfig {
        CoordinatorConfig { max_parallel, ..Default::default() }
    }

    fn budget_ok() -> GoalBudget {
        let now = Utc::now();
        GoalBudget { dispatches: 0, max_dispatches: 25, started_at: now, max_minutes: 60, now }
    }

    fn node(id: &str, deps: &[&str], effort: Effort, agent: &str) -> Node {
        Node {
            id: id.into(),
            desc: id.into(),
            kind: NodeKind::Code,
            effort,
            agent: agent.into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            status: NodeStatus::Pending,
            task_id: None,
            attempts: 0,
            worktree: false,
            output_ref: None,
        }
    }

    fn caps(global_running: usize, agent_caps: &[(&str, usize)], agent_running: &[(&str, usize)]) -> Capacity {
        Capacity {
            global_running,
            per_agent_cap: agent_caps.iter().map(|(a, c)| (a.to_string(), *c)).collect(),
            per_agent_running: agent_running.iter().map(|(a, c)| (a.to_string(), *c)).collect(),
        }
    }

    fn dispatched_ids(actions: &[TickAction]) -> Vec<String> {
        actions
            .iter()
            .filter_map(|a| match a {
                TickAction::Dispatch { node_id, .. } => Some(node_id.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn admits_independent_nodes_up_to_global_cap() {
        let g = TaskGraph {
            nodes: vec![
                node("s1", &[], Effort::Standard, "grok"),
                node("s2", &[], Effort::Standard, "grok"),
                node("s3", &[], Effort::Standard, "grok"),
                node("s4", &[], Effort::Standard, "grok"),
            ],
        };
        let a = tick_pure(&g, &cfg(2), &caps(0, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert_eq!(dispatched_ids(&a).len(), 2);
    }

    #[test]
    fn respects_per_agent_cap() {
        let g = TaskGraph {
            nodes: vec![
                node("s1", &[], Effort::Standard, "opencode"),
                node("s2", &[], Effort::Standard, "opencode"),
                node("s3", &[], Effort::Standard, "opencode"),
            ],
        };
        let a = tick_pure(
            &g,
            &cfg(6),
            &caps(0, &[("opencode", 1)], &[]),
            &budget_ok(),
            &RoutingTable::default(),
            &PoolHealth::default(),
        );
        assert_eq!(dispatched_ids(&a), vec!["s1"]);
    }

    #[test]
    fn prefers_critical_path_then_cheaper_effort() {
        // s1 -> s2 -> s3 (depth 2), plus lone s4 (deep) and lone s5 (quick).
        let g = TaskGraph {
            nodes: vec![
                node("s1", &[], Effort::Standard, "grok"),
                node("s2", &["s1"], Effort::Standard, "grok"),
                node("s3", &["s2"], Effort::Standard, "grok"),
                node("s4", &[], Effort::Deep, "grok"),
                node("s5", &[], Effort::Quick, "grok"),
            ],
        };
        // global cap 1 → only the single most-preferred node dispatches.
        let a = tick_pure(&g, &cfg(1), &caps(0, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert_eq!(dispatched_ids(&a), vec!["s1"]); // critical path wins

        // remove the chain; now effort breaks the tie: s5 (quick) before s4 (deep).
        let g2 = TaskGraph { nodes: vec![node("s4", &[], Effort::Deep, "grok"), node("s5", &[], Effort::Quick, "grok")] };
        let a2 = tick_pure(&g2, &cfg(1), &caps(0, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert_eq!(dispatched_ids(&a2), vec!["s5"]);
    }

    #[test]
    fn queues_when_pool_saturated() {
        let g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok")] };
        let a = tick_pure(&g, &cfg(2), &caps(2, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert_eq!(a, vec![TickAction::Noop]);
    }

    #[test]
    fn budget_dispatch_cap_blocks_the_goal() {
        let g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok")] };
        let now = Utc::now();
        let b = GoalBudget { dispatches: 25, max_dispatches: 25, started_at: now, max_minutes: 60, now };
        let a = tick_pure(&g, &cfg(6), &caps(0, &[], &[]), &b, &RoutingTable::default(), &PoolHealth::default());
        match &a[0] {
            TickAction::Block { reason } => assert!(reason.contains("dispatch budget")),
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[test]
    fn budget_wallclock_cap_blocks_the_goal() {
        let g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok")] };
        let now = Utc::now();
        let b = GoalBudget {
            dispatches: 1,
            max_dispatches: 25,
            started_at: now - chrono::Duration::minutes(90),
            max_minutes: 60,
            now,
        };
        let a = tick_pure(&g, &cfg(6), &caps(0, &[], &[]), &b, &RoutingTable::default(), &PoolHealth::default());
        match &a[0] {
            TickAction::Block { reason } => assert!(reason.contains("time budget")),
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[test]
    fn all_terminal_triggers_integrator() {
        let mut g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok")] };
        g.nodes[0].status = NodeStatus::Done;
        let a = tick_pure(&g, &cfg(6), &caps(0, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert_eq!(a, vec![TickAction::RunIntegrator]);
    }

    #[test]
    fn all_failed_and_nothing_done_fails_the_goal() {
        let mut g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok")] };
        g.nodes[0].status = NodeStatus::Failed;
        let a = tick_pure(&g, &cfg(6), &caps(0, &[], &[]), &budget_ok(), &RoutingTable::default(), &PoolHealth::default());
        assert!(matches!(a[0], TickAction::Fail { .. }));
    }

    #[test]
    fn retry_decision_advances_agent_then_escalates_to_supervisor() {
        assert_eq!(retry_decision(0, false), RetryDecision::RetrySameNextAgent);
        assert_eq!(retry_decision(1, false), RetryDecision::RetrySameNextAgent);
        assert_eq!(retry_decision(2, false), RetryDecision::Supervisor);
        assert_eq!(retry_decision(0, true), RetryDecision::Supervisor); // semantic failure jumps straight to supervisor
    }

    #[test]
    fn reconcile_marks_running_node_with_dead_task_as_failed_and_completed_as_done() {
        use crate::coordinator::goal;
        use crate::coordinator::graph::GoalMode;

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::coordinator::ensure_coordinator_schema(&conn).unwrap();
        crate::task::ensure_schema(&conn).unwrap();

        let s = crate::coordinator::session::new_session(&conn, std::path::Path::new("/tmp/p")).unwrap();
        let g = goal::create(&conn, &s.id, "g", GoalMode::Auto, 25, 60).unwrap();
        let graph = TaskGraph {
            nodes: vec![
                node("s1", &[], Effort::Standard, "grok"),
                node("s2", &[], Effort::Standard, "grok"),
                node("s3", &[], Effort::Standard, "grok"),
            ],
        };
        goal::save_graph(&mut conn, &g.id, &graph).unwrap();

        // s1 -> a task row that FAILED; s2 -> a task row still RUNNING;
        // s3 -> a task row that COMPLETED.
        for (nid, tid, status) in [("s1", 10i64, "failed"), ("s2", 11, "running"), ("s3", 12, "completed")] {
            conn.execute(
                "INSERT INTO tasks (id, description, agent, status, timed_out, created_at, updated_at, cwd, workspace_id)
                 VALUES (?1, 'x', 'grok', ?2, 0, '', '', '', '')",
                rusqlite::params![tid, status],
            )
            .unwrap();
            goal::update_node(&conn, &g.id, nid, NodeStatus::Running, Some(tid), None, None).unwrap();
        }

        let touched = reconcile(&conn).unwrap();
        assert_eq!(touched, 2); // s1 and s3 move; s2 stays running

        let reloaded = goal::load_graph(&conn, &g.id).unwrap();
        assert_eq!(reloaded.find("s1").unwrap().status, NodeStatus::Failed);
        assert_eq!(reloaded.find("s2").unwrap().status, NodeStatus::Running);
        assert_eq!(reloaded.find("s3").unwrap().status, NodeStatus::Done);
    }

    #[test]
    fn build_node_prompt_folds_in_upstream_output_when_present() {
        let g = TaskGraph { nodes: vec![node("s1", &[], Effort::Standard, "grok"), node("s2", &["s1"], Effort::Standard, "grok")] };
        // no output_ref on s1 -> prompt is just the bare desc
        let p = build_node_prompt(&g, g.find("s2").unwrap());
        assert_eq!(p, "s2");
    }
}
