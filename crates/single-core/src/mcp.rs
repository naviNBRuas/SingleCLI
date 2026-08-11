//! The unified MCP registry (spec section 11): one list of MCP servers,
//! stored at `~/.config/single/mcp.toml`, that every agent adapter's
//! `configure_mcp` call is given to sync into that agent's native format.
//!
//! Phase 1 ships two default entries — `git` and `memory` — because they
//! were the only two MCP servers observed configured identically across
//! all three real CLIs on the reference machine (`mcp-server-git` via
//! `uvx`, `@modelcontextprotocol/server-memory` via `npx`), making them a
//! reasonable, non-fabricated starting point rather than an arbitrary list.

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
            env: BTreeMap::new(),
            enabled: true,
        },
        McpServerSpec {
            name: "memory".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
            env: BTreeMap::new(),
            enabled: true,
        },
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_defaults_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let servers = load(&dir.path().join("mcp.toml")).unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        let servers = vec![McpServerSpec {
            name: "custom".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "custom-server".into()],
            env: BTreeMap::new(),
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
            env: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn add_appends_and_replaces_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        add(&path, sample()).unwrap();
        assert_eq!(load(&path).unwrap().len(), 3); // git, memory defaults + custom
        let mut replaced = sample();
        replaced.command = "uvx".into();
        add(&path, replaced).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 3);
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
    fn set_enabled_toggles_and_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        add(&path, sample()).unwrap();
        assert!(set_enabled(&path, "custom", false).unwrap());
        assert!(!find(&path, "custom").unwrap().unwrap().enabled);
        assert!(!set_enabled(&path, "ghost", true).unwrap());
    }
}
