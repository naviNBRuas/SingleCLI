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
    /// Asks a running `single-runtimed` to exit after acknowledging this
    /// request — see `single-cli::daemon::stop_running`. Exists because the
    /// daemon inherits its environment (notably `$PATH`) once at spawn
    /// time and keeps it for the life of the process, so a newly installed
    /// agent CLI is invisible to detection until the daemon is restarted.
    Shutdown,
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
    /// Toggles gateway mode (see `single_core::mcp::gateway_mode`) — takes
    /// effect on the next `single install-integrations --yes`, not
    /// retroactively.
    McpGatewaySetEnabled { enabled: bool },
    McpGatewayStatus,
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
    /// Lists the curated starter skills bundled with SingleCLI itself —
    /// see `single_core::skills::starter_set`.
    SkillStarterList,
    SkillInstallStarter { name: String },
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
    /// Embeds `query` and searches the vector store for the nearest
    /// stored memory entries — real semantic search, not `LIKE` matching
    /// (see `single-runtime::embeddings`/`qdrant_backend`). Falls back to
    /// `MemorySearch`'s substring matching when no embeddings key and/or
    /// `SINGLE_QDRANT_URL` are configured, rather than erroring.
    MemorySearchSemantic { query: String, scope: Option<MemoryScope>, project: Option<String>, limit: u64 },
    MemoryGet { id: i64 },
    MemoryDelete { id: i64 },
    MemoryList { scope: Option<MemoryScope> },
    /// Leaves a note for another agent (or, with `to_agent: None`, any
    /// agent) working the same project — a minimal inbox, not a live
    /// stream: the recipient picks it up the next time it runs a task in
    /// that project (see `single-runtime::task::run`'s prompt preamble).
    NoteLeave { project: Option<String>, from_agent: String, to_agent: Option<String>, topic: String, content: String },
    /// `to_agent` also matches notes left with `to_agent: None` (broadcast
    /// to the project). `unread_only` additionally filters to `read_at IS
    /// NULL` without marking anything read — see `NoteMarkRead`.
    NoteInbox { project: Option<String>, to_agent: String, unread_only: bool },
    NoteMarkRead { id: i64 },
    /// Extracts text from a PDF/image/plain-text file (OCR fallback for
    /// scanned PDFs) and stores it as a searchable memory entry — see
    /// `single-runtime::documents`.
    DocumentIngest { path: String, project: Option<String>, title: Option<String> },
    DocumentList { project: Option<String> },
    DocumentGet { id: i64 },
    ContextShow { cwd: String },
    TaskRun {
        description: String,
        agent: String,
        cwd: String,
        use_worktree: bool,
        account: Option<String>,
        /// Skips the usual SingleCLI-managed isolated $HOME
        /// (`single_core::agent_home`) and runs the agent against the
        /// real, ambient $HOME instead — for tasks that need to actually
        /// touch the real system (dotfiles, installed packages, desktop
        /// config), not a sandboxed copy. Off by default: this gives the
        /// agent full access to your real credentials and files, an
        /// explicit choice, not the default posture.
        real_home: bool,
        /// Skips injecting a relevant-memory + unread-notes preamble
        /// ahead of the prompt (on by default — see `task::run`'s
        /// context-injection step). Off by default: memory context helps
        /// more often than it costs, but this stays available for prompts
        /// that need to be sent exactly as given.
        no_memory_context: bool,
        timeout_secs: u64,
    },
    TaskList,
    TaskInspect { id: i64 },
    Orchestrate { goal: String, agents: Vec<String>, cwd: String, use_worktree: bool, real_home: bool, timeout_secs: u64 },
    /// Real concurrent execution (v0.1.17), as opposed to `Orchestrate`'s
    /// sequential relay: each `ParallelTaskSpec` runs on its own thread, in
    /// its own git worktree, with its own SQLite connection. There's no
    /// automatic goal decomposition here — the caller supplies each
    /// agent's own description explicitly (SingleCLI runs them, it doesn't
    /// invent the split).
    OrchestrateParallel { tasks: Vec<ParallelTaskSpec>, cwd: String, real_home: bool, timeout_secs: u64 },
    AccountCapture { agent: String, name: String, label: Option<String> },
    AccountUse { agent: String, name: String },
    AccountList { agent: Option<String> },
    AccountRemove { agent: String, name: String },
    AccountSetStatus { agent: String, name: String, status: AccountStatus },
    /// Opt-in Docker execution backend (see `single_core::docker`) —
    /// `account: None` means the agent-wide setting, `Some` overrides it
    /// for one captured account. Takes effect on the next `single task
    /// run`/orchestrate step for that agent/account, not retroactively.
    DockerEnable { agent: String, account: Option<String> },
    DockerDisable { agent: String, account: Option<String> },
    /// `agent: None` lists every configured agent/account pair.
    DockerStatus { agent: Option<String> },
    DockerStop { agent: String, account: Option<String> },
    /// Pending human decisions created by `single_core::preferences::evaluate_and_learn`
    /// — raised by the single-mcp gateway's `invoke_mcp` and, when enabled,
    /// an agent's own mid-run permission hook (see `HooksEnable`). See
    /// `single_core::preferences`.
    ApprovalList,
    /// `remember: true` also records this as a learned preference for the
    /// same resource pattern, so it doesn't ask again next time.
    ApprovalResolve { id: i64, allow: bool, remember: bool },
    PreferenceList,
    /// Opt-in per-agent mid-run permission interception (see
    /// `single_core::hooks`) — an agent's own process pauses mid-task to
    /// ask before using a tool, gated the same way as `single-mcp`'s
    /// `invoke_mcp`. Only `claude` is wired up; other agents error.
    /// Bootstraps the isolated home and writes the hook into its
    /// settings.json immediately, so it takes effect on the next run.
    HooksEnable { agent: String },
    HooksDisable { agent: String },
    HooksStatus,
    ProviderAdd { name: String, env_var_name: String, base_url: Option<String> },
    ProviderAddPreset { name: String },
    ProviderPresetList,
    ProviderRemove { name: String },
    ProviderList,
    /// Same shape as `ProviderList`, filtered to providers that actually
    /// have a key stored (shared `set-key` or any labeled `add-key`) —
    /// `providers.toml` itself carries every built-in preset unconditionally
    /// (see `single_core::providers::sync_missing_presets`; unlike MCP/LSP/
    /// Tools, `ProviderSpec` has no `enabled` field), so plain `ProviderList`
    /// can't answer "which of these did I actually configure."
    ConfiguredProviderList,
    ProviderInspect { name: String },
    ProviderSetKey { name: String, value: String },
    ProviderSync { name: String, agents: Vec<String>, dry_run: bool },
    /// Stores one *labeled* key for a provider (see `ProviderKeySpec`),
    /// distinct from `ProviderSetKey`'s single shared key.
    ProviderAddKey { provider: String, label: String, agent: Option<String>, value: String },
    ProviderListKeys { provider: String },
    ProviderRemoveKey { provider: String, label: String },
    /// Same as `ProviderSync` but syncs one specific labeled key (not the
    /// shared `providers.toml` one) into one specific agent.
    ProviderKeySync { provider: String, label: String, agent: String, dry_run: bool },
    /// The org/admin-scoped key used only to *query* a provider's usage
    /// API — separate from any inference key in `ProviderSpec`/
    /// `ProviderKeySpec`, since billing endpoints typically need a
    /// different credential scope than making model calls.
    ProviderSetBillingKey { provider: String, value: String },
    BillingProviderList,
    UsageShow { provider: Option<String> },
    UsageRefresh,
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
    PluginPresetList,
    PluginAddPreset { name: String },
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
    McpGatewayMode(bool),
    LspServers(Vec<LspServerSpec>),
    LspServer(LspServerSpec),
    LspPresets(Vec<LspPresetInfo>),
    Tools(Vec<ToolSpec>),
    Tool(ToolSpec),
    SecretNames(Vec<String>),
    SecretValue(Option<String>),
    Skills(Vec<String>),
    SkillStarters(Vec<SkillStarterInfo>),
    SkillContents(Vec<String>),
    SkillSynced { path: String },
    MemoryId(i64),
    MemoryEntry(MemoryEntry),
    MemoryEntries(Vec<MemoryEntry>),
    NoteId(i64),
    Notes(Vec<AgentNote>),
    Document(DocumentInfo),
    Documents(Vec<DocumentInfo>),
    Context(ProjectContext),
    Task(TaskRecord),
    AccountProfile(AccountProfileInfo),
    AccountProfiles(Vec<AccountProfileInfo>),
    AccountSwitched(AccountSwitchResult),
    DockerContainerInfo(DockerContainerInfo),
    DockerContainerList(Vec<DockerContainerInfo>),
    Approvals(Vec<ApprovalInfo>),
    /// `(agent, enabled)` pairs — see `single_core::hooks::status`.
    HooksStatus(Vec<(String, bool)>),
    Preferences(Vec<PreferenceInfo>),
    Provider(ProviderSpec),
    Providers(Vec<ProviderSpec>),
    ProviderPresets(Vec<ProviderPresetInfo>),
    ProviderSyncResults(Vec<ProviderSyncResult>),
    ProviderKeys(Vec<ProviderKeySpec>),
    BillingProviders(Vec<BillingProviderInfo>),
    Usage(UsageSummary),
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
    PluginPresets(Vec<PluginPresetInfo>),
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
    /// Auto-detected presence of *some* live login for this agent, checked
    /// across both SingleCLI's isolated home and the real ambient home. See
    /// `AuthState` docs for how this differs from `AccountProfileInfo::status`.
    #[serde(default)]
    pub authenticated: AuthState,
}

