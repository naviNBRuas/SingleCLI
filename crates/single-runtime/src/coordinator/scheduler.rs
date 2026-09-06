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

use crate::coordinator::graph::{Effort, NodeStatus, TaskGraph};
use crate::coordinator::routing::{self, CoordinatorConfig, PoolHealth, RoutingTable};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

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
}
