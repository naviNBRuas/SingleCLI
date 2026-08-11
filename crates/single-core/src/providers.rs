//! The provider registry (spec section 30): OpenAI, Anthropic, OpenCode
//! Zen, or any other LLM API, stored at `~/.config/single/providers.toml`.
//! The registry only holds *metadata* — which env var name an agent needs,
//! and which secret-store entry holds the actual key
//! (`single-core::secrets`). The key value itself never touches this file.

use anyhow::{Context, Result};
use single_protocol::ProviderSpec;
use std::collections::BTreeMap;
use std::path::Path;

pub fn load(path: &Path) -> Result<Vec<ProviderSpec>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ProviderRegistryFile =
        toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.providers.into_values().collect())
}

pub fn save(path: &Path, providers: &[ProviderSpec]) -> Result<()> {
    let mut map = BTreeMap::new();
    for provider in providers {
        map.insert(provider.name.clone(), provider.clone());
    }
    let file = ProviderRegistryFile { providers: map };
    let rendered = toml::to_string_pretty(&file).context("serializing provider registry")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

pub fn add(path: &Path, provider: ProviderSpec) -> Result<()> {
    let mut providers = load(path)?;
    providers.retain(|p| p.name != provider.name);
    providers.push(provider);
    save(path, &providers)
}

pub fn remove(path: &Path, name: &str) -> Result<bool> {
    let mut providers = load(path)?;
    let before = providers.len();
    providers.retain(|p| p.name != name);
    let removed = providers.len() != before;
    if removed {
        save(path, &providers)?;
    }
    Ok(removed)
}

pub fn find(path: &Path, name: &str) -> Result<Option<ProviderSpec>> {
    Ok(load(path)?.into_iter().find(|p| p.name == name))
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ProviderRegistryFile {
    #[serde(default)]
    providers: BTreeMap<String, ProviderSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProviderSpec {
        ProviderSpec { name: "anthropic".into(), env_var_name: "ANTHROPIC_API_KEY".into(), secret_name: "provider:anthropic".into(), base_url: None }
    }

    #[test]
    fn add_then_find_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        add(&path, sample()).unwrap();
        let found = find(&path, "anthropic").unwrap().unwrap();
        assert_eq!(found.env_var_name, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn add_replaces_existing_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        add(&path, sample()).unwrap();
        let mut updated = sample();
        updated.base_url = Some("https://custom.example.com".into());
        add(&path, updated).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].base_url.as_deref(), Some("https://custom.example.com"));
    }

    #[test]
    fn remove_reports_whether_anything_was_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        add(&path, sample()).unwrap();
        assert!(remove(&path, "anthropic").unwrap());
        assert!(!remove(&path, "anthropic").unwrap());
    }
}
