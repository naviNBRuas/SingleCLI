mod client;
mod daemon;
mod render;
mod update;

use clap::{Parser, Subcommand};
use single_core::SingleDirs;
use single_protocol::{LspServerSpec, McpServerSpec, Request, Response, RiskLevel, ToolSpec};
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
    /// Leave/read notes between agents working the same project — a
    /// minimal inbox, not a live stream (see `single-runtime::notes`).
    Note {
        #[command(subcommand)]
        action: NoteCommand,
    },
    /// Ingest a document (PDF/image/text) into the shared memory store — see `single-runtime::documents`.
    Doc {
        #[command(subcommand)]
        action: DocCommand,
    },
    /// Pending human decisions (see `single_core::preferences`) — currently
    /// raised by the single-mcp gateway when a tool call needs approval.
    Approval {
        #[command(subcommand)]
        action: ApprovalCommand,
    },
    /// Learned decisions — what SingleCLI has auto-approved/denied before
    /// without asking, and why (see `single_core::preferences`).
    Preference {
        #[command(subcommand)]
        action: PreferenceCommand,
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
        /// See `single task run --help`'s --real-home — applies to every step.
        #[arg(long)]
        real_home: bool,
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
    /// Undocumented: internal helpers other SingleCLI-owned tooling shells out to.
    #[command(hide = true, subcommand)]
    Internal(InternalCommand),
}

#[derive(Subcommand)]
enum InternalCommand {
    /// Prints a shell script installing every registry agent's real
    /// bootstrap_install command — docker/Dockerfile runs this so the
    /// image build always matches whatever's in
    /// single_core::registry::builtin_registry() at build time, instead
    /// of a separately maintained install list going stale.
    PrintBootstrapScript,
    /// Claude Code's PreToolUse hook (see `single agent hooks enable
    /// claude`): reads the hook's JSON on stdin, evaluates the tool call
    /// against permissions.toml + learned preferences, and — for the
    /// undecided case — blocks polling for a real `single approval
    /// resolve` before answering. Prints the exact
    /// hookSpecificOutput/permissionDecision JSON Claude Code expects.
    #[command(name = "claude-pretooluse-hook")]
    ClaudePreToolUseHook,
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
    /// Interactively log in to this agent's SingleCLI-managed home (never
    /// the real, ambient one — see `docs/architecture.md`'s "Isolation"
    /// section). Runs the agent's own real login command attached to
    /// your terminal (browser OAuth or a prompt, whichever that agent
    /// uses); bootstraps the isolated home first if this is the first
    /// time it's used.
    Login { name: String },
    /// Opt-in Docker execution backend: run this agent's tasks inside a
    /// persistent container instead of on the host. Host isolation
    /// ($HOME-swap) stays the default for everyone else.
    Docker {
        #[command(subcommand)]
        action: AgentDockerCommand,
    },
    /// Opt-in mid-run permission interception: the agent's own process
    /// pauses before using a tool and asks (see `single approval`). Only
    /// `claude` is wired up right now (its PreToolUse hook).
    Hooks {
        #[command(subcommand)]
        action: AgentHooksCommand,
    },
}

#[derive(Subcommand)]
enum AgentHooksCommand {
    Enable { agent: String },
    Disable { agent: String },
    Status,
}

