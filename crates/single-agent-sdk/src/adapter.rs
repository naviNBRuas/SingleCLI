use crate::discover::{discover, Discovery};
use anyhow::Result;
use single_protocol::{IntegrationWrite, McpServerSpec, RunOutcome};
use std::path::Path;
use std::time::Duration;

/// What SingleCLI asks of an agent adapter: real detection, writing
/// SingleCLI's unified MCP registry into the agent's native config format,
/// and (Phase 4) a synchronous one-shot non-interactive invocation.
///
/// `run_prompt` is deliberately **not** the full spec section 39 lifecycle
/// (`start`/`stop`/`pause`/`resume`/`stream`/`cancel` against a live
/// session) — it shells out to each CLI's own non-interactive mode
/// (`claude -p`, `codex exec`, `opencode run`, `agy -p`), blocks until it
/// exits or a timeout fires, and returns the captured output. That's a
/// real, working capability (Phase 4's orchestrator uses it for real task
/// execution), just a narrower one than a fully streamed, cancellable
/// session — which needs the runtime to hold long-lived per-task process
/// state across multiple requests, not yet built. Extending this to true
/// streaming later is additive, not a rewrite of this trait.
pub trait AgentAdapter {
    fn command(&self) -> &str;

    fn discover(&self) -> Discovery {
        discover(self.command())
    }

    /// Applies `servers` to this agent's real config file(s) under `home`.
    /// When `dry_run` is true, computes what *would* change without writing
    /// anything (still backs up nothing, writes nothing).
    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite>;

    /// Inverse of `configure_mcp`: removes only the named servers, used by
    /// `single uninstall-integrations`.
    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite>;

    /// Runs `prompt` non-interactively with `cwd` as the working directory,
    /// killing the process and setting `timed_out: true` if it runs past
    /// `timeout`. Default: unsupported (used by agents with no verified
    /// non-interactive mode, e.g. `pplx`, which isn't a coding agent at all).
    ///
    /// `home` overrides `$HOME` for the subprocess when set — this is how
    /// multiple isolated accounts of the same agent run concurrently (see
    /// `single-core::account::ensure_isolated_home`); `None` runs against
    /// the caller's real `$HOME` as before.
    fn run_prompt(&self, _cwd: &Path, _prompt: &str, _home: Option<&Path>, _timeout: Duration) -> Result<RunOutcome> {
        anyhow::bail!("{} has no non-interactive run mode wired up", self.command())
    }

    /// Installs a plugin via this agent's own real plugin-install command
    /// (`claude plugin install`, `codex plugin add`, `opencode plugin`,
    /// `agy plugin install` — verified per-agent, see each impl). Default:
    /// unsupported (agents with no verified plugin CLI, e.g. `pplx`).
    fn install_plugin(&self, _target: &str, _cwd: &Path, _timeout: Duration) -> Result<RunOutcome> {
        anyhow::bail!("{} has no verified plugin-install command wired up", self.command())
    }
}

pub(crate) fn run_with_prompt_flag(command: &str, cwd: &Path, prompt: &str, home: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
    crate::run::run_command_with_home(command, &["-p".to_string(), prompt.to_string()], cwd, home, timeout)
}
