use crate::adapter::{run_with_prompt_flag, AgentAdapter};
use crate::backend::ExecBackend;
use crate::backup::backup_before_write;
use crate::formats;
use crate::run::{run_command_live, run_command_with_home, run_interactive_with_home};
use anyhow::Result;
use single_protocol::{IntegrationWrite, McpServerSpec, RunOutcome};
use std::path::Path;
use std::time::Duration;

pub struct ClaudeAdapter;
pub struct CodexAdapter;
pub struct OpenCodeAdapter;
pub struct AgyAdapter;
pub struct PerplexityAdapter;
pub struct CursorAdapter;
pub struct AiderAdapter;
pub struct GooseAdapter;
pub struct CopilotAdapter;
pub struct KiroAdapter;
pub struct CodyAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn command(&self) -> &str {
        "claude"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".claude.json");
        let updated = formats::claude::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("claude", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".claude.json");
        match formats::claude::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("claude", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("claude", home, "no config file present; nothing to remove")),
        }
    }

    /// `claude -p "<prompt>"` — confirmed non-interactive print mode via
    /// `claude --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_with_prompt_flag("claude", cwd, prompt, backend, live_output_path, timeout)
    }

    /// `claude plugin install <plugin[@marketplace]>` — confirmed real via
    /// `claude plugin --help` on the reference machine (aliased `claude
    /// plugin i`).
    fn install_plugin(&self, target: &str, home: &Path, timeout: Duration) -> Result<RunOutcome> {
        run_command_with_home("claude", &["plugin".to_string(), "install".to_string(), target.to_string()], home, Some(home), timeout)
    }

    /// `claude auth login` — confirmed real via `claude auth --help` on
    /// the reference machine ("Sign in to your Anthropic account").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("claude", &["auth".to_string(), "login".to_string()], home)
    }
}

impl AgentAdapter for CodexAdapter {
    fn command(&self) -> &str {
        "codex"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".codex").join("config.toml");
        let updated = formats::codex::apply(&path, servers)?;
        let rendered = toml::to_string_pretty(&updated)?;
        write_with_backup("codex", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".codex").join("config.toml");
        match formats::codex::remove(&path, names)? {
            Some(updated) => {
                let rendered = toml::to_string_pretty(&updated)?;
                write_with_backup("codex", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("codex", home, "no config file present; nothing to remove")),
        }
    }

    /// `codex exec "<prompt>"` — confirmed non-interactive mode via
    /// `codex exec --help` on the reference machine. `--skip-git-repo-check`
    /// is also real (confirmed the same way): without it, `codex exec`
    /// refuses to run in a directory that isn't a trusted git repo, which
    /// would otherwise break `single task run --agent codex` for any `cwd`
    /// that isn't already a repo. This doesn't bypass a SingleCLI-level
    /// trust decision — `cwd` here is already whatever directory the
    /// caller (a plain `task run`, or `orchestrate`'s shared worktree)
    /// deliberately chose; it just stops codex from re-litigating that
    /// choice with its own redundant check.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live("codex", &["exec".to_string(), "--skip-git-repo-check".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout)
    }

    /// `codex plugin add <plugin[@marketplace]>` — confirmed real via
    /// `codex plugin add --help` on the reference machine. Requires the
    /// marketplace to already be configured (`codex plugin marketplace
    /// add`) if `target` uses `@marketplace` — SingleCLI doesn't auto-add
    /// marketplaces on the user's behalf, since that's a real trust
    /// decision (what code source to pull plugins from), not a config
    /// sync operation.
    fn install_plugin(&self, target: &str, home: &Path, timeout: Duration) -> Result<RunOutcome> {
        run_command_with_home("codex", &["plugin".to_string(), "add".to_string(), target.to_string()], home, Some(home), timeout)
    }

    /// `codex login` — confirmed real via `codex --help` on the
    /// reference machine ("Manage login"; defaults to an interactive
    /// browser OAuth flow).
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("codex", &["login".to_string()], home)
    }
}

impl AgentAdapter for OpenCodeAdapter {
    fn command(&self) -> &str {
        "opencode"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        let updated = formats::opencode::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("opencode", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        match formats::opencode::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("opencode", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("opencode", home, "no config file present; nothing to remove")),
        }
    }

    /// Writes into `opencode.jsonc`'s `lsp` key — the one confirmed real
    /// LSP config surface among the built-in agents (see
    /// `single-core::lsp` and `formats::opencode::apply_lsp` docs).
    fn configure_lsp(&self, home: &Path, servers: &[single_protocol::LspServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        let updated = formats::opencode::apply_lsp(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("opencode", &path, &rendered, dry_run)
    }

    fn remove_lsp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        match formats::opencode::remove_lsp(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("opencode", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("opencode", home, "no config file present; nothing to remove")),
        }
    }

    /// `opencode run "<prompt>" --dir <cwd>` — confirmed non-interactive
    /// mode and `--dir` flag via `opencode run --help` on the reference
    /// machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live(
            "opencode",
            &["run".to_string(), prompt.to_string(), "--dir".to_string(), cwd.display().to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
        )
    }

