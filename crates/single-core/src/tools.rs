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

/// A starter catalog of real, common developer CLI tools — not tied to any
/// one agent, just the metadata/risk-level seam described in the module
/// docs above. Risk levels reflect what the tool can actually do: read-only
/// or scoped-write tools are `Low`/`Medium`; anything that can run
/// arbitrary containers or push to a remote is `High`.
pub fn default_tools() -> Vec<ToolSpec> {
    use single_protocol::RiskLevel;
    vec![
        ToolSpec { name: "git".into(), description: "version control".into(), risk_level: RiskLevel::Medium, enabled: true },
        ToolSpec { name: "docker".into(), description: "container runtime".into(), risk_level: RiskLevel::High, enabled: true },
        ToolSpec { name: "node".into(), description: "JavaScript/TypeScript runtime".into(), risk_level: RiskLevel::Medium, enabled: true },
        ToolSpec { name: "python3".into(), description: "Python runtime".into(), risk_level: RiskLevel::Medium, enabled: true },
        ToolSpec { name: "cargo".into(), description: "Rust build tool and package manager".into(), risk_level: RiskLevel::Medium, enabled: true },
        ToolSpec { name: "curl".into(), description: "HTTP client".into(), risk_level: RiskLevel::Low, enabled: true },
        ToolSpec { name: "gh".into(), description: "GitHub CLI".into(), risk_level: RiskLevel::High, enabled: true },
    ]
}

pub fn load(path: &Path) -> Result<Vec<ToolSpec>> {
    if !path.exists() {
        return Ok(default_tools());
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
    fn load_returns_defaults_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tools = load(&dir.path().join("tools.toml")).unwrap();
        assert_eq!(tools.len(), default_tools().len());
        assert!(tools.iter().any(|t| t.name == "git"));
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
