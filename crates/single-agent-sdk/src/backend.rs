//! Where an agent's prompt-driving process actually runs: directly on the
//! host (today's default, isolated only by `$HOME` — see
//! `single_core::agent_home`/`account`), or inside a persistent Docker
//! container (opt-in per agent/account — see `single_core::docker` for
//! the enable/disable settings and `single-runtime::docker` for container
//! lifecycle). Threaded through `AgentAdapter::run_prompt` and
//! `run::run_command_live` so the same call site works for either backend
//! without the caller needing to know which one is active.

use std::path::Path;

pub enum ExecBackend<'a> {
    /// `home: Some(path)` overrides `$HOME` for the child process — same
    /// meaning as the plain `Option<&Path>` this replaced. `None` runs
    /// against the caller's real ambient `$HOME`.
    Host { home: Option<&'a Path> },
    /// Runs via `docker exec -w <workdir> <container> <command> <args...>`
    /// against an already-running container (see
    /// `single-runtime::docker::ensure_started`, called before this by
    /// whichever daemon handler resolved the backend) — no `$HOME`
    /// override needed, since the container's home is whatever was baked
    /// in when it was created (the isolated home directory, bind-mounted).
    Docker { container: &'a str, workdir: &'a Path },
}

impl<'a> ExecBackend<'a> {
    pub fn host(home: Option<&'a Path>) -> Self {
        ExecBackend::Host { home }
    }
}