    /// `opencode plugin <npm-module>` — confirmed real via `opencode
    /// plugin --help` on the reference machine. Unlike claude/codex/agy,
    /// OpenCode addresses plugins by plain npm module name, not
    /// `name@marketplace` — `target` here is expected to already be that
    /// npm module name (the caller picks `PluginSpec::opencode_module`
    /// rather than `PluginSpec::target` before calling this for opencode).
    fn install_plugin(&self, target: &str, home: &Path, timeout: Duration) -> Result<RunOutcome> {
        run_command_with_home("opencode", &["plugin".to_string(), target.to_string()], home, Some(home), timeout)
    }

    /// `opencode auth login` (aliased `opencode providers login`) —
    /// confirmed real via `opencode auth --help` on the reference
    /// machine.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("opencode", &["auth".to_string(), "login".to_string()], home)
    }
}

impl AgentAdapter for AgyAdapter {
    fn command(&self) -> &str {
        "agy"
    }

    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("agy", home, "no on-disk MCP config location has been identified for agy"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("agy", home, "no on-disk MCP config location has been identified for agy"))
    }

    /// `agy -p "<prompt>"` — confirmed non-interactive print mode via
    /// `agy --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_with_prompt_flag("agy", cwd, prompt, backend, live_output_path, timeout)
    }

    /// `agy plugin install <plugin[@marketplace]>` — confirmed real via
    /// `agy plugin --help` on the reference machine.
    fn install_plugin(&self, target: &str, home: &Path, timeout: Duration) -> Result<RunOutcome> {
        run_command_with_home("agy", &["plugin".to_string(), "install".to_string(), target.to_string()], home, Some(home), timeout)
    }
}

impl AgentAdapter for PerplexityAdapter {
    fn command(&self) -> &str {
        "pplx"
    }

    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write(
            "perplexity",
            home,
            "pplx is a Search API client, not an MCP-capable coding agent; nothing to configure",
        ))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write(
            "perplexity",
            home,
            "pplx is a Search API client, not an MCP-capable coding agent; nothing to remove",
        ))
    }

    /// `pplx auth login` — confirmed real via `pplx auth --help` on the
    /// reference machine ("Store a Perplexity API key for the public CLI
    /// (interactive; macOS/Linux only)").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("pplx", &["auth".to_string(), "login".to_string()], home)
    }
}

impl AgentAdapter for CursorAdapter {
    fn command(&self) -> &str {
        "cursor-agent"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".cursor").join("mcp.json");
        let updated = formats::cursor::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("cursor", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".cursor").join("mcp.json");
        match formats::cursor::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("cursor", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("cursor", home, "no config file present; nothing to remove")),
        }
    }

    /// `cursor-agent -p "<prompt>"` — confirmed non-interactive print mode
    /// via `cursor-agent --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live("cursor-agent", &["-p".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout)
    }

    /// `cursor-agent login` — confirmed real via `cursor-agent --help` on
    /// the reference machine ("Authenticate with Cursor. Set
    /// NO_OPEN_BROWSER to disable browser opening.").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("cursor-agent", &["login".to_string()], home)
    }
}

impl AgentAdapter for AiderAdapter {
    fn command(&self) -> &str {
        "aider"
    }

    /// Aider has no confirmed on-disk MCP config location (`aider --help`
    /// shows no `mcp` subcommand or flag) — honest no-op, not a guess.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("aider", home, "aider has no confirmed MCP config surface"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("aider", home, "aider has no confirmed MCP config surface"))
    }

    /// `aider --message "<prompt>" --yes-always` — confirmed non-interactive
    /// mode via `aider --help` on the reference machine (`--yes-always`
    /// skips the confirmation prompts `--message` alone would still hit).
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live("aider", &["--message".to_string(), prompt.to_string(), "--yes-always".to_string()], cwd, backend, live_output_path, timeout)
    }

    // No `login`: aider authenticates via API-key flags/env vars
    // (`--api-key`, `--set-env`, `.env` files), not an interactive OAuth
    // command — there is nothing to attach a terminal session to.
}

impl AgentAdapter for GooseAdapter {
    fn command(&self) -> &str {
        "goose"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("goose").join("config.yaml");
        let updated = formats::goose::apply(&path, servers)?;
        let rendered = serde_yaml::to_string(&updated)?;
        write_with_backup("goose", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("goose").join("config.yaml");
        match formats::goose::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_yaml::to_string(&updated)?;
                write_with_backup("goose", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("goose", home, "no config file present; nothing to remove")),
        }
    }

