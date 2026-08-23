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
// -- v0.1.19 additions: all confirmed by installing the real binary and
// running `--help` directly on the reference machine (not taken from
// vendor docs) — see each impl's doc comments for what was actually
// observed. `configure_mcp`/`remove_mcp` stay unsupported for every one of
// these: each vendor CLI does have a real `mcp` subcommand, but none of
// them had a logged-in account to actually run it against and inspect the
// resulting config file's shape, so writing one directly would be a
// guess — same reasoning `kiro` above already documents.
pub struct GeminiAdapter;
pub struct QwenCodeAdapter;
pub struct AmpAdapter;
pub struct DroidAdapter;
pub struct CodebuffAdapter;
pub struct ContinueCliAdapter;
pub struct GrokAdapter;
pub struct CrushAdapter;

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
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_with_prompt_flag("claude", cwd, prompt, backend, live_output_path, timeout, cancel)
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

    fn login_supported(&self) -> bool {
        true
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

    /// `codex exec -s workspace-write --skip-git-repo-check -- "<prompt>"`
    /// — confirmed non-interactive mode via `codex exec --help` on the
    /// reference machine. `--skip-git-repo-check` is also real (confirmed
    /// the same way): without it, `codex exec` refuses to run in a
    /// directory that isn't a trusted git repo, which would otherwise
    /// break `single task run --agent codex` for any `cwd` that isn't
    /// already a repo. This doesn't bypass a SingleCLI-level trust
    /// decision — `cwd` here is already whatever directory the caller (a
    /// plain `task run`, or `orchestrate`'s shared worktree) deliberately
    /// chose; it just stops codex from re-litigating that choice with its
    /// own redundant check.
    ///
    /// `-s workspace-write` is load-bearing, found the hard way: codex
    /// exec's *default* sandbox is `read-only` (confirmed via `codex exec
    /// --help`'s `[possible values: read-only, workspace-write,
    /// danger-full-access]`) — every task ever run through this adapter
    /// before this was added could only inspect the repo, never actually
    /// change anything, and failed silently rather than erroring (codex
    /// just reports it's blocked by the sandbox in its own output, exit
    /// code still looked like success). `workspace-write` scopes writes
    /// to `cwd` (the worktree/directory the caller already chose) without
    /// granting the full host access `danger-full-access` would.
    ///
    /// The `--` before `prompt` is load-bearing too, confirmed live:
    /// `single task run`'s memory/notes preamble starts with a literal
    /// `"---"`, and codex's own parser rejected that as an unrecognized
    /// argument without it — its error message even suggests this exact
    /// fix ("tip: to pass ... as a value, use '-- ...'").
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live(
            "codex",
            &[
                "exec".to_string(),
                "-s".to_string(),
                "workspace-write".to_string(),
                "--skip-git-repo-check".to_string(),
                "--".to_string(),
                prompt.to_string(),
            ],
            cwd,
            backend,
            live_output_path,
            timeout,
            cancel,
        )
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

    fn login_supported(&self) -> bool {
        true
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

    /// `opencode run -- "<prompt>" --dir <cwd>` — confirmed non-interactive
    /// mode and `--dir` flag via `opencode run --help` on the reference
    /// machine. The `--` is load-bearing: `single task run`'s memory/notes
    /// preamble starts with a literal `"---"`, and without a `--`
    /// separator `opencode run` misparsed that as a flag and dumped its
    /// own help instead of running — confirmed live.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live(
            "opencode",
            &["run".to_string(), "--".to_string(), prompt.to_string(), "--dir".to_string(), cwd.display().to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
            cancel,
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

    fn login_supported(&self) -> bool {
        true
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

    /// `agy --print=<prompt>` — confirmed non-interactive print mode via
    /// `agy --help` on the reference machine. `--print` (`-p`'s long form)
    /// takes the prompt as its own value rather than a separate
    /// positional, so a `--` separator doesn't help here the way it does
    /// for claude — confirmed live: `agy -p -- "---..."` still dumped
    /// help, while `agy --print="---..."` correctly ran the prompt.
    /// (`single task run`'s memory/notes preamble literally starts with
    /// `"---"`, which is what surfaced this.)
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("agy", &[format!("--print={prompt}")], cwd, backend, live_output_path, timeout, cancel)
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

    fn login_supported(&self) -> bool {
        true
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

    /// `cursor-agent --trust -p -- "<prompt>"` — confirmed non-interactive
    /// print mode via `cursor-agent --help` on the reference machine.
    /// `--trust` isn't an optional bypass here the way `-f`/`--yolo`
    /// would be (those also auto-approve every tool call) — without it,
    /// `cursor-agent` refuses to run at all in a directory it hasn't seen
    /// before ("Workspace Trust Required"), exiting 1 with no way to
    /// answer the prompt non-interactively. Same "needed to function at
    /// all, not an extra permission grant" reasoning already applied to
    /// codex's `--skip-git-repo-check` and copilot's `--allow-all-tools`.
    ///
    /// The `--` before `prompt` is load-bearing, confirmed by hitting the
    /// failure live: `single task run` prepends a memory/notes preamble
    /// (`task::context_preamble`) that starts with a literal `"---"`, and
    /// `cursor-agent -p "<that text>"` rejects it as an unrecognized
    /// option rather than treating it as `-p`'s value — clap-style
    /// parsers only accept a value that looks like a flag when it's
    /// explicitly separated from option parsing this way.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("cursor-agent", &["--trust".to_string(), "-p".to_string(), "--".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `cursor-agent login` — confirmed real via `cursor-agent --help` on
    /// the reference machine ("Authenticate with Cursor. Set
    /// NO_OPEN_BROWSER to disable browser opening.").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("cursor-agent", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
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
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("aider", &["--message".to_string(), prompt.to_string(), "--yes-always".to_string()], cwd, backend, live_output_path, timeout, cancel)
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

    /// `goose run --text=<prompt> --no-session --quiet` — confirmed
    /// non-interactive mode via `goose run --help` on the reference
    /// machine (`--no-session` skips creating a session file, `--quiet`
    /// prints only the model's response). `--text=value` (not a `--`
    /// separator) is load-bearing: `single task run`'s memory/notes
    /// preamble starts with a literal `"---"`, and `goose run --text
    /// "---..."` (as a separate argv token) rejected it as an unexpected
    /// argument even with `--` inserted before it — confirmed live that
    /// only binding the value directly to `--text` with `=` works.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live(
            "goose",
            &["run".to_string(), format!("--text={prompt}"), "--no-session".to_string(), "--quiet".to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
            cancel,
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

    fn login_supported(&self) -> bool {
        true
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
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("copilot", &["-p".to_string(), prompt.to_string(), "--allow-all-tools".to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `copilot login` — confirmed real via `copilot login --help` on the
    /// reference machine (OAuth device flow).
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("copilot", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
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
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live(
            "kiro-cli",
            &["chat".to_string(), "--no-interactive".to_string(), prompt.to_string(), "--trust-all-tools".to_string()],
            cwd,
            backend,
            live_output_path,
            timeout,
            cancel,
        )
    }

    /// `kiro-cli login` — confirmed real via `kiro-cli login --help` on
    /// the reference machine.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("kiro-cli", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
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
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("cody", &["chat".to_string(), "-m".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `cody auth login --web` — per Sourcegraph's Cody CLI docs.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("cody", &["auth".to_string(), "login".to_string(), "--web".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
    }
}

impl AgentAdapter for GeminiAdapter {
    fn command(&self) -> &str {
        "gemini"
    }

    /// Real `gemini mcp add/remove/list` confirmed via `gemini mcp --help`,
    /// and a documented `mcpServers` key in `~/.gemini/settings.json` —
    /// but no account was logged in to actually produce that file and
    /// confirm its exact shape.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("gemini", home, "gemini mcp add/list/remove is real (confirmed via --help) but settings.json's exact shape wasn't inspected without a logged-in account"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("gemini", home, "gemini mcp add/list/remove is real (confirmed via --help) but settings.json's exact shape wasn't inspected without a logged-in account"))
    }

    /// `gemini --prompt=<prompt> --skip-trust` — confirmed non-interactive
    /// mode via `gemini --help` ("Run in non-interactive (headless) mode
    /// with the given prompt"). `--prompt` takes the value directly
    /// rather than a separate positional, so `--` doesn't help here
    /// (confirmed live: `gemini -p -- "---..."` dumped help; `gemini
    /// --prompt="---..."` ran correctly) — `single task run`'s
    /// memory/notes preamble starts with a literal `"---"`, which is what
    /// surfaced this. `--skip-trust` isn't an optional bypass here the
    /// way `--yolo` would be (that also auto-approves every tool call) —
    /// without it, `gemini` refuses to run at all in a directory it
    /// hasn't seen before ("Gemini CLI is not running in a trusted
    /// directory"), exiting 55 with no way to answer non-interactively —
    /// confirmed live. Same "needed to function at all, not an extra
    /// permission grant" reasoning already applied to codex's
    /// `--skip-git-repo-check` and cursor's `--trust`.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("gemini", &[format!("--prompt={prompt}"), "--skip-trust".to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    // No `login`: `gemini --help` lists no `auth`/`login` subcommand —
    // interactive launches trigger a Google OAuth browser flow on their
    // own the first time, not a separately invokable command.
}

impl AgentAdapter for QwenCodeAdapter {
    fn command(&self) -> &str {
        "qwen"
    }

    /// Real `qwen mcp` subcommand confirmed via `--help` (this fork keeps
    /// Gemini CLI's MCP support), but the settings file shape wasn't
    /// inspected without a logged-in account.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("qwen-code", home, "qwen mcp is real (confirmed via --help) but its settings file shape wasn't inspected without a logged-in account"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("qwen-code", home, "qwen mcp is real (confirmed via --help) but its settings file shape wasn't inspected without a logged-in account"))
    }

    /// `qwen --prompt=<prompt>` — confirmed via `qwen --help` (same flag
    /// behavior as upstream Gemini CLI, which this is forked from —
    /// `--prompt` takes the value directly, `--` doesn't help; see
    /// `GeminiAdapter::run_prompt`'s doc comment for the confirmation).
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("qwen", &[format!("--prompt={prompt}")], cwd, backend, live_output_path, timeout, cancel)
    }

    // No `login`: `qwen auth --help` on the reference machine describes
    // itself as "Configure authentication (removed)" — the subcommand
    // still parses but the vendor's own help text says the feature is
    // gone, so it's not wired up as a real login path here.
}

impl AgentAdapter for AmpAdapter {
    fn command(&self) -> &str {
        "amp"
    }

    /// Real `amp mcp add/list/remove` confirmed via `--help`, and `--help`
    /// even prints the exact settings.json shape (a flat top-level
    /// `"amp.mcpServers"` key) — but that's still the vendor's own
    /// documentation text, not a file this project actually wrote and
    /// re-read on the reference machine, so it stays unsupported rather
    /// than risk a subtly wrong merge.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("amp", home, "amp mcp add/list/remove is real and --help documents a flat \"amp.mcpServers\" key in settings.json, but that shape was never round-tripped against a real file on the reference machine"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("amp", home, "amp mcp add/list/remove is real and --help documents a flat \"amp.mcpServers\" key in settings.json, but that shape was never round-tripped against a real file on the reference machine"))
    }

    /// `amp -x -- "<prompt>"` — confirmed non-interactive mode via `amp
    /// --help` ("Use execute mode ... agent will execute provided
    /// prompt ... Only last assistant message is printed"). `--` before
    /// `prompt`: `single task run`'s memory/notes preamble starts with a
    /// literal `"---"`, which `amp -x "---..."` (no `--`) rejected as an
    /// unknown option — confirmed live. Unlike the other agents fixed the
    /// same way, `amp -x -- "..."` couldn't be confirmed *succeeding*
    /// (no AMP_API_KEY / login on the reference machine, so it just hung
    /// rather than returning a clean auth error) — but it did stop
    /// producing the parse error, matching the same fix pattern verified
    /// end-to-end for codex/opencode/droid/crush.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("amp", &["-x".to_string(), "--".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `amp login` — confirmed real via `amp --help` ("Log in to Amp").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("amp", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
    }
}

impl AgentAdapter for DroidAdapter {
    fn command(&self) -> &str {
        "droid"
    }

    /// Real `droid mcp add/remove/list` confirmed via `droid mcp --help`,
    /// but no account was logged in to inspect the config file it writes.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("droid", home, "droid mcp add/remove/list is real (confirmed via --help) but its config file shape wasn't inspected without a logged-in account"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("droid", home, "droid mcp add/remove/list is real (confirmed via --help) but its config file shape wasn't inspected without a logged-in account"))
    }

    /// `droid exec -- "<prompt>"` — confirmed non-interactive mode via
    /// `droid --help` ("Run non-interactively (for scripts/automation)").
    /// The `--` is load-bearing: `single task run`'s memory/notes preamble
    /// starts with a literal `"---"`, which `droid exec "---..."` (no
    /// `--`) rejected as an unknown option — confirmed live, including
    /// that the fixed form correctly reaches droid's own auth check
    /// instead (a clean, expected error rather than a parse failure).
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("droid", &["exec".to_string(), "--".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    // No `login`: neither `droid --help`, `droid auth --help`, nor `droid
    // login --help` show an auth/login subcommand on the reference
    // machine — Factory's docs describe requiring an account, but the CLI
    // itself doesn't expose a command to drive that from here.
}

impl AgentAdapter for CodebuffAdapter {
    fn command(&self) -> &str {
        "codebuff"
    }

    /// No `mcp` subcommand appears anywhere in `codebuff --help` on the
    /// reference machine — honest gap, not guessed.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("codebuff", home, "codebuff has no confirmed MCP config surface (no mcp subcommand in --help)"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("codebuff", home, "codebuff has no confirmed MCP config surface (no mcp subcommand in --help)"))
    }

    // No `run_prompt`: `codebuff --help` takes a prompt as a positional
    // argument but nothing in --help confirms it exits after one response
    // rather than continuing interactively — left unsupported rather than
    // guessed (unlike the `-p`/`exec` agents above, which have an explicit
    // documented non-interactive flag).

    /// `codebuff login` — confirmed real via `codebuff --help` ("Log in to
    /// your account").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("codebuff", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
    }
}

impl AgentAdapter for ContinueCliAdapter {
    fn command(&self) -> &str {
        "cn"
    }

    /// `cn --help` only offers a per-session `--mcp <hub-slug>` flag (pulls
    /// a server from Continue's hub for that one run), not a persistent
    /// local registry to write into — nothing to configure here.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("continue-cli", home, "cn only supports a per-session --mcp <hub-slug> flag, not a persistent local MCP config to write"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("continue-cli", home, "cn only supports a per-session --mcp <hub-slug> flag, not a persistent local MCP config to write"))
    }

    /// `cn -p "<prompt>"` — confirmed via `cn --help` ("-p, --print: Print
    /// response and exit (useful for pipes)").
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_with_prompt_flag("cn", cwd, prompt, backend, live_output_path, timeout, cancel)
    }

    // No `login`: no auth/login subcommand in `cn --help` on the
    // reference machine.
}

impl AgentAdapter for GrokAdapter {
    fn command(&self) -> &str {
        "grok"
    }

    /// Real `grok mcp` subcommand confirmed via `--help`, but no account
    /// was logged in to inspect the config file it writes.
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("grok", home, "grok mcp is real (confirmed via --help) but its config file shape wasn't inspected without a logged-in account"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("grok", home, "grok mcp is real (confirmed via --help) but its config file shape wasn't inspected without a logged-in account"))
    }

    /// `grok --single=<prompt>` — confirmed via `grok --help` ("-p,
    /// --single <PROMPT>: Single-turn prompt. Prints the response to
    /// stdout and exits"). `--single` takes the value directly, so a `--`
    /// separator doesn't help — confirmed live: `grok -p -- "---..."`
    /// errored "a value is required for '--single <PROMPT>' but none was
    /// supplied", while `grok --single="---..."` ran correctly.
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("grok", &[format!("--single={prompt}")], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `grok login` — confirmed real via `grok --help` ("Sign in to
    /// Grok").
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("grok", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
    }
}

impl AgentAdapter for CrushAdapter {
    fn command(&self) -> &str {
        "crush"
    }

    /// No `mcp` subcommand appears in `crush --help` on the reference
    /// machine (vendor docs describe MCP extensibility via its config
    /// file, but the file wasn't inspected without a logged-in account).
    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("crush", home, "crush's MCP config file wasn't inspected without a logged-in account, and no mcp subcommand appears in --help"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("crush", home, "crush's MCP config file wasn't inspected without a logged-in account, and no mcp subcommand appears in --help"))
    }

    /// `crush run -- "<prompt>"` — confirmed non-interactive mode via
    /// `crush --help` ("Run a single non-interactive prompt"). The `--` is
    /// load-bearing: `single task run`'s memory/notes preamble starts
    /// with a literal `"---"`, which `crush run "---..."` (no `--`)
    /// rejected as bad flag syntax — confirmed live, including that the
    /// fixed form correctly reaches crush's own provider check instead
    /// (a clean, expected error rather than a parse failure).
    #[allow(clippy::too_many_arguments)]
    fn run_prompt(
        &self,
        cwd: &Path,
        prompt: &str,
        backend: &ExecBackend,
        live_output_path: Option<&Path>,
        timeout: Duration,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome> {
        run_command_live("crush", &["run".to_string(), "--".to_string(), prompt.to_string()], cwd, backend, live_output_path, timeout, cancel)
    }

    /// `crush login [platform]` — confirmed real via `crush --help`
    /// ("Login Crush to a platform"). Run without a platform argument, it
    /// prompts interactively for one.
    fn login(&self, home: &Path) -> Result<()> {
        run_interactive_with_home("crush", &["login".to_string()], home)
    }

    fn login_supported(&self) -> bool {
        true
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
        "gemini" => Some(Box::new(GeminiAdapter)),
        "qwen-code" => Some(Box::new(QwenCodeAdapter)),
        "amp" => Some(Box::new(AmpAdapter)),
        "droid" => Some(Box::new(DroidAdapter)),
        "codebuff" => Some(Box::new(CodebuffAdapter)),
        "continue-cli" => Some(Box::new(ContinueCliAdapter)),
        "grok" => Some(Box::new(GrokAdapter)),
        "crush" => Some(Box::new(CrushAdapter)),
        _ => None,
    }
}

/// Same as `for_agent`, but falls back (in order) to a user-defined custom
/// agent (`<custom_agents_dir>/<name>.toml`, see `single-core::custom_agents`)
/// and then to a bare-minimum generic adapter derived from `registry` —
/// detection only (`command -v <command>`), everything else honestly
/// `Unsupported` — for `builtin_registry()` entries that don't have a
/// dedicated Rust adapter (e.g. the v0.1.18 agent-catalog additions). This
/// is the seam that lets a new CLI agent be added without recompiling
/// SingleCLI, and the reason registry entries without a real adapter still
/// get detected by `doctor`/`single setup` instead of silently doing nothing.
pub fn for_agent_with_custom(name: &str, custom_agents_dir: &Path, registry: &[single_core::registry::AgentDefinition]) -> Option<Box<dyn AgentAdapter>> {
    if let Some(builtin) = for_agent(name) {
        return Some(builtin);
    }
    if let Ok((defs, _errors)) = single_core::custom_agents::load_all(custom_agents_dir) {
        if let Some(def) = defs.into_iter().find(|d| d.name == name) {
            return Some(Box::new(crate::generic_adapter::GenericAdapter::new(def)));
        }
    }
    let entry = registry.iter().find(|a| a.name == name)?;
    Some(Box::new(crate::generic_adapter::GenericAdapter::new(single_core::custom_agents::CustomAgentFile {
        name: entry.name.clone(),
        command: entry.command.clone(),
        install: None,
        run: None,
        mcp: None,
    })))
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
        assert!(adapter.run_prompt(dir.path(), "hello", &ExecBackend::host(None), None, Duration::from_secs(1), None).is_err());
    }

    #[test]
    fn for_agent_with_custom_falls_back_to_a_registry_derived_generic_adapter() {
        // A v0.1.18 catalog addition like "plandex" has no dedicated Rust
        // adapter (no ClaudeAdapter-style struct) and no custom_agents.toml
        // override — it should still resolve to a working GenericAdapter
        // built straight from its AgentDefinition, not None. (Its sibling
        // additions gemini/qwen-code/amp/droid/codebuff/continue-cli/grok/
        // crush graduated to real adapters in v0.1.19 — see the top of
        // this file — so this test picks one that's still generic.)
        let dir = tempfile::tempdir().unwrap();
        let registry = single_core::builtin_registry();
        let entry = registry.iter().find(|a| a.name == "plandex").expect("plandex should be in builtin_registry()");

        let adapter = for_agent_with_custom("plandex", dir.path(), &registry).expect("expected a generic-adapter fallback");
        assert_eq!(adapter.command(), entry.command);
        // Detection still does something real (checks PATH), it just won't find it in a test sandbox.
        assert!(!adapter.discover().detected);
    }

    #[test]
    fn for_agent_with_custom_returns_none_for_a_name_in_neither_registry_nor_custom_agents() {
        let dir = tempfile::tempdir().unwrap();
        let registry = single_core::builtin_registry();
        assert!(for_agent_with_custom("totally-made-up-agent", dir.path(), &registry).is_none());
    }
}
