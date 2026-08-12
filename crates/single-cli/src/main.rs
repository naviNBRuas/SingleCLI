mod client;
mod daemon;
mod render;
mod update;

use clap::{Parser, Subcommand};
use single_core::SingleDirs;
use single_protocol::{LspServerSpec, McpServerSpec, Request, RiskLevel, ToolSpec};
use std::collections::BTreeMap;

#[derive(Parser)]
#[command(name = "single", version, about = "SingleCLI — unified control plane for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show runtime status.
    Status,
    /// Diagnose installed agent CLIs, config, and runtime health.
    Doctor,
    /// Install missing agent CLIs and sync SingleCLI's config into all of them.
    Setup {
        /// Actually run install commands and write config. Without this, only shows the plan.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Manage the agent registry.
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
    /// Manage the unified MCP registry.
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
    /// Manage the unified LSP registry.
    Lsp {
        #[command(subcommand)]
        action: LspCommand,
    },
    /// Manage the tool registry (metadata only — no execution engine yet).
    Tool {
        #[command(subcommand)]
        action: ToolCommand,
    },
    /// Manage secrets (OS keychain-backed; Linux via secret-tool in Phase 2).
    Secret {
        #[command(subcommand)]
        action: SecretCommand,
    },
    /// Manage skills (local directories under ~/.config/single/skills).
    Skill {
        #[command(subcommand)]
        action: SkillCommand,
    },
    /// Manage the shared memory store.
    Memory {
        #[command(subcommand)]
        action: MemoryCommand,
    },
    /// Show resolved project context (git state, project docs) for a directory.
    Context {
        /// Defaults to the current directory.
        cwd: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run a task: delegate a prompt to one real agent CLI, synchronously.
    Task {
        #[command(subcommand)]
        action: TaskCommand,
    },
    /// Run several agents in sequence on one goal: each agent runs in the
    /// same shared git worktree and is handed the previous agent's real
    /// output. A sequential relay, not live parallel/bidirectional
    /// multi-agent chat — see docs/architecture.md for the honest scope.
    Orchestrate {
        goal: String,
        #[arg(long, value_delimiter = ',', required = true)]
        agents: Vec<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        worktree: bool,
        #[arg(long, default_value = "300")]
        timeout_secs: u64,
    },
    /// Switch between multiple logged-in accounts for an agent (e.g. two
    /// Claude Code accounts). Log in normally with the agent's own CLI
    /// first, then capture that login state as a named profile.
    Account {
        #[command(subcommand)]
        action: AccountCommand,
    },
    /// Manage LLM provider API keys (OpenAI, Anthropic, OpenCode Zen, ...)
    /// and sync them into agents that support custom providers.
    Provider {
        #[command(subcommand)]
        action: ProviderCommand,
    },
    /// Manage plugins and sync installs into agents that have a real
    /// plugin-install command (claude, codex, opencode, agy).
    Plugin {
        #[command(subcommand)]
        action: PluginCommand,
    },
    /// Manage profiles.
    Profile {
        #[command(subcommand)]
        action: ProfileCommand,
    },
    /// Check for or apply a newer SingleCLI build from GitHub Releases.
    Update {
        #[arg(long, default_value = "stable")]
        channel: String,
        /// Only report whether a newer build is available; don't download or replace anything.
        #[arg(long)]
        check: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Sync SingleCLI's MCP registry into every agent's native config.
    InstallIntegrations {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove SingleCLI-managed entries from every agent's native config.
    UninstallIntegrations {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum AgentCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Install a single agent (dry run by default).
    Install {
        name: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        command: String,
        /// Extra arguments passed to `command`, in order (may start with `-`, e.g. `-y`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Remove {
        name: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// List built-in MCP server presets (brave-search, slack, puppeteer, postgres).
    Presets,
    /// Register an MCP server from a built-in preset (ships disabled — most need a secret).
    AddPreset { name: String },
}

#[derive(Subcommand)]
enum LspCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        command: String,
        /// File extensions this server handles, e.g. .rs .toml
        #[arg(long, value_delimiter = ' ')]
        extensions: Vec<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Remove {
        name: String,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// List built-in LSP presets (rust-analyzer, pyright, typescript, gopls, dockerfile, clangd, bash, yaml, terraform, json).
    Presets,
    /// Register an LSP server from a built-in preset.
    AddPreset { name: String },
}

#[derive(Subcommand)]
enum ToolCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        description: String,
        #[arg(long, value_enum, default_value = "medium")]
        risk: RiskArg,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum RiskArg {
    Low,
    Medium,
    High,
}

#[derive(Subcommand)]
enum SecretCommand {
    List,
    Set { name: String, value: String },
    Get { name: String },
    Delete { name: String },
}

#[derive(Subcommand)]
enum SkillCommand {
    List,
    Install { name: String, source_path: String },
    Remove { name: String },
    Inspect { name: String },
}

#[derive(Subcommand)]
enum MemoryCommand {
    /// Store a memory entry.
    Store {
        title: String,
        content: String,
        #[arg(long, value_enum)]
        scope: Option<MemoryScopeArg>,
        #[arg(long, value_enum)]
        source: Option<MemorySourceArg>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        confidence: Option<f64>,
        /// Auto-delete after this many seconds.
        #[arg(long)]
        expires_in: Option<i64>,
    },
    /// Substring search over title + content (not semantic search — see docs/architecture.md).
    Search {
        query: String,
        #[arg(long, value_enum)]
        scope: Option<MemoryScopeArg>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Get {
        id: i64,
        #[arg(long)]
        json: bool,
    },
    Delete {
        id: i64,
    },
    List {
        #[arg(long, value_enum)]
        scope: Option<MemoryScopeArg>,
        #[arg(long)]
        json: bool,
    },
    /// Knowledge-graph memory: entities, observations, and typed relations
    /// between them — a "powerful shared brain" agents can build up over
    /// time, distinct from the scoped store/search memory above.
    Graph {
        #[command(subcommand)]
        action: KgCommand,
    },
    /// Fast ephemeral Redis-backed working memory. Requires SINGLE_REDIS_URL.
    Cache {
        #[command(subcommand)]
        action: CacheCommand,
    },
    /// Vector store for RAG (Qdrant-backed). Requires SINGLE_QDRANT_URL.
    /// Stores/searches pre-computed vectors — turning text into a vector
    /// (embedding) isn't wired to a live provider yet, see docs/architecture.md.
    Vector {
        #[command(subcommand)]
        action: VectorCommand,
    },
}

#[derive(Subcommand)]
enum VectorCommand {
    /// vector is a comma-separated list of floats, e.g. 0.1,0.2,0.3
    Upsert {
        collection: String,
        id: u64,
        #[arg(long, value_delimiter = ',')]
        vector: Vec<f32>,
        #[arg(long, default_value = "{}")]
        payload: String,
    },
    Search {
        collection: String,
        #[arg(long, value_delimiter = ',')]
        vector: Vec<f32>,
        #[arg(long, default_value = "5")]
        limit: u64,
    },
    Delete { collection: String, id: u64 },
    Status,
}

#[derive(Subcommand)]
enum CacheCommand {
    Set {
        key: String,
        value: String,
        #[arg(long)]
        ttl_secs: Option<u64>,
    },
    Get { key: String },
    Delete { key: String },
    List {
        #[arg(default_value = "*")]
        pattern: String,
    },
    Status,
}

#[derive(Subcommand)]
enum KgCommand {
    CreateEntity { name: String, entity_type: String },
    AddObservation { entity: String, content: String },
    CreateRelation { from: String, to: String, relation_type: String },
    DeleteEntity { name: String },
    Get {
        name: String,
        #[arg(long)]
        json: bool,
    },
    Query {
        term: String,
        #[arg(long)]
        json: bool,
    },
    /// Dump the full graph.
    Show {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum MemoryScopeArg {
    Working,
    Project,
    User,
    Agent,
    Task,
    LongTerm,
    Knowledge,
}

impl From<MemoryScopeArg> for single_protocol::MemoryScope {
    fn from(v: MemoryScopeArg) -> Self {
        use single_protocol::MemoryScope::*;
        match v {
            MemoryScopeArg::Working => Working,
            MemoryScopeArg::Project => Project,
            MemoryScopeArg::User => User,
            MemoryScopeArg::Agent => Agent,
            MemoryScopeArg::Task => Task,
            MemoryScopeArg::LongTerm => LongTerm,
            MemoryScopeArg::Knowledge => Knowledge,
        }
    }
}

#[derive(Clone, clap::ValueEnum)]
enum MemorySourceArg {
    UserInstruction,
    AgentOutput,
    ToolOutput,
    ProjectContent,
    ExternalContent,
}

impl From<MemorySourceArg> for single_protocol::MemorySource {
    fn from(v: MemorySourceArg) -> Self {
        use single_protocol::MemorySource::*;
        match v {
            MemorySourceArg::UserInstruction => UserInstruction,
            MemorySourceArg::AgentOutput => AgentOutput,
            MemorySourceArg::ToolOutput => ToolOutput,
            MemorySourceArg::ProjectContent => ProjectContent,
            MemorySourceArg::ExternalContent => ExternalContent,
        }
    }
}

#[derive(Subcommand)]
enum ProviderCommand {
    /// Register a provider. The key itself is set separately with `set-key`.
    Add {
        name: String,
        /// The environment variable name an agent needs to see this key as, e.g. ANTHROPIC_API_KEY.
        #[arg(long)]
        env_var: String,
        #[arg(long)]
        base_url: Option<String>,
    },
    Remove { name: String },
    List {
        #[arg(long)]
        json: bool,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Store the actual API key in the OS keychain.
    SetKey { name: String, value: String },
    /// Write the key into the named agents' real config (all registered agents if none given).
    Sync {
        name: String,
        #[arg(long, value_delimiter = ',')]
        agents: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    /// List built-in provider presets (OpenAI, Anthropic, OpenCode Zen, NVIDIA).
    Presets,
    /// Register a provider from a built-in preset (name, env var, base URL already filled in).
    AddPreset { name: String },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Register a plugin. `target` is the `plugin[@marketplace]` selector
    /// used verbatim by claude/codex/agy; `--opencode-module` is the plain
    /// npm module name OpenCode's real `opencode plugin <module>` command
    /// needs instead (a genuinely different addressing scheme, not set
    /// automatically from `target`).
    Add {
        name: String,
        target: String,
        #[arg(long)]
        opencode_module: Option<String>,
    },
    Remove { name: String },
    List {
        #[arg(long)]
        json: bool,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Install the plugin into the named agents (all registered agents if none given).
    Sync {
        name: String,
        #[arg(long, value_delimiter = ',')]
        agents: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Capture the agent's currently-live login state as a named profile.
    /// Log in normally with the agent's own CLI first.
    Capture {
        agent: String,
        name: String,
        /// Human-readable identity (email/display name) so multiple
        /// accounts per agent are distinguishable later.
        #[arg(long)]
        label: Option<String>,
    },
    /// Swap a captured profile into place as the agent's live login state
    /// (backs up whatever was live first).
    Use { agent: String, name: String },
    List {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Remove { agent: String, name: String },
    /// Manually record whether an account is usable, rate-limited, or
    /// needs a top-up. Never auto-detected (no verified quota API across
    /// agents) — you or a failed task tell SingleCLI, and it remembers.
    SetStatus {
        agent: String,
        name: String,
        /// One of: available, rate_limited, needs_topup, unknown
        status: String,
    },
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Run a prompt against a real agent CLI and block until it finishes.
    Run {
        description: String,
        #[arg(long)]
        agent: String,
        /// Directory to run in; defaults to the current directory.
        #[arg(long)]
        cwd: Option<String>,
        /// Isolate the run in a new git worktree + branch (requires `cwd` to be inside a git repo).
        #[arg(long)]
        worktree: bool,
        /// Run as this captured account (isolated $HOME — see `single account capture`)
        /// instead of the real one, so multiple accounts of the same agent can run concurrently.
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value = "300")]
        timeout_secs: u64,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Inspect {
        id: i64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Use { name: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let dirs = SingleDirs::discover()?;
    dirs.ensure_created()?;
    let socket_path = dirs.socket_path();

    let Some(command) = cli.command else {
        daemon::ensure_running(&dirs)?;
        return single_tui::run(&socket_path);
    };

    // Self-update needs no runtime/socket at all — it just talks to
    // GitHub and replaces files next to the current executable.
    if let Command::Update { channel, check, yes } = command {
        return run_update(&channel, check, yes);
    }

    match command {
        Command::Status => {
            let response = client::send(&socket_path, Request::Status)?;
            render::print(response, false);
        }
        Command::Doctor => {
            let response = client::send(&socket_path, Request::Doctor)?;
            render::print(response, false);
        }
        Command::Setup { yes, json } => {
            if !yes {
                eprintln!("Dry run (pass --yes to actually install and configure). This may run vendor install scripts over the network.");
            }
            let response = client::send(&socket_path, Request::Setup { dry_run: !yes })?;
            render::print(response, json);
        }
        Command::Agent { action } => match action {
            AgentCommand::List { json } => {
                let response = client::send(&socket_path, Request::AgentList)?;
                render::print(response, json);
            }
            AgentCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::AgentInspect { name })?;
                render::print(response, json);
            }
            AgentCommand::Install { name, yes, json } => {
                if !yes {
                    eprintln!("Dry run (pass --yes to actually run the install command).");
                }
                let response = client::send(&socket_path, Request::AgentInstall { name, dry_run: !yes })?;
                render::print(response, json);
            }
        },
        Command::Mcp { action } => match action {
            McpCommand::List { json } => {
                let response = client::send(&socket_path, Request::McpList)?;
                render::print(response, json);
            }
            McpCommand::Add { name, command, args } => {
                let server = McpServerSpec { name, command, args, env: BTreeMap::new(), enabled: true };
                let response = client::send(&socket_path, Request::McpAdd { server })?;
                render::print(response, false);
            }
            McpCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::McpRemove { name })?;
                render::print(response, false);
            }
            McpCommand::Enable { name } => {
                let response = client::send(&socket_path, Request::McpEnable { name })?;
                render::print(response, false);
            }
            McpCommand::Disable { name } => {
                let response = client::send(&socket_path, Request::McpDisable { name })?;
                render::print(response, false);
            }
            McpCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::McpInspect { name })?;
                render::print(response, json);
            }
            McpCommand::Presets => {
                let response = client::send(&socket_path, Request::McpPresetList)?;
                render::print(response, false);
            }
            McpCommand::AddPreset { name } => {
                let response = client::send(&socket_path, Request::McpAddPreset { name })?;
                render::print(response, false);
            }
        },
        Command::Lsp { action } => match action {
            LspCommand::List { json } => {
                let response = client::send(&socket_path, Request::LspList)?;
                render::print(response, json);
            }
            LspCommand::Add { name, command, extensions, args } => {
                let server = LspServerSpec { name, command, args, extensions, enabled: true };
                let response = client::send(&socket_path, Request::LspAdd { server })?;
                render::print(response, false);
            }
            LspCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::LspRemove { name })?;
                render::print(response, false);
            }
            LspCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::LspInspect { name })?;
                render::print(response, json);
            }
            LspCommand::Presets => {
                let response = client::send(&socket_path, Request::LspPresetList)?;
                render::print(response, false);
            }
            LspCommand::AddPreset { name } => {
                let response = client::send(&socket_path, Request::LspAddPreset { name })?;
                render::print(response, false);
            }
        },
        Command::Tool { action } => match action {
            ToolCommand::List { json } => {
                let response = client::send(&socket_path, Request::ToolList)?;
                render::print(response, json);
            }
            ToolCommand::Add { name, description, risk } => {
                let risk_level = match risk {
                    RiskArg::Low => RiskLevel::Low,
                    RiskArg::Medium => RiskLevel::Medium,
                    RiskArg::High => RiskLevel::High,
                };
                let tool = ToolSpec { name, description, risk_level, enabled: true };
                let response = client::send(&socket_path, Request::ToolAdd { tool })?;
                render::print(response, false);
            }
            ToolCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::ToolInspect { name })?;
                render::print(response, json);
            }
            ToolCommand::Enable { name } => {
                let response = client::send(&socket_path, Request::ToolEnable { name })?;
                render::print(response, false);
            }
            ToolCommand::Disable { name } => {
                let response = client::send(&socket_path, Request::ToolDisable { name })?;
                render::print(response, false);
            }
        },
        Command::Secret { action } => match action {
            SecretCommand::List => {
                let response = client::send(&socket_path, Request::SecretList)?;
                render::print(response, false);
            }
            SecretCommand::Set { name, value } => {
                let response = client::send(&socket_path, Request::SecretSet { name, value })?;
                render::print(response, false);
            }
            SecretCommand::Get { name } => {
                let response = client::send(&socket_path, Request::SecretGet { name })?;
                render::print(response, false);
            }
            SecretCommand::Delete { name } => {
                let response = client::send(&socket_path, Request::SecretDelete { name })?;
                render::print(response, false);
            }
        },
        Command::Skill { action } => match action {
            SkillCommand::List => {
                let response = client::send(&socket_path, Request::SkillList)?;
                render::print(response, false);
            }
            SkillCommand::Install { name, source_path } => {
                let response = client::send(&socket_path, Request::SkillInstall { name, source_path })?;
                render::print(response, false);
            }
            SkillCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::SkillRemove { name })?;
                render::print(response, false);
            }
            SkillCommand::Inspect { name } => {
                let response = client::send(&socket_path, Request::SkillInspect { name })?;
                render::print(response, false);
            }
        },
        Command::Memory { action } => match action {
            MemoryCommand::Store { title, content, scope, source, project, agent, task, confidence, expires_in } => {
                let response = client::send(&socket_path, Request::MemoryStore {
                    scope: scope.map(Into::into),
                    source: source.map(Into::into),
                    project,
                    agent,
                    task,
                    title,
                    content,
                    confidence,
                    expires_in_seconds: expires_in,
                })?;
                render::print(response, false);
            }
            MemoryCommand::Search { query, scope, project, json } => {
                let response = client::send(&socket_path, Request::MemorySearch {
                    query,
                    scope: scope.map(Into::into),
                    project,
                })?;
                render::print(response, json);
            }
            MemoryCommand::Get { id, json } => {
                let response = client::send(&socket_path, Request::MemoryGet { id })?;
                render::print(response, json);
            }
            MemoryCommand::Delete { id } => {
                let response = client::send(&socket_path, Request::MemoryDelete { id })?;
                render::print(response, false);
            }
            MemoryCommand::List { scope, json } => {
                let response = client::send(&socket_path, Request::MemoryList { scope: scope.map(Into::into) })?;
                render::print(response, json);
            }
            MemoryCommand::Graph { action } => match action {
                KgCommand::CreateEntity { name, entity_type } => {
                    let response = client::send(&socket_path, Request::KgCreateEntity { name, entity_type })?;
                    render::print(response, false);
                }
                KgCommand::AddObservation { entity, content } => {
                    let response = client::send(&socket_path, Request::KgAddObservation { entity, content })?;
                    render::print(response, false);
                }
                KgCommand::CreateRelation { from, to, relation_type } => {
                    let response = client::send(&socket_path, Request::KgCreateRelation { from, to, relation_type })?;
                    render::print(response, false);
                }
                KgCommand::DeleteEntity { name } => {
                    let response = client::send(&socket_path, Request::KgDeleteEntity { name })?;
                    render::print(response, false);
                }
                KgCommand::Get { name, json } => {
                    let response = client::send(&socket_path, Request::KgGetEntity { name })?;
                    render::print(response, json);
                }
                KgCommand::Query { term, json } => {
                    let response = client::send(&socket_path, Request::KgQuery { term })?;
                    render::print(response, json);
                }
                KgCommand::Show { json } => {
                    let response = client::send(&socket_path, Request::KgReadGraph)?;
                    render::print(response, json);
                }
            },
            MemoryCommand::Cache { action } => match action {
                CacheCommand::Set { key, value, ttl_secs } => {
                    let response = client::send(&socket_path, Request::CacheSet { key, value, ttl_secs })?;
                    render::print(response, false);
                }
                CacheCommand::Get { key } => {
                    let response = client::send(&socket_path, Request::CacheGet { key })?;
                    render::print(response, false);
                }
                CacheCommand::Delete { key } => {
                    let response = client::send(&socket_path, Request::CacheDelete { key })?;
                    render::print(response, false);
                }
                CacheCommand::List { pattern } => {
                    let response = client::send(&socket_path, Request::CacheList { pattern })?;
                    render::print(response, false);
                }
                CacheCommand::Status => {
                    let response = client::send(&socket_path, Request::CacheStatus)?;
                    render::print(response, false);
                }
            },
            MemoryCommand::Vector { action } => match action {
                VectorCommand::Upsert { collection, id, vector, payload } => {
                    let payload: serde_json::Value = serde_json::from_str(&payload)?;
                    let response = client::send(&socket_path, Request::VectorUpsert { collection, id, vector, payload })?;
                    render::print(response, false);
                }
                VectorCommand::Search { collection, vector, limit } => {
                    let response = client::send(&socket_path, Request::VectorSearch { collection, vector, limit })?;
                    render::print(response, false);
                }
                VectorCommand::Delete { collection, id } => {
                    let response = client::send(&socket_path, Request::VectorDelete { collection, id })?;
                    render::print(response, false);
                }
                VectorCommand::Status => {
                    let response = client::send(&socket_path, Request::VectorStatus)?;
                    render::print(response, false);
                }
            },
        },
        Command::Context { cwd, json } => {
            let cwd = cwd.unwrap_or_else(|| ".".to_string());
            let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
            let response = client::send(&socket_path, Request::ContextShow { cwd })?;
            render::print(response, json);
        }
        Command::Task { action } => match action {
            TaskCommand::Run { description, agent, cwd, worktree, account, timeout_secs, json } => {
                let cwd = cwd.unwrap_or_else(|| ".".to_string());
                let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
                let response = client::send(&socket_path, Request::TaskRun {
                    description,
                    agent,
                    cwd,
                    use_worktree: worktree,
                    account,
                    timeout_secs,
                })?;
                render::print(response, json);
            }
            TaskCommand::List { json } => {
                let response = client::send(&socket_path, Request::TaskList)?;
                render::print(response, json);
            }
            TaskCommand::Inspect { id, json } => {
                let response = client::send(&socket_path, Request::TaskInspect { id })?;
                render::print(response, json);
            }
        },
        Command::Orchestrate { goal, agents, cwd, worktree, timeout_secs } => {
            let cwd = cwd.unwrap_or_else(|| ".".to_string());
            let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
            let response = client::send(&socket_path, Request::Orchestrate { goal, agents, cwd, use_worktree: worktree, timeout_secs })?;
            render::print(response, false);
        }
        Command::Provider { action } => match action {
            ProviderCommand::Add { name, env_var, base_url } => {
                let response = client::send(&socket_path, Request::ProviderAdd { name, env_var_name: env_var, base_url })?;
                render::print(response, false);
            }
            ProviderCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::ProviderRemove { name })?;
                render::print(response, false);
            }
            ProviderCommand::List { json } => {
                let response = client::send(&socket_path, Request::ProviderList)?;
                render::print(response, json);
            }
            ProviderCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::ProviderInspect { name })?;
                render::print(response, json);
            }
            ProviderCommand::SetKey { name, value } => {
                let response = client::send(&socket_path, Request::ProviderSetKey { name, value })?;
                render::print(response, false);
            }
            ProviderCommand::Sync { name, agents, yes } => {
                if !yes {
                    eprintln!("Dry run (pass --yes to actually write the key into agent config files; backups are made either way).");
                }
                let response = client::send(&socket_path, Request::ProviderSync { name, agents, dry_run: !yes })?;
                render::print(response, false);
            }
            ProviderCommand::Presets => {
                let response = client::send(&socket_path, Request::ProviderPresetList)?;
                render::print(response, false);
            }
            ProviderCommand::AddPreset { name } => {
                let response = client::send(&socket_path, Request::ProviderAddPreset { name })?;
                render::print(response, false);
            }
        },
        Command::Plugin { action } => match action {
            PluginCommand::Add { name, target, opencode_module } => {
                let response = client::send(&socket_path, Request::PluginAdd { plugin: single_protocol::PluginSpec { name, target, opencode_module } })?;
                render::print(response, false);
            }
            PluginCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::PluginRemove { name })?;
                render::print(response, false);
            }
            PluginCommand::List { json } => {
                let response = client::send(&socket_path, Request::PluginList)?;
                render::print(response, json);
            }
            PluginCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::PluginInspect { name })?;
                render::print(response, json);
            }
            PluginCommand::Sync { name, agents, yes } => {
                if !yes {
                    eprintln!("Dry run (pass --yes to actually install the plugin into agent CLIs).");
                }
                let response = client::send(&socket_path, Request::PluginSync { name, agents, dry_run: !yes })?;
                render::print(response, false);
            }
        },
        Command::Account { action } => match action {
            AccountCommand::Capture { agent, name, label } => {
                let response = client::send(&socket_path, Request::AccountCapture { agent, name, label })?;
                render::print(response, false);
            }
            AccountCommand::Use { agent, name } => {
                let response = client::send(&socket_path, Request::AccountUse { agent, name })?;
                render::print(response, false);
            }
            AccountCommand::List { agent, json } => {
                let response = client::send(&socket_path, Request::AccountList { agent })?;
                render::print(response, json);
            }
            AccountCommand::Remove { agent, name } => {
                let response = client::send(&socket_path, Request::AccountRemove { agent, name })?;
                render::print(response, false);
            }
            AccountCommand::SetStatus { agent, name, status } => {
                let status = single_protocol::AccountStatus::parse(&status)
                    .ok_or_else(|| anyhow::anyhow!("invalid status '{status}' (expected: available, rate_limited, needs_topup, unknown)"))?;
                let response = client::send(&socket_path, Request::AccountSetStatus { agent, name, status })?;
                render::print(response, false);
            }
        },
        Command::Profile { action } => match action {
            ProfileCommand::List => {
                let response = client::send(&socket_path, Request::ProfileList)?;
                render::print(response, false);
            }
            ProfileCommand::Use { name } => {
                let response = client::send(&socket_path, Request::ProfileUse { name })?;
                render::print(response, false);
            }
        },
        Command::InstallIntegrations { yes, json } => {
            if !yes {
                eprintln!("Dry run (pass --yes to actually write config files; backups are made either way).");
            }
            let response = client::send(&socket_path, Request::InstallIntegrations { dry_run: !yes })?;
            render::print(response, json);
        }
        Command::UninstallIntegrations { yes } => {
            if !yes {
                anyhow::bail!("this removes SingleCLI-managed MCP entries from every agent's config; pass --yes to confirm");
            }
            let response = client::send(&socket_path, Request::UninstallIntegrations)?;
            render::print(response, false);
        }
        Command::Update { .. } => unreachable!("handled before the socket-based dispatch above"),
    }

    Ok(())
}

fn run_update(channel: &str, check_only: bool, yes: bool) -> anyhow::Result<()> {
    println!("current version: {}", update::current_version());
    let release = update::check_latest(channel)?;
    println!("latest {channel}: {}", release.tag);

    let newer = update::is_newer(update::current_version(), &release.tag);
    match newer {
        Some(false) => {
            println!("already up to date.");
            return Ok(());
        }
        Some(true) => println!("update available: {} -> {}", update::current_version(), release.tag),
        None => println!("(can't compare versions for this tag; the {channel} channel always has the latest build available)"),
    }

    if check_only {
        return Ok(());
    }
    if !yes {
        eprintln!("pass --yes to download and install this update.");
        return Ok(());
    }

    println!("downloading and installing...");
    let install_dir = update::apply(&release)?;
    println!("updated in {}", install_dir.display());
    Ok(())
}
