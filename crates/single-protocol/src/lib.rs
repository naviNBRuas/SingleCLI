//! Wire types for the SingleCLI runtime IPC protocol.
//!
//! The CLI and TUI talk to the runtime daemon over a Unix domain socket using
//! newline-delimited JSON: one [`Request`] per line in, one [`Response`] per
//! line out. This keeps the protocol trivially inspectable with `nc`/`socat`
//! during development, at the cost of not being a "real" RPC framework —
//! acceptable for Phase 1's single local daemon + local clients.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Status,
    Doctor,
    AgentList,
    AgentInspect { name: String },
    McpList,
    Setup { dry_run: bool },
    InstallIntegrations { dry_run: bool },
    UninstallIntegrations,
    ProfileList,
    ProfileUse { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok { data: ResponseData },
    Error { message: String },
}

// Adjacently tagged (not internally tagged): a couple of variants below
// wrap a `Vec<_>`, and serde can't internally-tag a variant whose payload
// serializes to a JSON array rather than an object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ResponseData {
    Status(RuntimeStatus),
    Doctor(DoctorReport),
    Agents(Vec<AgentInfo>),
    Agent(AgentInfo),
    McpServers(Vec<McpServerInfo>),
    SetupPlan(SetupPlan),
    IntegrationResult(IntegrationResult),
    Profiles(Vec<String>),
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub version: String,
    pub active_profile: String,
    pub agents_known: usize,
    pub agents_detected: usize,
    pub socket_path: String,
    pub db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub adapter: String,
    pub command: String,
    pub detected: bool,
    pub version: Option<String>,
    pub install_method: InstallMethod,
    pub bootstrap_install: Option<BootstrapInstall>,
    pub unverified: bool,
    pub capabilities: CapabilityFlags,
    pub config_paths: Vec<String>,
    /// Free-text caveat surfaced in `doctor`/`agent inspect`, e.g. when an
    /// entry doesn't fit the coding-agent model cleanly (see `perplexity`).
    pub notes: Option<String>,
}

/// Describes how an agent CLI is (or would be) installed. Distinct from
/// `BootstrapInstall`, which is the exact command `single setup` runs when
/// the agent is missing — this is just descriptive, for `doctor` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallMethod {
    Native { detail: String },
    StandaloneBinary { detail: String },
    PackageManager { detail: String },
    Unsupported { reason: String },
}

/// The real, vendor-verified install command `single setup` runs when this
/// agent isn't detected. `source` is the documentation URL it was verified
/// against — kept alongside the command so the registry stays auditable
/// instead of hiding a bare `curl | sh` in code. `None` means no verified
/// install method exists (see `InstallMethod::Unsupported` for why).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapInstall {
    pub command: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub streaming: bool,
    pub mcp: bool,
    pub lsp: bool,
    pub tools: bool,
    pub sessions: bool,
    pub structured_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    pub synced_to: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPlan {
    pub actions: Vec<SetupAction>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupAction {
    pub agent: String,
    pub action: SetupActionKind,
    pub detail: String,
    pub executed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupActionKind {
    AlreadyInstalled,
    Install,
    Unsupported,
    ConfigureIntegration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationResult {
    pub dry_run: bool,
    pub writes: Vec<IntegrationWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationWrite {
    pub agent: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub applied: bool,
    pub detail: String,
}

/// Envelope helper so a future event stream (Phase 4) can share the same
/// framing without breaking the request/response wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub payload: T,
}

pub type Metadata = BTreeMap<String, String>;

/// A format-agnostic MCP server entry from SingleCLI's unified registry.
/// Lives here (rather than in `single-agent-sdk`, which consumes it) so
/// both `single-core`'s config/registry loading and `single-agent-sdk`'s
/// per-format writers can share one definition without a circular
/// dependency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerSpec {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
