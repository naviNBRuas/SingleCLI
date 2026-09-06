//! routing table (`~/.config/single/routing.toml`, spec §5.4) and
//! `coordinator.toml` config. every `kind` — work kinds and the brain
//! kinds — has a capability-ranked agent list per `effort`; the selector
//! walks it and picks the first agent that is detected, authed, and not
//! rate-limited, then dispatch-time fallback continues down the chain.
//!
//! fleshed out in a later task (plan Task 6). this stub carries the config
//! types so the module tree compiles.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoordinatorConfig {
    pub max_parallel: usize,
    pub tick_interval_secs: u64,
    pub max_dispatches_per_goal: u32,
    pub max_goal_minutes: u32,
    pub max_supervisor_patches: u32,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_parallel: 6,
            tick_interval_secs: 5,
            max_dispatches_per_goal: 25,
            max_goal_minutes: 60,
            max_supervisor_patches: 5,
        }
    }
}