    /// `goose run --text "<prompt>" --no-session --quiet` — confirmed
    /// non-interactive mode via `goose run --help` on the reference
    /// machine (`--no-session` skips creating a session file, `--quiet`
    /// prints only the model's response).
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live(
            "goose",
            &["run".to_string(), "--text".to_string(), prompt.to_string(), "--no-session".to_string(), "--quiet".to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
        )
    }

    /// `goose configure` — confirmed real via `goose configure --help` on
    /// the reference machine. Not a narrow OAuth "login" like the other
    /// agents' — it's goose's one interactive entry point for setting up
    /// a provider and its API key/credentials, which is the closest real
    /// equivalent goose has.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("goose", &["configure".to_string()], home)
    }
}

impl AgentAdapter for CopilotAdapter {
    fn command(&self) -> &str {
        "copilot"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".copilot").join("mcp-config.json");
        let updated = formats::copilot::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("copilot", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".copilot").join("mcp-config.json");
        match formats::copilot::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("copilot", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("copilot", home, "no config file present; nothing to remove")),
        }
    }

    /// `copilot -p "<prompt>" --allow-all-tools` — confirmed non-interactive
    /// mode via `copilot --help` on the reference machine. `--allow-all-tools`
    /// isn't an optional bypass here the way Claude's
    /// `--dangerously-skip-permissions` is — Copilot's own `--help` states
    /// it's "required for non-interactive mode" (without it `-p` has
    /// nothing to auto-approve tool calls with and can't complete), the
    /// same "needed to function at all, not an extra permission grant"
    /// reasoning already applied to codex's `--skip-git-repo-check`.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live("copilot", &["-p".to_string(), prompt.to_string(), "--allow-all-tools".to_string()], cwd, backend, live_output_path, timeout)
    }

    /// `copilot login` — confirmed real via `copilot login --help` on the
    /// reference machine (OAuth device flow).
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("copilot", &["login".to_string()], home)
    }

    /// `copilot plugin install <source>` — confirmed real via `copilot
    /// plugin install --help` on the reference machine; `source` accepts
    /// the same `plugin@marketplace` convention as claude/codex/agy
    /// (also `owner/repo`, `owner/repo:path`, or a git URL, but
    /// SingleCLI's `PluginSpec::target` is passed through verbatim either
    /// way).
    fn install_plugin(&self, target: &str, home: &Path, timeout: Duration) -> Result<RunOutcome> {
        run_command_with_home("copilot", &["plugin".to_string(), "install".to_string(), target.to_string()], home, Some(home), timeout)
    }
}

impl AgentAdapter for KiroAdapter {
    fn command(&self) -> &str {
        "kiro-cli"
    }

    /// `kiro-cli mcp add` is real — confirmed via `kiro-cli mcp add --help`
    /// on the reference machine, including that it writes to "the global
    /// mcp.json" — but actually running it requires being logged in
    /// ("error: You are not logged in, please log in with kiro-cli
    /// login"), so the exact on-disk shape couldn't be inspected without
    /// authenticating a real account just to check a file format. Left
    /// unsupported rather than guessed.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("kiro-cli", home, "kiro-cli's MCP config file format is not confirmed (writing it requires being logged in)"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("kiro-cli", home, "kiro-cli's MCP config file format is not confirmed (writing it requires being logged in)"))
    }

    /// `kiro-cli chat --no-interactive "<prompt>" --trust-all-tools` —
    /// confirmed real via `kiro-cli chat --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live(
            "kiro-cli",
            &["chat".to_string(), "--no-interactive".to_string(), prompt.to_string(), "--trust-all-tools".to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
        )
    }

    /// `kiro-cli login` — confirmed real via `kiro-cli login --help` on
    /// the reference machine.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("kiro-cli", &["login".to_string()], home)
    }
}

impl AgentAdapter for CodyAdapter {
    fn command(&self) -> &str {
        "cody"
    }

    /// No MCP subcommand or config surface documented on
    /// sourcegraph.com/docs for the Cody CLI — honest gap, not guessed.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("cody", home, "cody has no documented MCP config surface"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("cody", home, "cody has no documented MCP config surface"))
    }

    /// `cody chat -m "<prompt>"` — per Sourcegraph's own Cody CLI install
    /// docs (fetched directly, not run locally — this agent is marked
    /// `unverified` in the registry for that reason).
    fn run_prompt(&self, cwd: &Path, prompt: &str, backend: &ExecBackend, live_output_path: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
        run_command_live("cody", &["chat".to_string(), "-m".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout)
    }

    /// `cody auth login --web` — per Sourcegraph's Cody CLI docs.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("cody", &["auth".to_string(), "login".to_string(), "--web".to_string()], home)
    }
}