#[derive(Subcommand)]
enum AgentDockerCommand {
    /// Enable for an agent (all its accounts) or one specific captured account with --account.
    Enable {
        agent: String,
        #[arg(long)]
        account: Option<String>,
    },
    Disable {
        agent: String,
        #[arg(long)]
        account: Option<String>,
    },
    /// Show configured agents/accounts and their live container state. Omit `agent` to list all.
    Status { agent: Option<String> },
    Stop {
        agent: String,
        #[arg(long)]
        account: Option<String>,
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
        /// A secret-backed env var this server needs, e.g.
        /// `--secret CLOUDFLARE_API_TOKEN=abc123`. Repeatable. Stored in
        /// the OS keychain (see `single secret`), never written to
        /// mcp.toml in plain text — only resolved at spawn time by the
        /// single-mcp gateway (see `single mcp gateway enable`).
        #[arg(long = "secret", value_parser = parse_key_val)]
        secrets: Vec<(String, String)>,
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
    /// List built-in MCP server presets (brave-search, slack, puppeteer, postgres, cloudflare, postman, distrobox-control).
    Presets,
    /// Register an MCP server from a built-in preset (ships disabled — most need a secret).
    AddPreset { name: String },
    /// Dynamic MCP gateway (crates/single-mcp): when enabled, `single install-integrations`
    /// syncs only single-mcp into agents' native config instead of every enabled server —
    /// single-mcp then proxies to them lazily. Takes effect on the next install-integrations.
    Gateway {
        #[command(subcommand)]
        action: McpGatewayCommand,
    },
}

fn parse_key_val(s: &str) -> anyhow::Result<(String, String)> {
    let (k, v) = s.split_once('=').ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE, got '{s}'"))?;
    Ok((k.to_string(), v.to_string()))
}

#[derive(Subcommand)]
enum McpGatewayCommand {
    Enable,
    Disable,
    Status,
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
    /// Copies a skill into Claude Code's real skill directory
    /// (~/.claude/skills/<name>/) — backs up any existing same-named directory first.
    SyncClaude { name: String },
    /// List the curated starter skills bundled with SingleCLI.
    Starters,
    /// Install a bundled starter skill by name (see `single skill starters`).
    InstallStarter { name: String },
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
        /// Embed the query and search by meaning instead of substring —
        /// requires an embeddings key + SINGLE_QDRANT_URL (falls back to
        /// substring search with a warning otherwise).
        #[arg(long)]
        semantic: bool,
        #[arg(long, default_value = "10")]
        limit: u64,
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
    /// Stores/searches pre-computed vectors directly — for text, use
    /// `single memory search --semantic` instead, which embeds the query
    /// for you (needs an embeddings key too, see `single secret set
    /// embeddings:api_key`).
    Vector {
        #[command(subcommand)]
        action: VectorCommand,
    },
}

#[derive(Subcommand)]
enum NoteCommand {
    /// Leave a note for another agent (or, with no --to, any agent) working this project.
    Leave {
        content: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value = "general")]
        topic: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Read notes addressed to an agent (including broadcast notes) for a project.
    Inbox {
        #[arg(long)]
        to: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        unread_only: bool,
        #[arg(long)]
        json: bool,
    },
    MarkRead { id: i64 },
}

#[derive(Subcommand)]
enum ApprovalCommand {
    List,
    Resolve {
        id: i64,
        #[arg(long, conflicts_with = "deny")]
        allow: bool,
        #[arg(long, conflicts_with = "allow")]
        deny: bool,
        /// Also learn this as a preference for the same resource pattern, so it doesn't ask again.
        #[arg(long)]
        remember: bool,
    },
}

#[derive(Subcommand)]
enum PreferenceCommand {
    List,
}

#[derive(Subcommand)]
enum DocCommand {
    /// Extract text from a PDF/image/text file (OCR fallback for scanned PDFs) and store it as memory.
    Ingest {
        path: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        id: i64,
        #[arg(long)]
        json: bool,
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
    /// List built-in plugin presets, sourced from Anthropic's official
    /// marketplace (github.com/anthropics/claude-plugins-official).
    Presets,
    /// Register a plugin from a built-in preset (verified to sync into `claude`; other agents best-effort).
    AddPreset { name: String },
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
        /// Skip SingleCLI's isolated $HOME and run against your real, ambient one instead —
        /// for tasks that need to actually touch your real system (dotfiles, packages, desktop
        /// config), not a sandboxed copy. The agent gets full access to your real credentials
        /// and files; only use this when that's exactly what you want.
        #[arg(long)]
        real_home: bool,
        /// Skip prepending relevant memory + unread agent notes to the prompt
        /// (on by default — see `single-runtime::task::build_context_preamble`).
        #[arg(long)]
        no_memory_context: bool,
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
        return single_tui::run(dirs);
    };

    // Self-update needs no runtime/socket at all — it just talks to
    // GitHub and replaces files next to the current executable.
    if let Command::Update { channel, check, yes } = command {
        return run_update(&channel, check, yes);
    }

    // Pure local logic — no daemon/socket needed.
    if let Command::Internal(InternalCommand::PrintBootstrapScript) = command {
        print_bootstrap_script();
        return Ok(());
    }
    // Runs synchronously inside Claude Code's own hook lifecycle, blocking
    // it — must not depend on a daemon being reachable, so this talks to
    // SingleCLI's state database directly, same as single-mcp's gateway.
    if let Command::Internal(InternalCommand::ClaudePreToolUseHook) = command {
        return run_claude_pretooluse_hook();
    }

