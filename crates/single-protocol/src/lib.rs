//! Wire types for the SingleCLI runtime IPC protocol.
//!
//! The CLI and TUI talk to the runtime daemon over a Unix domain socket using
//! newline-delimited JSON: one [`Request`] per line in, one [`Response`] per
//! line out. This keeps the protocol trivially inspectable with `nc`/`socat`
//! during development, at the cost of not being a "real" RPC framework —
//! acceptable for Phase 1's single local daemon + local clients.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Status,
    Doctor,
    AgentList,
    AgentInspect { name: String },
    McpList,
    McpAdd { server: McpServerSpec },
    McpRemove { name: String },
    McpEnable { name: String },
    McpDisable { name: String },
    McpInspect { name: String },
    McpPresetList,
    McpAddPreset { name: String },
    LspList,
    LspAdd { server: LspServerSpec },
    LspRemove { name: String },
    LspInspect { name: String },
    LspPresetList,
    LspAddPreset { name: String },
    ToolList,
    ToolAdd { tool: ToolSpec },
    ToolInspect { name: String },
    ToolEnable { name: String },
    ToolDisable { name: String },
    SecretList,
    SecretSet { name: String, value: String },
    /// Returns whether the secret exists and its value in one shot — the
    /// value never passes through the runtime's event log (see
    /// `single-runtime`'s `handlers.rs`), only through this direct response.
    SecretGet { name: String },
    SecretDelete { name: String },
    SkillList,
    SkillInstall { name: String, source_path: String },
    SkillRemove { name: String },
    SkillInspect { name: String },
    /// Copies a skill into Claude Code's real skill directory
    /// (`~/.claude/skills/<name>/`) — see `single_core::skills::sync_to_claude`.
    SkillSyncClaude { name: String },
    MemoryStore {
        scope: Option<MemoryScope>,
        source: Option<MemorySource>,
        project: Option<String>,
        agent: Option<String>,
        task: Option<String>,
        title: String,
        content: String,
        confidence: Option<f64>,
        expires_in_seconds: Option<i64>,
    },
    MemorySearch { query: String, scope: Option<MemoryScope>, project: Option<String> },
    MemoryGet { id: i64 },
    MemoryDelete { id: i64 },
    MemoryList { scope: Option<MemoryScope> },
    ContextShow { cwd: String },
    TaskRun { description: String, agent: String, cwd: String, use_worktree: bool, account: Option<String>, timeout_secs: u64 },
    TaskList,
    TaskInspect { id: i64 },
    Orchestrate { goal: String, agents: Vec<String>, cwd: String, use_worktree: bool, timeout_secs: u64 },
    AccountCapture { agent: String, name: String, label: Option<String> },
    AccountUse { agent: String, name: String },
    AccountList { agent: Option<String> },
    AccountRemove { agent: String, name: String },
    AccountSetStatus { agent: String, name: String, status: AccountStatus },
    ProviderAdd { name: String, env_var_name: String, base_url: Option<String> },
    ProviderAddPreset { name: String },
    ProviderPresetList,
    ProviderRemove { name: String },
    ProviderList,
    ProviderInspect { name: String },
    ProviderSetKey { name: String, value: String },
    ProviderSync { name: String, agents: Vec<String>, dry_run: bool },
    KgCreateEntity { name: String, entity_type: String },
    KgAddObservation { entity: String, content: String },
    KgCreateRelation { from: String, to: String, relation_type: String },
    KgDeleteEntity { name: String },
    KgGetEntity { name: String },
    KgQuery { term: String },
    KgReadGraph,
    CacheSet { key: String, value: String, ttl_secs: Option<u64> },
    CacheGet { key: String },
    CacheDelete { key: String },
    CacheList { pattern: String },
    CacheStatus,
    VectorUpsert { collection: String, id: u64, vector: Vec<f32>, payload: serde_json::Value },
    VectorSearch { collection: String, vector: Vec<f32>, limit: u64 },
    VectorDelete { collection: String, id: u64 },
    VectorStatus,
    AgentInstall { name: String, dry_run: bool },
    Setup { dry_run: bool },
    InstallIntegrations { dry_run: bool },
    UninstallIntegrations,
    ProfileList,
    ProfileUse { name: String },
    PluginAdd { plugin: PluginSpec },
    PluginRemove { name: String },
    PluginList,
    PluginInspect { name: String },
    PluginSync { name: String, agents: Vec<String>, dry_run: bool },
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
    McpServer(McpServerSpec),
    McpPresets(Vec<McpPresetInfo>),
    LspServers(Vec<LspServerSpec>),
    LspServer(LspServerSpec),
    LspPresets(Vec<LspPresetInfo>),
    Tools(Vec<ToolSpec>),
    Tool(ToolSpec),
    SecretNames(Vec<String>),
    SecretValue(Option<String>),
    Skills(Vec<String>),
    SkillContents(Vec<String>),
    SkillSynced { path: String },
    MemoryId(i64),
    MemoryEntry(MemoryEntry),
    MemoryEntries(Vec<MemoryEntry>),
    Context(ProjectContext),
    Task(TaskRecord),
    AccountProfile(AccountProfileInfo),
    AccountProfiles(Vec<AccountProfileInfo>),
    AccountSwitched(AccountSwitchResult),
    Provider(ProviderSpec),
    Providers(Vec<ProviderSpec>),
    ProviderPresets(Vec<ProviderPresetInfo>),
    ProviderSyncResults(Vec<ProviderSyncResult>),
    KgEntityId(i64),
    KgEntity(KgEntity),
    KgEntities(Vec<KgEntity>),
    KgGraph(KnowledgeGraphSnapshot),
    CacheValue(Option<String>),
    CacheKeys(Vec<String>),
    CacheStatus { configured: bool, url: Option<String>, reachable: bool },
    VectorHits(Vec<VectorHit>),
    VectorStatus { configured: bool, url: Option<String>, reachable: bool },
    Tasks(Vec<TaskRecord>),
    OrchestrateResult(Vec<TaskRecord>),
    AgentInstallResult(SetupAction),
    SetupPlan(SetupPlan),
    IntegrationResult(IntegrationResult),
    Profiles(Vec<String>),
    Plugin(PluginSpec),
    Plugins(Vec<PluginSpec>),
    PluginSyncResults(Vec<PluginInstallResult>),
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

