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
}
