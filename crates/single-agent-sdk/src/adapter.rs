use crate::discover::{discover, Discovery};
use single_protocol::McpServerSpec;
use anyhow::Result;
use single_protocol::IntegrationWrite;
use std::path::Path;

/// What Phase 1 asks of an agent adapter: real detection, and writing
/// SingleCLI's unified MCP registry into the agent's native config format.
/// Process lifecycle (start/stop/stream), which the full spec's
/// `AgentAdapter` (section 39) also requires, is intentionally not part of
/// this trait yet — it's Phase 4 scope, and adding it later shouldn't
/// require reshaping this trait, just extending it.
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
}