/// Whether *some* live login is currently present for an agent, auto-
/// detected by checking for the agent's credential file(s) — no notion of
/// *which* account, just "is anything logged in right now". Distinct from
/// `AccountProfileInfo::status` (`AccountStatus`), which is a manually-set
/// usability flag on one *named, captured* profile and is never auto-
/// detected. `authenticated` answers "can I run this agent at all";
/// `status` answers "is this particular saved account currently usable".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    #[default]
    NotAuthenticated,
    Authenticated,
    /// Account-switching/credential-detection isn't implemented for this
    /// agent (e.g. opencode, perplexity) — see `single-core::account`'s
    /// module docs for why.
    Unsupported,
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
    /// Env var name -> secret-store key (see `single_core::secrets`), for
    /// values that must never sit in plain text in `mcp.toml` the way
    /// `env` does — an API token for Cloudflare/Postman, for example.
    /// Resolved at spawn time, not stored raw: the `single-mcp` gateway
    /// (crates/single-mcp) reads each key from the OS keychain and sets it
    /// as a real env var on the child process it spawns, so the value
    /// never touches disk anywhere. This resolution currently only
    /// happens in the gateway path — direct native-config sync
    /// (`single install-integrations` without gateway mode) still writes
    /// whatever's in `env` verbatim into each agent's own config file,
    /// same as it always has (matching `provider_sync.rs`'s existing
    /// precedent for provider API keys); a secret-backed server synced
    /// that way needs its value put in `env` directly, same as before.
    #[serde(default)]
    pub secret_env: BTreeMap<String, String>,
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

