//! The agent registry: the list of AI CLIs SingleCLI knows how to detect,
//! install, and configure.
//!
//! Capability flags and config paths reflect what was directly observed on a
//! real machine with claude/codex/opencode/agy installed. Bootstrap install
//! commands were independently verified against each vendor's own current
//! documentation (fetched directly, not taken on an agent's word) — sources
//! are recorded alongside each command and mirrored in
//! `docs/install-methods.md`.

use serde::{Deserialize, Serialize};
use single_protocol::{BootstrapInstall, CapabilityFlags, InstallMethod};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub adapter: String,
    pub command: String,
    pub install_method: InstallMethod,
    pub bootstrap_install: Option<BootstrapInstall>,
    pub unverified: bool,
    pub capabilities: CapabilityFlags,
    /// Config file paths this adapter can read/write, relative to `$HOME`.
    pub config_paths: Vec<String>,
    pub notes: Option<String>,
}

/// The built-in registry. Phase 1 does not yet support user-defined agent
/// entries layered on top of this (spec section 5 allows it eventually);
/// this function is the seam where that would plug in later.
pub fn builtin_registry() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            name: "claude".into(),
            adapter: "claude-code".into(),
            command: "claude".into(),
            install_method: InstallMethod::Native {
                detail: "Anthropic's native installer; installs to ~/.local/share/claude, \
                         symlinked from ~/.local/bin/claude"
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://claude.ai/install.sh | bash".into(),
                source: "https://code.claude.com/docs/en/setup".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: true,
                mcp: true,
                lsp: true, // observed via ~/.claude/settings.json enabledPlugins (gopls/pyright/etc LSP plugins)
                tools: true,
                sessions: true, // resumable conversation history observed in ~/.claude/history.jsonl
                structured_output: false,
            },
            config_paths: vec![".claude.json".into(), ".claude/settings.json".into()],
            notes: None,
        },
        AgentDefinition {
            name: "codex".into(),
            adapter: "codex".into(),
            command: "codex".into(),
            install_method: InstallMethod::PackageManager {
                detail: "Codex's own standalone package manager under ~/.codex/packages/standalone, \
                         symlinked from ~/.local/bin/codex"
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://chatgpt.com/codex/install.sh | sh".into(),
                source: "https://github.com/openai/codex/blob/main/README.md".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: true,
                mcp: true, // observed [mcp_servers.*] tables in ~/.codex/config.toml
                lsp: false,
                tools: true,
                sessions: true, // ~/.codex/sessions observed on disk
                structured_output: true, // `codex exec` non-interactive mode observed via --help
            },
            config_paths: vec![".codex/config.toml".into()],
            notes: None,
        },
        AgentDefinition {
            name: "opencode".into(),
            adapter: "opencode".into(),
            command: "opencode".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Standalone binary at ~/.opencode/bin/opencode".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://opencode.ai/install | bash".into(),
                source: "https://opencode.ai/docs/".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: true,
                mcp: true, // observed "mcp" key in opencode.jsonc
                lsp: true, // observed "lsp" key in opencode.jsonc
                tools: true,
                sessions: false,
                structured_output: false,
            },
            config_paths: vec![".config/opencode/opencode.jsonc".into()],
            notes: None,
        },
        AgentDefinition {
            name: "agy".into(),
            adapter: "antigravity".into(),
            command: "agy".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Standalone binary at ~/.local/bin/agy; ships its own `agy install` \
                         subcommand for PATH/shell-alias setup"
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://antigravity.google/cli/install.sh | bash".into(),
                source: "https://antigravity.google/docs/cli/install".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: false, // not confirmed; no structured event/streaming flag observed in --help
                mcp: false,       // no on-disk MCP config location found; unconfirmed
                lsp: false,
                tools: true,
                sessions: true, // --continue/--conversation flags observed in --help
                structured_output: false,
            },
            config_paths: vec![], // no config directory found on this machine; adapter shells out to `agy` subcommands instead
            notes: None,
        },
        AgentDefinition {
            name: "perplexity".into(),
            adapter: "perplexity".into(),
            // The real binary this install produces is named `pplx`, not
            // `perplexity` — kept as a distinct field from `name` so the
            // registry key stays stable even though the launch command
            // differs from what the other four entries use.
            command: "pplx".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Perplexity's official CLI is `pplx`, a thin client for the Perplexity \
                         Search API (web search + content-snippet extraction). It is NOT an \
                         interactive coding agent like the other four entries in this registry \
                         — it's the kind of tool a coding agent calls, not one itself."
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://github.com/perplexityai/perplexity-cli/releases/latest/download/install.sh | sh".into(),
                source: "https://docs.perplexity.ai/docs/cli/overview".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: false,
                mcp: false,
                lsp: false,
                tools: true, // usable as a search tool, not as an agent session
                sessions: false,
                structured_output: true, // returns structured JSON per official docs
            },
            config_paths: vec![],
            notes: Some(
                "No official Perplexity coding-agent CLI exists as of this writing. `pplx` is a \
                 Search API client, included here because it's the closest real product and the \
                 registry needed a concrete decision rather than a fabricated agent CLI. Revisit \
                 if/when Perplexity ships an actual coding agent."
                    .into(),
            ),
        },
        AgentDefinition {
            name: "cursor".into(),
            adapter: "cursor".into(),
            // The real binary is `cursor-agent` (the `cursor` shim on this
            // machine just re-execs it) — kept distinct from `name` the
            // same way `perplexity`/`pplx` are above.
            command: "cursor-agent".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Standalone binary installed to ~/.local/bin/cursor-agent by Cursor's own install script".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl https://cursor.com/install -fsS | bash".into(),
                source: "https://cursor.com/docs/cli/installation".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: true, // --output-format stream-json observed in --help
                mcp: true,       // observed real ~/.cursor/mcp.json (mcpServers map) on the reference machine
                lsp: false,
                tools: true,
                sessions: true, // --resume/--continue/ls observed in --help
                structured_output: true, // --output-format json|stream-json observed in --help
            },
            config_paths: vec![".cursor/mcp.json".into()],
            notes: None,
        },
        AgentDefinition {
            name: "aider".into(),
            adapter: "aider".into(),
            command: "aider".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Installed via aider-install (uv-managed isolated environment) or the vendor's own install.sh".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -LsSf https://aider.chat/install.sh | sh".into(),
                source: "https://aider.chat/docs/install.html".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: false,
                mcp: false, // no mcp subcommand/flag found in `aider --help`; unconfirmed
                lsp: false,
                tools: true,
                sessions: false,
                structured_output: false,
            },
            config_paths: vec![".aider.conf.yml".into()],
            notes: Some(
                "Authenticates via API-key flags/env vars (--api-key, --set-env, .env files), \
                 not an interactive OAuth login — `single agent login aider` is unsupported \
                 rather than guessing a flow that doesn't exist."
                    .into(),
            ),
        },
        AgentDefinition {
            name: "goose".into(),
            adapter: "goose".into(),
            command: "goose".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "Standalone binary installed to $GOOSE_BIN_DIR (default ~/.local/bin) by Block's own download_cli.sh".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash".into(),
                source: "https://github.com/block/goose/blob/main/download_cli.sh".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: true, // --output-format stream-json observed in `goose run --help`
                mcp: true,       // observed real ~/.config/goose/config.yaml (extensions map) on the reference machine
                lsp: false,
                tools: true,
                sessions: true, // --resume/--session-id observed in `goose run --help`
                structured_output: true, // --output-format json|stream-json observed in --help
            },
            config_paths: vec![".config/goose/config.yaml".into()],
            notes: None,
        },
        AgentDefinition {
            name: "copilot".into(),
            adapter: "copilot".into(),
            command: "copilot".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "GitHub's official Copilot CLI, standalone binary at ~/.local/bin/copilot \
                         (confirmed on the reference machine: a 177MB binary directly in \
                         ~/.local/bin, not an npm-wrapped script — consistent with the install \
                         script below rather than `npm install -g @github/copilot`, which is \
                         also officially supported)"
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://gh.io/copilot-install | bash".into(),
                source: "https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli".into(),
            }),
            unverified: false,
            capabilities: CapabilityFlags {
                streaming: false,
                mcp: true, // observed real ~/.copilot/mcp-config.json shape by running `copilot mcp add` on the reference machine
                lsp: false,
                tools: true,
                sessions: true, // --resume observed in --help
                structured_output: false,
            },
            config_paths: vec![".copilot/mcp-config.json".into()],
            notes: None,
        },
        AgentDefinition {
            name: "kiro".into(),
            adapter: "kiro".into(),
            command: "kiro-cli".into(),
            install_method: InstallMethod::StandaloneBinary {
                detail: "AWS's Kiro CLI, standalone binary at ~/.local/bin/kiro-cli (confirmed present on the reference machine)".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://cli.kiro.dev/install | bash".into(),
                source: "https://kiro.dev/docs/cli/headless".into(),
            }),
            unverified: false, // installed on the reference machine; `chat`/`mcp`/`login` subcommands and flags confirmed via direct `--help` execution
            capabilities: CapabilityFlags {
                streaming: false,
                mcp: false, // `kiro-cli mcp add` confirmed real via --help, but writes require login, which this project won't do just to inspect a file format
                lsp: false,
                tools: true,
                sessions: true, // --resume/--resume-id/--list-sessions observed in `kiro-cli chat --help`
                structured_output: true, // --format json|json-pretty observed in `kiro-cli chat --help`
            },
            config_paths: vec![],
            notes: Some(
                "`kiro-cli mcp add` is real (confirmed via --help) but requires being logged in \
                 to run, so its on-disk config file format couldn't be inspected without \
                 authenticating a real account just to check a file shape — configure_mcp stays \
                 unsupported rather than guessing it."
                    .into(),
            ),
        },
        AgentDefinition {
            name: "cody".into(),
            adapter: "cody".into(),
            command: "cody".into(),
            install_method: InstallMethod::PackageManager {
                detail: "Sourcegraph's Cody CLI, installed as an npm global package".into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "npm install -g @sourcegraph/cody".into(),
                source: "https://sourcegraph.com/docs/cody/clients/install-cli".into(),
            }),
            unverified: true, // not installed on the reference machine; commands sourced from sourcegraph.com docs, not confirmed by direct execution
            capabilities: CapabilityFlags {
                streaming: false,
                mcp: false, // no MCP subcommand or config surface documented
                lsp: false,
                tools: true,
                sessions: false,
                structured_output: false,
            },
            config_paths: vec![],
            notes: Some(
                "Not installed on the machine this registry was built on; every command here \
                 comes from Sourcegraph's own current docs (fetched directly), not from \
                 running the CLI. Cody CLI is also documented as Experimental for Enterprise \
                 accounts, i.e. Sourcegraph itself doesn't consider it stable yet."
                    .into(),
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_eleven_agents() {
        let reg = builtin_registry();
        assert_eq!(reg.len(), 11);
        let names: Vec<_> = reg.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["claude", "codex", "opencode", "agy", "perplexity", "cursor", "aider", "goose", "copilot", "kiro", "cody"]
        );
    }

    #[test]
    fn every_agent_has_a_verified_bootstrap_install_with_source() {
        let reg = builtin_registry();
        for agent in &reg {
            let install = agent
                .bootstrap_install
                .as_ref()
                .unwrap_or_else(|| panic!("agent {} has no bootstrap install", agent.name));
            // Every install command is a real one sourced from the vendor's own
            // docs — usually a curl script, but `cody`'s only officially
            // documented install path is an npm global install.
            assert!(install.command.contains("curl") || install.command.contains("npm install"), "agent {}", agent.name);
            assert!(install.source.starts_with("https://"), "agent {}", agent.name);
        }
    }

    #[test]
    fn perplexity_entry_is_flagged_as_not_a_coding_agent() {
        let reg = builtin_registry();
        let perplexity = reg.iter().find(|a| a.name == "perplexity").unwrap();
        assert_eq!(perplexity.command, "pplx");
        assert!(perplexity.notes.is_some());
        assert!(!perplexity.capabilities.sessions);
    }
}
