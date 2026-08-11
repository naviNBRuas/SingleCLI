//! The unified tool registry (spec section 13): metadata only in Phase 2.
//! There is no execution engine yet to actually invoke a tool for an agent
//! (that's Phase 4's orchestrator) — this is the catalog + risk metadata
//! seam that engine will read from, kept honestly separate from a promise
//! that tools are wired into any agent today.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use single_protocol::ToolSpec;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ToolRegistryFile {
    #[serde(default)]
    tools: BTreeMap<String, ToolSpec>,
}

pub fn load(path: &Path) -> Result<Vec<ToolSpec>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ToolRegistryFile =
        toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.tools.into_values().collect())
}

pub fn save(path: &Path, tools: &[ToolSpec]) -> Result<()> {
    let mut map = BTreeMap::new();
    for tool in tools {
        map.insert(tool.name.clone(), tool.clone());
    }
    let file = ToolRegistryFile { tools: map };
    let rendered = toml::to_string_pretty(&file).context("serializing tool registry")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

pub fn add(path: &Path, tool: ToolSpec) -> Result<()> {
    let mut tools = load(path)?;
    tools.retain(|t| t.name != tool.name);
    tools.push(tool);
    save(path, &tools)
}

pub fn set_enabled(path: &Path, name: &str, enabled: bool) -> Result<bool> {
    let mut tools = load(path)?;
    let Some(tool) = tools.iter_mut().find(|t| t.name == name) else {
        return Ok(false);
    };
    tool.enabled = enabled;
    save(path, &tools)?;
    Ok(true)
}

pub fn find(path: &Path, name: &str) -> Result<Option<ToolSpec>> {
    Ok(load(path)?.into_iter().find(|t| t.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use single_protocol::RiskLevel;

    fn sample() -> ToolSpec {
        ToolSpec { name: "git".into(), description: "git CLI".into(), risk_level: RiskLevel::Medium, enabled: true }
    }

    #[test]
    fn add_then_find_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.toml");
        add(&path, sample()).unwrap();
        let found = find(&path, "git").unwrap().unwrap();
        assert_eq!(found.risk_level, RiskLevel::Medium);
    }

    #[test]
    fn set_enabled_toggles_and_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.toml");
        add(&path, sample()).unwrap();
        assert!(set_enabled(&path, "git", false).unwrap());
        assert!(!find(&path, "git").unwrap().unwrap().enabled);
        assert!(!set_enabled(&path, "ghost", true).unwrap());
    }
}