/// A note one agent leaves for another (or for whoever picks up the
/// project next) — a minimal inbox, not a live event stream. See
/// `Request::NoteLeave`/`NoteInbox`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNote {
    pub id: i64,
    pub project: Option<String>,
    pub from_agent: String,
    /// `None` = left for any agent working this project, not one in particular.
    pub to_agent: Option<String>,
    pub topic: String,
    pub content: String,
    pub created_at: String,
    pub read_at: Option<String>,
}

/// An ingested document — see `single-runtime::documents`. The extracted
/// text itself lives in the shared memory store (`memory_id` points at
/// it); this only tracks the original file and OCR provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub id: i64,
    pub title: String,
    pub project: Option<String>,
    pub source_path: String,
    pub extracted_chars: i64,
    pub memory_id: i64,
    pub ingested_at: String,
}

/// One agent/account's Docker execution setting plus (when known) its
/// live container state — see `single_core::docker` (settings) and
/// `single-runtime::docker` (lifecycle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainerInfo {
    pub agent: String,
    pub account: Option<String>,
    pub container_name: String,
    pub enabled: bool,
    /// `None` when the container doesn't exist yet (e.g. never started,
    /// or `enabled` but no task has run since) — distinct from `Some(false)`,
    /// which means it exists but is stopped.
    pub running: Option<bool>,
}

