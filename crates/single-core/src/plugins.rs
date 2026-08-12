//! The unified plugin registry (spec section 29): one list of plugins,
//! stored at `~/.config/single/plugins.toml`, synced out to every agent
//! that has a real plugin-install command. Verified real commands per
//! agent (`--help` output on the reference machine, not assumed):
//! `claude plugin install <plugin[@marketplace]>`, `codex plugin add
//! <plugin[@marketplace]>`, `agy plugin install <target>` (same
//! `plugin@marketplace` convention as claude/codex), and `opencode plugin
//! <npm-module>` (a genuinely different addressing scheme — a plain npm
//! module name, no marketplace concept).

use anyhow::{Context, Result};
use single_protocol::PluginSpec;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct PluginRegistryFile {
    #[serde(default)]
    plugins: BTreeMap<String, PluginSpec>,
}

pub fn load(path: &Path) -> Result<Vec<PluginSpec>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: PluginRegistryFile = toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.plugins.into_values().collect())
}

pub fn save(path: &Path, plugins: &[PluginSpec]) -> Result<()> {
    let mut map = BTreeMap::new();
    for plugin in plugins {
        map.insert(plugin.name.clone(), plugin.clone());
    }
    let file = PluginRegistryFile { plugins: map };
    let rendered = toml::to_string_pretty(&file).context("serializing plugin registry")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

pub fn add(path: &Path, plugin: PluginSpec) -> Result<()> {
    let mut plugins = load(path)?;
    plugins.retain(|p| p.name != plugin.name);
    plugins.push(plugin);
    save(path, &plugins)
}

pub fn remove(path: &Path, name: &str) -> Result<bool> {
    let mut plugins = load(path)?;
    let before = plugins.len();
    plugins.retain(|p| p.name != name);
    let removed = plugins.len() != before;
    if removed {
        save(path, &plugins)?;
    }
    Ok(removed)
}

pub fn find(path: &Path, name: &str) -> Result<Option<PluginSpec>> {
    Ok(load(path)?.into_iter().find(|p| p.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PluginSpec {
        PluginSpec { name: "example".into(), target: "example@official".into(), opencode_module: Some("@example/opencode-plugin".into()) }
    }

    #[test]
    fn add_then_find_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugins.toml");
        add(&path, sample()).unwrap();
        let found = find(&path, "example").unwrap().unwrap();
        assert_eq!(found.target, "example@official");
        assert_eq!(found.opencode_module.as_deref(), Some("@example/opencode-plugin"));
    }

    #[test]
    fn remove_reports_whether_anything_was_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugins.toml");
        add(&path, sample()).unwrap();
        assert!(remove(&path, "example").unwrap());
        assert!(!remove(&path, "example").unwrap());
    }
}