/// A format-agnostic LSP server entry, mirroring `McpServerSpec`'s shape
/// and reasons for living here (shared by `single-core`'s registry and any
/// future agent-sdk writer without a circular dependency).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspServerSpec {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub risk_level: RiskLevel,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Memory scope (spec section 9). Lives here so both `single-runtime`'s
/// SQLite-backed store and `single-cli`'s request-building code share one
/// definition without a circular dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Working,
    Project,
    User,
    Agent,
    Task,
    LongTerm,
    Knowledge,
}

/// Provenance classification (spec sections 46-47): the *claimed* source of
/// a memory entry, as given by the caller — SingleCLI does not itself
/// verify or upgrade this classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    UserInstruction,
    AgentOutput,
    ToolOutput,
    ProjectContent,
    ExternalContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: i64,
    pub scope: MemoryScope,
    pub source: MemorySource,
    pub project: Option<String>,
    pub agent: Option<String>,
    pub task: Option<String>,
    pub title: String,
    pub content: String,
    pub confidence: f64,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// Task lifecycle status (spec section 17's TaskCreated/TaskStarted/
/// TaskCompleted/TaskFailed events, collapsed into a single current-state
/// field — Phase 4 doesn't yet persist the full event sequence as
/// separately queryable rows beyond the generic runtime event log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Created,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: i64,
    pub description: String,
    pub agent: String,
    pub status: TaskStatus,
    pub worktree_path: Option<String>,
    pub artifact_path: Option<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The result of running an agent CLI non-interactively against a single