/// A pending or resolved human decision — see `single_core::preferences`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInfo {
    pub id: i64,
    pub resource: String,
    pub context: Option<String>,
    /// `"pending"` / `"allowed"` / `"denied"`.
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

/// A learned decision for a resource pattern — see `single_core::preferences`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceInfo {
    pub id: i64,
    pub pattern: String,
    /// `"deny"` / `"ask"` / `"allow"`.
    pub decision: String,
    pub confidence: f64,
    pub learned_from: Option<String>,
    pub created_at: String,
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

/// One agent's explicit sub-task within a parallel orchestrate batch —
/// see `Request::OrchestrateParallel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelTaskSpec {
    pub agent: String,
    pub description: String,
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
    /// and displays it. See `AuthState` (on `AgentInfo`) for the auto-
    /// detected "is anything logged in" question this does NOT answer.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPresetInfo {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStarterInfo {
    pub name: String,
    pub description: String,
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

/// One labeled API key for a provider, distinct from `ProviderSpec`'s
/// single shared key (`providers.toml`) — lets the same provider have
/// several real keys, one per agent, so `single usage show` can attribute
/// billing-API spend to a specific agent instead of one undifferentiated
/// provider total. `secret_name` is always `"provider-key:{provider}:{label}"`.
/// `label` defaults to `"default"` for the common single-key case, keeping
/// `single provider set-key`'s existing behavior unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderKeySpec {
    pub provider: String,
    pub label: String,
    pub agent: Option<String>,
    pub secret_name: String,
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

/// One billing-provider registry entry (`single_core::billing`) — mirrors
/// `registry::AgentDefinition`'s honesty convention: `verified` is only
/// `true` once that provider's real usage endpoint has actually been
/// called successfully, not assumed from reading its docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingProviderInfo {
    pub provider: String,
    pub verified: bool,
    pub admin_key_env_hint: String,
    pub admin_key_configured: bool,
    pub notes: Option<String>,
}

/// One line item from a provider's real usage/billing API — the `$`
/// SingleCLI didn't compute itself, just relayed. `key_label` is `Some`
/// only where that provider's API exposes a per-key breakdown *and* the
/// key matches a locally-registered `ProviderKeySpec::label`; otherwise
/// it's `None` and the amount is an undifferentiated provider total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub provider: String,
    pub key_label: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub period_start: String,
    pub period_end: String,
}

/// Local-only activity for an agent with no billing-API `$` data (every
/// OAuth-authenticated agent — claude, codex, cursor, copilot, kiro,
/// cody, ...) — sourced from `TaskRecord`, not a provider API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLocalStats {
    pub agent: String,
    pub run_count: u64,
    pub avg_duration_ms: u64,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSummary {
    pub provider_usage: Vec<UsageRecord>,
    pub agent_local_stats: Vec<AgentLocalStats>,
    pub total_usd: f64,
    pub last_refreshed: Option<String>,
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
