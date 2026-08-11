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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_five_agents() {
        let reg = builtin_registry();
        assert_eq!(reg.len(), 5);
        let names: Vec<_> = reg.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["claude", "codex", "opencode", "agy", "perplexity"]);
    }

    #[test]
    fn every_agent_has_a_verified_bootstrap_install_with_source() {
        let reg = builtin_registry();
        for agent in &reg {
            let install = agent
                .bootstrap_install
                .as_ref()
                .unwrap_or_else(|| panic!("agent {} has no bootstrap install", agent.name));
            assert!(install.command.contains("curl"), "agent {}", agent.name);
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