    // Interactive login needs the user's real terminal (browser OAuth
    // round-trips, device codes, password prompts) attached directly —
    // routing it through the daemon over the socket would mean the
    // daemon's stdio, not the user's, which may not even be a TTY. Runs
    // entirely locally: no socket round-trip needed to resolve the
    // isolated home path either (single_core::agent_home is pure logic).
    if let Command::Agent { action: AgentCommand::Login { name } } = &command {
        return run_agent_login(&dirs, &socket_path, name);
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
            AgentCommand::Login { .. } => unreachable!("Command::Agent{{Login}} is intercepted before this match"),
            AgentCommand::Docker { action } => match action {
                AgentDockerCommand::Enable { agent, account } => {
                    let response = client::send(&socket_path, Request::DockerEnable { agent, account })?;
                    render::print(response, false);
                }
                AgentDockerCommand::Disable { agent, account } => {
                    let response = client::send(&socket_path, Request::DockerDisable { agent, account })?;
                    render::print(response, false);
                }
                AgentDockerCommand::Status { agent } => {
                    let response = client::send(&socket_path, Request::DockerStatus { agent })?;
                    render::print(response, false);
                }
                AgentDockerCommand::Stop { agent, account } => {
                    let response = client::send(&socket_path, Request::DockerStop { agent, account })?;
                    render::print(response, false);
                }
            },
            AgentCommand::Hooks { action } => match action {
                AgentHooksCommand::Enable { agent } => {
                    let response = client::send(&socket_path, Request::HooksEnable { agent })?;
                    render::print(response, false);
                }
                AgentHooksCommand::Disable { agent } => {
                    let response = client::send(&socket_path, Request::HooksDisable { agent })?;
                    render::print(response, false);
                }
                AgentHooksCommand::Status => {
                    let response = client::send(&socket_path, Request::HooksStatus)?;
                    render::print(response, false);
                }
            },
        },
        Command::Mcp { action } => match action {
            McpCommand::List { json } => {
                let response = client::send(&socket_path, Request::McpList)?;
                render::print(response, json);
            }
            McpCommand::Add { name, command, secrets, args } => {
                let mut secret_env = BTreeMap::new();
                for (env_var, value) in secrets {
                    let secret_key = format!("mcp:{name}:{env_var}");
                    let response = client::send(&socket_path, Request::SecretSet { name: secret_key.clone(), value })?;
                    render::print(response, false); // exits on failure, prints nothing on success
                    secret_env.insert(env_var, secret_key);
                }
                let server = McpServerSpec { name, command, args, env: BTreeMap::new(), secret_env, enabled: true };
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
            McpCommand::Gateway { action } => match action {
                McpGatewayCommand::Enable => {
                    let response = client::send(&socket_path, Request::McpGatewaySetEnabled { enabled: true })?;
                    render::print(response, false);
                    eprintln!("run `single install-integrations --yes` to apply this to agents' native config.");
                }
                McpGatewayCommand::Disable => {
                    let response = client::send(&socket_path, Request::McpGatewaySetEnabled { enabled: false })?;
                    render::print(response, false);
                    eprintln!("run `single install-integrations --yes` to apply this to agents' native config.");
                }
                McpGatewayCommand::Status => {
                    let response = client::send(&socket_path, Request::McpGatewayStatus)?;
                    render::print(response, false);
                }
            },
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
            SkillCommand::SyncClaude { name } => {
                let response = client::send(&socket_path, Request::SkillSyncClaude { name })?;
                render::print(response, false);
            }
            SkillCommand::Starters => {
                let response = client::send(&socket_path, Request::SkillStarterList)?;
                render::print(response, false);
            }
            SkillCommand::InstallStarter { name } => {
                let response = client::send(&socket_path, Request::SkillInstallStarter { name })?;
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
            MemoryCommand::Search { query, scope, project, semantic, limit, json } => {
                let request = if semantic {
                    Request::MemorySearchSemantic { query, scope: scope.map(Into::into), project, limit }
                } else {
                    Request::MemorySearch { query, scope: scope.map(Into::into), project }
                };
                let response = client::send(&socket_path, request)?;
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
        Command::Approval { action } => match action {
            ApprovalCommand::List => {
                let response = client::send(&socket_path, Request::ApprovalList)?;
                render::print(response, false);
            }
            ApprovalCommand::Resolve { id, allow, deny, remember } => {
                if !allow && !deny {
                    anyhow::bail!("pass --allow or --deny");
                }
                let response = client::send(&socket_path, Request::ApprovalResolve { id, allow, remember })?;
                render::print(response, false);
            }
        },
        Command::Preference { action } => match action {
            PreferenceCommand::List => {
                let response = client::send(&socket_path, Request::PreferenceList)?;
                render::print(response, false);
            }
        },
        Command::Note { action } => match action {
            NoteCommand::Leave { content, from, to, topic, project } => {
                let response = client::send(&socket_path, Request::NoteLeave { project, from_agent: from, to_agent: to, topic, content })?;
                render::print(response, false);
            }
            NoteCommand::Inbox { to, project, unread_only, json } => {
                let response = client::send(&socket_path, Request::NoteInbox { project, to_agent: to, unread_only })?;
                render::print(response, json);
            }
            NoteCommand::MarkRead { id } => {
                let response = client::send(&socket_path, Request::NoteMarkRead { id })?;
                render::print(response, false);
            }
        },
        Command::Doc { action } => match action {
            DocCommand::Ingest { path, project, title } => {
                let path = std::fs::canonicalize(&path).map(|p| p.display().to_string()).unwrap_or(path);
                let response = client::send(&socket_path, Request::DocumentIngest { path, project, title })?;
                render::print(response, false);
            }
            DocCommand::List { project, json } => {
                let response = client::send(&socket_path, Request::DocumentList { project })?;
                render::print(response, json);
            }
            DocCommand::Show { id, json } => {
                let response = client::send(&socket_path, Request::DocumentGet { id })?;
                render::print(response, json);
            }
        },
        Command::Context { cwd, json } => {
            let cwd = cwd.unwrap_or_else(|| ".".to_string());
            let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
            let response = client::send(&socket_path, Request::ContextShow { cwd })?;
            render::print(response, json);
        }
        Command::Task { action } => match action {
            TaskCommand::Run { description, agent, cwd, worktree, account, real_home, no_memory_context, timeout_secs, json } => {
                let cwd = cwd.unwrap_or_else(|| ".".to_string());
                let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
                if real_home {
                    eprintln!("warning: running against your real $HOME — {agent} will see your real credentials/config and can modify real files.");
                }
                let response = client::send(&socket_path, Request::TaskRun {
                    description,
                    agent,
                    cwd,
                    use_worktree: worktree,
                    account,
                    real_home,
                    no_memory_context,
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
        Command::Orchestrate { goal, agents, cwd, worktree, real_home, timeout_secs } => {
            let cwd = cwd.unwrap_or_else(|| ".".to_string());
            let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
            if real_home {
                eprintln!("warning: running against your real $HOME — every agent in this relay will see your real credentials/config and can modify real files.");
            }
            let response = client::send(&socket_path, Request::Orchestrate { goal, agents, cwd, use_worktree: worktree, real_home, timeout_secs })?;
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
            PluginCommand::Presets => {
                let response = client::send(&socket_path, Request::PluginPresetList)?;
                render::print(response, false);
            }
            PluginCommand::AddPreset { name } => {
                let response = client::send(&socket_path, Request::PluginAddPreset { name })?;
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
        Command::Internal(_) => unreachable!("handled before the socket-based dispatch above"),
    }

    Ok(())
}

/// See `InternalCommand::PrintBootstrapScript`'s doc comment.
fn print_bootstrap_script() {
    println!("#!/bin/sh");
    println!("set -u"); // not -e: one agent's install failing shouldn't abort the rest
    for agent in single_core::builtin_registry() {
        let Some(install) = agent.bootstrap_install else { continue };
        println!("echo '==> installing {}'", agent.name);
        println!("if ! ( {} ); then echo 'WARN: {} install failed' >&2; fi", install.command, agent.name);
    }
}

/// See `InternalCommand::ClaudePreToolUseHook`'s doc comment. Contract
/// (stdin/stdout JSON shape) verified against a real installed plugin —
/// see `single_agent_sdk::formats::claude_settings`'s module doc.
fn run_claude_pretooluse_hook() -> anyhow::Result<()> {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input: serde_json::Value = serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let tool_name = input.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    let tool_input = input.get("tool_input").cloned().unwrap_or(serde_json::Value::Null);
    let resource = claude_hook_resource(tool_name, &tool_input);

    let dirs = SingleDirs::discover()?;
    let rules = single_core::permissions::load(&dirs.permissions_file())?;
    let db_path = dirs.db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = rusqlite::Connection::open(&db_path)?;
    single_core::preferences::ensure_schema(&conn)?;

    let verdict = single_core::preferences::evaluate_and_learn(&rules.tools, &conn, &resource, Some("claude PreToolUse hook"))?;
    let output = match verdict {
        single_core::preferences::Verdict::Allow => serde_json::json!({}),
        single_core::preferences::Verdict::Deny => hook_deny_json("blocked by SingleCLI permission policy"),
        single_core::preferences::Verdict::PendingApproval(id) => wait_for_approval(&conn, id)?,
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Polls the pending approval created for this call until a human
/// resolves it via `single approval resolve` (from another terminal or
/// the TUI) or our own margin under `HOOK_TIMEOUT_SECS` runs out —
/// timing out denies (fail closed) rather than guessing.
fn wait_for_approval(conn: &rusqlite::Connection, id: i64) -> anyhow::Result<serde_json::Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(single_core::hooks::CLAUDE_HOOK_TIMEOUT_SECS.saturating_sub(20));
    loop {
        let Some(approval) = single_core::preferences::get_approval(conn, id)? else {
            return Ok(hook_deny_json("approval record disappeared"));
        };
        match approval.status {
            single_core::preferences::ApprovalStatus::Allowed => return Ok(serde_json::json!({})),
            single_core::preferences::ApprovalStatus::Denied => return Ok(hook_deny_json("denied via `single approval resolve`")),
            single_core::preferences::ApprovalStatus::Pending => {
                if std::time::Instant::now() >= deadline {
                    return Ok(hook_deny_json(&format!(
                        "timed out waiting for approval #{id} — run `single approval resolve {id} --allow`, then retry"
                    )));
                }
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
    }
}

fn hook_deny_json(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "deny" },
        "systemMessage": reason
    })
}

/// Builds the `permissions.toml`/learned-preference resource pattern for
/// one Claude tool call. Field names (`tool_input.command`/`file_path`)
/// verified against the same real plugin as `claude_settings.rs`.
fn claude_hook_resource(tool_name: &str, tool_input: &serde_json::Value) -> String {
    match tool_name {
        "Bash" => {
            let command = tool_input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            format!("claude:bash:{command}")
        }
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            let path = tool_input.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            format!("claude:edit:{path}")
        }
        other => format!("claude:{other}"),
    }
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

fn run_agent_login(dirs: &SingleDirs, socket_path: &std::path::Path, agent: &str) -> anyhow::Result<()> {
    let Some(adapter) = single_agent_sdk::adapters::for_agent_with_custom(agent, &dirs.agents_dir()) else {
        anyhow::bail!("unknown agent: {agent}");
    };
    if !adapter.discover().detected {
        anyhow::bail!("{agent} is not installed; run `single agent install {agent} --yes` first");
    }
    let real_home = single_core::paths::real_home_dir()?;
    let home = single_core::agent_home::ensure_bootstrapped(&dirs.homes_dir(), &real_home, agent)?;
    println!("logging in to {agent} (isolated home: {})...", home.display());
    adapter.login(&home)?;
    println!("done.");

    auto_capture_after_login(dirs, socket_path, &home, agent);

    println!("run `single doctor` or `single agent inspect {agent}` to confirm.");
    Ok(())
}

/// After a successful `single agent login`, registers this login as a
/// named account automatically — otherwise it only appears in `single
/// account list`/the TUI Accounts tab after a separate manual `single
/// account capture` call, which is easy to forget and was the source of
/// "I logged in but don't see an account" confusion. Best-effort: login
/// itself already succeeded, so a capture failure here (e.g. an agent with
/// no account-switching support, like opencode) is only a warning.
fn auto_capture_after_login(dirs: &SingleDirs, socket_path: &std::path::Path, home: &std::path::Path, agent: &str) {
    let label = single_core::account::derive_label(home, agent);
    let existing = single_core::account::list(&dirs.accounts_dir(), Some(agent)).unwrap_or_default();

    let base_name = label.clone().unwrap_or_else(|| {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        format!("auto-{secs}")
    });
    let slug: String =
        base_name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' }).collect();
    let mut name = slug.clone();
    let mut suffix = 2;
    while existing.iter().any(|p| p.name == name) {
        name = format!("{slug}-{suffix}");
        suffix += 1;
    }

    match client::send(socket_path, Request::AccountCapture { agent: agent.to_string(), name: name.clone(), label: label.clone() }) {
        Ok(Response::Ok { .. }) => println!("captured as: {}", label.unwrap_or(name)),
        Ok(Response::Error { message }) => {
            eprintln!("note: login succeeded, but auto-capturing an account failed: {message}");
            eprintln!("      run `single account capture {agent} <name>` manually if you want this login saved as an account.");
        }
        Err(e) => eprintln!("note: login succeeded, but auto-capturing an account failed: {e:#}"),
    }
}
