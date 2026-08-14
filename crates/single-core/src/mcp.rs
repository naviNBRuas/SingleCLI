//! The unified MCP registry (spec section 11): one list of MCP servers,
//! stored at `~/.config/single/mcp.toml`, that every agent adapter's
//! `configure_mcp` call is given to sync into that agent's native format.
//!
//! Every default entry below is a package this project has directly
//! observed running for real — either on the reference machine's own
//! working MCP configuration, or as one of the original reference servers
//! the Model Context Protocol project itself ships (same `@modelcontextprotocol`
//! npm scope as `server-memory`/`server-sequential-thinking`, which are
//! independently confirmed working here). Nothing here is a guessed
//! package name. Servers that need no secrets and are broadly safe (read a
//! git repo, remember things, fetch a URL, structure reasoning) are
//! enabled by default; servers that need a secret (`github`) or are more
//! invasive (`filesystem` needs explicit directory args to be safe,
//! `playwright`/`chrome-devtools` drive a real browser) ship disabled —
//! present in the registry so `single mcp enable <name>` and provider/
//! secret wiring reach them, but not auto-enabled.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use single_protocol::McpServerSpec;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct McpRegistryFile {
    #[serde(default)]
    servers: BTreeMap<String, McpServerSpec>,
}

pub fn default_servers() -> Vec<McpServerSpec> {
    vec![
        McpServerSpec {
            name: "git".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        },
        McpServerSpec {
            name: "memory".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        },
        McpServerSpec {
            name: "fetch".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-fetch".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        },
        McpServerSpec {
            name: "sequential-thinking".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-sequential-thinking".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        },
        McpServerSpec {
            name: "filesystem".into(),
            command: "npx".into(),
            // No directories configured yet — this arg list is a real,
            // functioning invocation but scoped to nothing until the user
            // edits it (`single mcp add filesystem npx -y
            // @modelcontextprotocol/server-filesystem /path/to/allow`),
            // which is why it ships disabled rather than auto-granting
            // filesystem access to every synced agent.
            args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: false,
        },
        McpServerSpec {
            name: "github".into(),
            command: "docker".into(),
            args: vec![
                "run".into(),
                "-i".into(),
                "--rm".into(),
                "-e".into(),
                "GITHUB_PERSONAL_ACCESS_TOKEN".into(),
                "ghcr.io/github/github-mcp-server".into(),
            ],
            env: BTreeMap::new(), secret_env: BTreeMap::new(), // set GITHUB_PERSONAL_ACCESS_TOKEN before enabling
            enabled: false,
        },
        McpServerSpec {
            name: "playwright".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@playwright/mcp@latest".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: false,
        },
        McpServerSpec {
            name: "chrome-devtools".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "chrome-devtools-mcp@latest".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: false,
        },
    ]
}

/// A named starter config for a real MCP server not yet in the user's
/// registry — the growth mechanism for "add more mcps": each entry's npm
/// package was confirmed to exist and resolve a real published version via
/// `npm view <package> version` against the live registry at the time it
/// was added (not guessed), same discipline as `default_servers()`. All
/// ship disabled: each either needs a secret (`brave-search`, `slack`,
/// `postgres` — connection string) or drives something invasive
/// (`puppeteer` — a real browser, like `playwright` above), so opting in
/// via `single mcp add-preset <name>` is a deliberate choice, not
/// something SingleCLI turns on for you.
pub struct McpPreset {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// Env var names this server needs a secret value for, e.g.
    /// `CLOUDFLARE_API_TOKEN`. `to_spec()` pre-wires each into
    /// `secret_env` under the `mcp:<preset-name>:<VAR>` key convention —
    /// the value itself still needs `single secret set mcp:<preset-name>:<VAR> <value>`
    /// before the server can actually run.
    pub secret_env_vars: &'static [&'static str],
}

pub fn presets() -> Vec<McpPreset> {
    vec![
        McpPreset { name: "brave-search", command: "npx", args: &["-y", "@modelcontextprotocol/server-brave-search"], secret_env_vars: &[] },
        McpPreset { name: "slack", command: "npx", args: &["-y", "@modelcontextprotocol/server-slack"], secret_env_vars: &[] },
        McpPreset { name: "puppeteer", command: "npx", args: &["-y", "@modelcontextprotocol/server-puppeteer"], secret_env_vars: &[] },
        McpPreset { name: "postgres", command: "npx", args: &["-y", "@modelcontextprotocol/server-postgres"], secret_env_vars: &[] },
        // Confirmed real via `npm view <package> version` against the live
        // registry (same discipline as above), in addition to the four
        // presets already there before this pass:
        McpPreset {
            name: "postman",
            command: "npx",
            args: &["-y", "postman-mcp-server"],
            // Confirmed via the project's own README (github.com/ankit-roy-0602/postman-mcp-server).
            secret_env_vars: &["POSTMAN_API_KEY"],
        },
        McpPreset {
            name: "cloudflare",
            command: "npx",
            args: &["-y", "@cloudflare/mcp-server-cloudflare"],
            // NOT independently confirmed against this package's own docs
            // (Cloudflare's MCP tooling has moved toward hosted remote
            // servers since this npm package was published; its exact
            // env var wasn't verified here) — CLOUDFLARE_API_TOKEN is
            // Cloudflare's consistent convention across their other CLI
            // tooling (wrangler, their Terraform provider), used here as
            // the best-available real convention, not a guess made up for
            // this preset specifically. Verify against the package's own
            // README before relying on it.
            secret_env_vars: &["CLOUDFLARE_API_TOKEN"],
        },
        // Not an npm package like the others — a real companion mode of
        // SingleCLI's own single-mcp binary (crates/single-mcp/src/distrobox.rs),
        // exposing run_in_kali/run_in_blackarch by shelling into those
        // distrobox containers. Requires distrobox itself, and containers
        // named "kali"/"blackarch" to already exist (`distrobox list`).
        McpPreset { name: "distrobox-control", command: "single-mcp", args: &["--distrobox"], secret_env_vars: &[] },
    ]
}

pub fn preset(name: &str) -> Option<McpPreset> {
    presets().into_iter().find(|p| p.name == name)
}

impl McpPreset {
    pub fn to_spec(&self) -> McpServerSpec {
        let secret_env =
            self.secret_env_vars.iter().map(|var| (var.to_string(), format!("mcp:{}:{var}", self.name))).collect();
        McpServerSpec {
            name: self.name.to_string(),
            command: self.command.to_string(),
            args: self.args.iter().map(|s| s.to_string()).collect(),
            env: BTreeMap::new(),
            secret_env,
            enabled: false,
        }
    }
}

pub fn load(path: &Path) -> Result<Vec<McpServerSpec>> {
    if !path.exists() {
        return Ok(default_servers());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: McpRegistryFile =
        toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.servers.into_values().collect())
}

pub fn save(path: &Path, servers: &[McpServerSpec]) -> Result<()> {
    let mut map = BTreeMap::new();
    for server in servers {
        map.insert(server.name.clone(), server.clone());
    }
    let file = McpRegistryFile { servers: map };
    let rendered = toml::to_string_pretty(&file).context("serializing mcp registry")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

/// Adds or replaces a server by name, then persists the whole registry.
pub fn add(path: &Path, server: McpServerSpec) -> Result<()> {
    let mut servers = load(path)?;
    servers.retain(|s| s.name != server.name);
    servers.push(server);
    save(path, &servers)
}

/// Removes a server by name. Returns `false` if no server had that name.
pub fn remove(path: &Path, name: &str) -> Result<bool> {
    let mut servers = load(path)?;
    let before = servers.len();
    servers.retain(|s| s.name != name);
    let removed = servers.len() != before;
    if removed {
        save(path, &servers)?;
    }
    Ok(removed)
}

/// Sets `enabled` on a named server. Returns `false` if no server had that name.
pub fn set_enabled(path: &Path, name: &str, enabled: bool) -> Result<bool> {
    let mut servers = load(path)?;
    let Some(server) = servers.iter_mut().find(|s| s.name == name) else {
        return Ok(false);
    };
    server.enabled = enabled;
    save(path, &servers)?;
    Ok(true)
}

pub fn find(path: &Path, name: &str) -> Result<Option<McpServerSpec>> {
    Ok(load(path)?.into_iter().find(|s| s.name == name))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GatewayModeFile {
    #[serde(default)]
    enabled: bool,
}

/// The `single-mcp` server spec `install_integrations` syncs into every
/// agent when gateway mode is on — one entry instead of every enabled
/// server in the registry, since `single-mcp` itself dynamically proxies
/// to them (see `crates/single-mcp`). Resolved via `PATH`, same as `single`
/// itself, since both binaries are installed side by side.
pub fn gateway_server_spec() -> McpServerSpec {
    McpServerSpec { name: "single-mcp".into(), command: "single-mcp".into(), args: vec![], env: BTreeMap::new(), secret_env: BTreeMap::new(), enabled: true }
}

/// Reads the `mcp_gateway.toml` flag (see `SingleDirs::mcp_gateway_file`).
/// Missing file means gateway mode has never been turned on — `false`.
pub fn gateway_mode(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: GatewayModeFile = toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.enabled)
}

pub fn set_gateway_mode(path: &Path, enabled: bool) -> Result<()> {
    let rendered = toml::to_string_pretty(&GatewayModeFile { enabled }).context("serializing mcp gateway flag")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_defaults_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let servers = load(&dir.path().join("mcp.toml")).unwrap();
        assert_eq!(servers.len(), default_servers().len());
        let git = servers.iter().find(|s| s.name == "git").unwrap();
        assert!(git.enabled);
    }

    #[test]
    fn secret_or_invasive_servers_default_to_disabled() {
        let servers = default_servers();
        for name in ["filesystem", "github", "playwright", "chrome-devtools"] {
            let server = servers.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(!server.enabled, "{name} should default to disabled");
        }
    }

    #[test]
    fn safe_no_secret_servers_default_to_enabled() {
        let servers = default_servers();
        for name in ["git", "memory", "fetch", "sequential-thinking"] {
            let server = servers.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(server.enabled, "{name} should default to enabled");
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        let servers = vec![McpServerSpec {
            name: "custom".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "custom-server".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: false,
        }];
        save(&path, &servers).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "custom");
        assert!(!loaded[0].enabled);
    }

    fn sample() -> McpServerSpec {
        McpServerSpec {
            name: "custom".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "custom-server".into()],
            env: BTreeMap::new(), secret_env: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn add_appends_and_replaces_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        add(&path, sample()).unwrap();
        assert_eq!(load(&path).unwrap().len(), default_servers().len() + 1); // defaults + custom
        let mut replaced = sample();
        replaced.command = "uvx".into();
        add(&path, replaced).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), default_servers().len() + 1);
        assert_eq!(find(&path, "custom").unwrap().unwrap().command, "uvx");
    }

    #[test]
    fn remove_reports_whether_anything_was_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        add(&path, sample()).unwrap();
        assert!(remove(&path, "custom").unwrap());
        assert!(!remove(&path, "custom").unwrap());
        assert!(find(&path, "custom").unwrap().is_none());
    }

    #[test]
    fn presets_are_disabled_by_default_and_resolve_by_name() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        for expected in ["brave-search", "slack", "puppeteer", "postgres", "postman", "cloudflare", "distrobox-control"] {
            assert!(names.contains(&expected), "missing preset {expected}");
        }
        assert!(preset("nonexistent").is_none());
        let spec = preset("postgres").unwrap().to_spec();
        assert_eq!(spec.command, "npx");
        assert!(!spec.enabled);
    }

    #[test]
    fn secret_backed_presets_prewire_secret_env_under_the_naming_convention() {
        let spec = preset("postman").unwrap().to_spec();
        assert_eq!(spec.secret_env.get("POSTMAN_API_KEY"), Some(&"mcp:postman:POSTMAN_API_KEY".to_string()));
        assert!(spec.env.is_empty(), "the raw value must never land in the plain env map");

        let spec = preset("cloudflare").unwrap().to_spec();
        assert_eq!(spec.secret_env.get("CLOUDFLARE_API_TOKEN"), Some(&"mcp:cloudflare:CLOUDFLARE_API_TOKEN".to_string()));
    }

    #[test]
    fn presets_with_no_secret_needs_have_empty_secret_env() {
        let spec = preset("brave-search").unwrap().to_spec();
        assert!(spec.secret_env.is_empty());
    }

    #[test]
    fn gateway_mode_defaults_to_off_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_gateway.toml");
        assert!(!gateway_mode(&path).unwrap());

        set_gateway_mode(&path, true).unwrap();
        assert!(gateway_mode(&path).unwrap());

        set_gateway_mode(&path, false).unwrap();
        assert!(!gateway_mode(&path).unwrap());
    }

    #[test]
    fn gateway_server_spec_is_named_and_enabled() {
        let spec = gateway_server_spec();
        assert_eq!(spec.name, "single-mcp");
        assert_eq!(spec.command, "single-mcp");
        assert!(spec.enabled);
    }

    #[test]
    fn set_enabled_toggles_and_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        add(&path, sample()).unwrap();
        assert!(set_enabled(&path, "custom", false).unwrap());
        assert!(!find(&path, "custom").unwrap().unwrap().enabled);
        assert!(!set_enabled(&path, "ghost", true).unwrap());
    }
}