fn unsupported_write(agent: &str, home: &Path, detail: &str) -> IntegrationWrite {
    IntegrationWrite {
        agent: agent.to_string(),
        config_path: home.display().to_string(),
        backup_path: None,
        applied: false,
        detail: detail.to_string(),
    }
}

fn write_with_backup(agent: &str, path: &Path, rendered: &str, dry_run: bool) -> Result<IntegrationWrite> {
    if dry_run {
        return Ok(IntegrationWrite {
            agent: agent.to_string(),
            config_path: path.display().to_string(),
            backup_path: None,
            applied: false,
            detail: format!("dry run: would write {} bytes to {}", rendered.len(), path.display()),
        });
    }

    let backup_path = backup_before_write(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered)?;

    Ok(IntegrationWrite {
        agent: agent.to_string(),
        config_path: path.display().to_string(),
        backup_path: backup_path.map(|p| p.display().to_string()),
        applied: true,
        detail: format!("wrote {} bytes", rendered.len()),
    })
}

pub fn for_agent(name: &str) -> Option<Box<dyn AgentAdapter>> {
    match name {
        "claude" => Some(Box::new(ClaudeAdapter)),
        "codex" => Some(Box::new(CodexAdapter)),
        "opencode" => Some(Box::new(OpenCodeAdapter)),
        "agy" => Some(Box::new(AgyAdapter)),
        "perplexity" => Some(Box::new(PerplexityAdapter)),
        "cursor" => Some(Box::new(CursorAdapter)),
        "aider" => Some(Box::new(AiderAdapter)),
        "goose" => Some(Box::new(GooseAdapter)),
        "copilot" => Some(Box::new(CopilotAdapter)),
        "kiro" => Some(Box::new(KiroAdapter)),
        "cody" => Some(Box::new(CodyAdapter)),
        _ => None,
    }
}

/// Same as `for_agent`, but falls back to a user-defined custom agent
/// (`<custom_agents_dir>/<name>.toml`, see `single-core::custom_agents`)
/// when `name` isn't one of the five built-in agents. This is the seam
/// that lets a new CLI agent be added without recompiling SingleCLI.
pub fn for_agent_with_custom(name: &str, custom_agents_dir: &Path) -> Option<Box<dyn AgentAdapter>> {
    if let Some(builtin) = for_agent(name) {
        return Some(builtin);
    }
    let (defs, _errors) = single_core::custom_agents::load_all(custom_agents_dir).ok()?;
    let def = defs.into_iter().find(|d| d.name == name)?;
    Some(Box::new(crate::generic_adapter::GenericAdapter::new(def)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_servers() -> Vec<McpServerSpec> {
        vec![McpServerSpec {
            name: "git".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        }]
    }

    #[test]
    fn claude_adapter_writes_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::write(home.join(".claude.json"), r#"{"numStartups": 1}"#).unwrap();

        let adapter = ClaudeAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), false).unwrap();
        assert!(result.applied);
        assert!(result.backup_path.is_some());

        let written = std::fs::read_to_string(home.join(".claude.json")).unwrap();
        assert!(written.contains("mcpServers"));
        assert!(written.contains("numStartups"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let adapter = ClaudeAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), true).unwrap();
        assert!(!result.applied);
        assert!(!home.join(".claude.json").exists());
    }

    #[test]
    fn codex_adapter_writes_toml() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let adapter = CodexAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), false).unwrap();
        assert!(result.applied);
        let written = std::fs::read_to_string(home.join(".codex").join("config.toml")).unwrap();
        assert!(written.contains("[mcp_servers.git]"));
    }

    #[test]
    fn agy_configure_is_a_documented_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = AgyAdapter;
        let result = adapter.configure_mcp(dir.path(), &sample_servers(), false).unwrap();
        assert!(!result.applied);
    }

    #[test]
    fn for_agent_returns_none_for_unknown_name() {
        assert!(for_agent("nonexistent").is_none());
        assert!(for_agent("claude").is_some());
    }

    #[test]
    fn perplexity_run_prompt_is_unsupported_by_default() {
        // pplx is a Search API client, not a coding agent — it should fall
        // through to AgentAdapter's default "unsupported" implementation
        // rather than silently claiming to run a prompt against it.
        let dir = tempfile::tempdir().unwrap();
        let adapter = PerplexityAdapter;
        assert!(adapter.run_prompt(dir.path(), "hello", &ExecBackend::host(None), None, Duration::from_secs(1)).is_err());
    }
}
