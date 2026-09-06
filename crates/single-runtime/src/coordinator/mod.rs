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

    #[test]
    fn short_id_is_unique_and_prefixed() {
        let a = short_id("goal");
        let b = short_id("goal");
        assert!(a.starts_with("goal_"));
        assert_ne!(a, b);
    }
}