/// prompt (spec section 39's `send`/lifecycle, scoped down to Phase 4's
/// synchronous one-shot invocation — see `single-agent-sdk::adapter` docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutcome {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u128,
}

/// Metadata about a captured account-switch profile (spec section 41's
/// "reusable agent definitions" adjacent concept, but scoped to login
/// state rather than full persona config). Never carries token contents —
/// see `single-core::account`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProfileInfo {
    pub agent: String,
    pub name: String,
    /// Human-readable identity (email or display name) for this captured
    /// login, so multiple accounts per agent are distinguishable at a
    /// glance. Set at capture time; SingleCLI has no way to read it back
    /// out of the agent's own credential files, so it's user-supplied.
    pub label: Option<String>,
    pub captured_at: String,
    pub unverified_complete: bool,
    /// Manually-set usability of this account. There is no verified,
    /// stable API across claude/codex/agy for querying live quota/rate-
    /// limit state, so this is never auto-detected — the user (or a task
    /// failure surfaced elsewhere) sets it, and SingleCLI just remembers
    /// and displays it.
    #[serde(default)]
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    #[default]
    Unknown,
    Available,
    RateLimited,
    NeedsTopup,
}

impl AccountStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "unknown" => Self::Unknown,
            "available" => Self::Available,
            "rate_limited" | "rate-limited" => Self::RateLimited,
            "needs_topup" | "needs-topup" => Self::NeedsTopup,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Available => "available",
            Self::RateLimited => "rate_limited",
            Self::NeedsTopup => "needs_topup",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSwitchResult {
    pub agent: String,
    pub name: String,
    pub backed_up: Vec<String>,
}

/// A knowledge-graph entity with its accumulated observations (the same
/// entity/observation/relation shape as the widely-used MCP memory-server
/// convention already configured on this project's own reference machine
/// — a real, proven pattern, not invented for this project).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEntity {
    pub name: String,
    pub entity_type: String,
    pub observations: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgRelation {
    pub from_entity: String,
    pub to_entity: String,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeGraphSnapshot {
    pub entities: Vec<KgEntity>,
    pub relations: Vec<KgRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorHit {
    pub id: u64,
    pub score: f32,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPresetInfo {
    pub name: String,
    pub env_var_name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPresetInfo {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspPresetInfo {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub extensions: Vec<String>,
}

/// A plugin registered across agents (spec section 29/41). `target` is
/// used verbatim for the three agents that share the real, verified
/// `plugin[@marketplace]` convention (`claude plugin install`, `codex
/// plugin add`, `agy plugin install`); `opencode_module`, if set, is used
/// for OpenCode, whose real plugin command (`opencode plugin <module>`)
/// takes a plain npm module name instead — a genuinely different
/// addressing scheme, not a naming inconsistency this project invented.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginSpec {
    pub name: String,
    pub target: String,
    pub opencode_module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstallResult {
    pub plugin: String,
    pub agent: String,
    pub applied: bool,
    pub detail: String,
}

/// A registered LLM provider (spec section 30): OpenAI, Anthropic,
/// OpenCode Zen, a local model server, etc. The actual API key is never
/// stored here — only a reference (`secret_name`) into the OS keychain
/// (`single-core::secrets`), and `env_var_name` says which environment
/// variable name that key needs to become for an agent to pick it up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSpec {
    pub name: String,
    pub env_var_name: String,
    pub secret_name: String,
    pub base_url: Option<String>,
}

/// The result of trying to sync one provider's key into one agent's real
/// config. Mirrors `IntegrationWrite`'s shape/spirit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSyncResult {
    pub provider: String,
    pub agent: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub applied: bool,
    pub detail: String,
}

/// Repository/git state + project doc discovery for a working directory
/// (spec section 10).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    pub cwd: String,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub changed_files: Vec<String>,
    pub project_docs: Vec<String>,
}
