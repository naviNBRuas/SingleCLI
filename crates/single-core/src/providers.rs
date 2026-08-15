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

/// Registers every built-in preset not already present in the registry.
/// Safe to bulk-add: a `ProviderSpec` is just metadata (env var name, base
/// URL, and a pointer to a secret-store entry) — no key material moves and
/// nothing is used by any agent until its secret is actually set. Idempotent:
/// safe to call on every startup. Returns how many were newly added.
pub fn sync_missing_presets(path: &Path) -> Result<usize> {
    let mut providers = load(path)?;
    let existing: std::collections::HashSet<String> = providers.iter().map(|p| p.name.clone()).collect();
    let mut added = 0;
    for preset in presets() {
        if !existing.contains(preset.name) {
            providers.push(preset.to_spec());
            added += 1;
        }
    }
    if added > 0 {
        save(path, &providers)?;
    }
    Ok(added)
}

/// A well-known preset a user can pick from instead of typing an env var
/// name/base URL by hand (e.g. in the TUI's "add provider" flow).
/// `secret_name` is deliberately not part of the preset — it's always
/// derived as `provider:<name>` when the preset is turned into a real
/// `ProviderSpec`, keeping that convention in one place.
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub name: &'static str,
    pub env_var_name: &'static str,
    pub base_url: &'static str,
}

/// Every base URL/env var pair here was verified against the vendor's own
/// current documentation, not guessed:
/// - OpenAI, Anthropic: their well-documented standard API endpoints.
/// - OpenCode Zen: `https://opencode.ai/zen/v1`, `OPENCODE_API_KEY` (opencode.ai/docs/providers).
/// - NVIDIA: `https://integrate.api.nvidia.com/v1`, `NVIDIA_API_KEY` (build.nvidia.com's own OpenAI-compatible endpoint docs).
///
/// v0.1.18 additions (each confirmed against the vendor's own current docs,
/// not guessed — all are OpenAI-compatible Chat Completions endpoints):
/// - Groq: `https://api.groq.com/openai/v1` (console.groq.com/docs/openai)
/// - DeepSeek: `https://api.deepseek.com` (api-docs.deepseek.com)
/// - Mistral: `https://api.mistral.ai/v1` (docs.mistral.ai)
/// - xAI (Grok): `https://api.x.ai/v1` (docs.x.ai)
/// - Google Gemini: `https://generativelanguage.googleapis.com/v1beta/openai/` (ai.google.dev/gemini-api/docs/openai)
/// - OpenRouter: `https://openrouter.ai/api/v1` (openrouter.ai/docs)
/// - Together AI: `https://api.together.xyz/v1` (docs.together.ai)
/// - Fireworks AI: `https://api.fireworks.ai/inference/v1` (docs.fireworks.ai)
/// - Cerebras: `https://api.cerebras.ai/v1` (inference-docs.cerebras.ai)
/// - SambaNova: `https://api.sambanova.ai/v1` (docs.sambanova.ai)
/// - DeepInfra: `https://api.deepinfra.com/v1/openai` (docs.deepinfra.com)
/// - Perplexity: `https://api.perplexity.ai` (docs.perplexity.ai)
/// - Cohere: `https://api.cohere.ai/compatibility/v1` (docs.cohere.com)
pub fn presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset { name: "openai", env_var_name: "OPENAI_API_KEY", base_url: "https://api.openai.com/v1" },
        ProviderPreset { name: "anthropic", env_var_name: "ANTHROPIC_API_KEY", base_url: "https://api.anthropic.com" },
        ProviderPreset { name: "opencode-zen", env_var_name: "OPENCODE_API_KEY", base_url: "https://opencode.ai/zen/v1" },
        ProviderPreset { name: "nvidia", env_var_name: "NVIDIA_API_KEY", base_url: "https://integrate.api.nvidia.com/v1" },
        ProviderPreset { name: "groq", env_var_name: "GROQ_API_KEY", base_url: "https://api.groq.com/openai/v1" },
        ProviderPreset { name: "deepseek", env_var_name: "DEEPSEEK_API_KEY", base_url: "https://api.deepseek.com" },
        ProviderPreset { name: "mistral", env_var_name: "MISTRAL_API_KEY", base_url: "https://api.mistral.ai/v1" },
        ProviderPreset { name: "xai", env_var_name: "XAI_API_KEY", base_url: "https://api.x.ai/v1" },
        ProviderPreset {
            name: "gemini",
            env_var_name: "GEMINI_API_KEY",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/",
        },
        ProviderPreset { name: "openrouter", env_var_name: "OPENROUTER_API_KEY", base_url: "https://openrouter.ai/api/v1" },
        ProviderPreset { name: "together", env_var_name: "TOGETHER_API_KEY", base_url: "https://api.together.xyz/v1" },
        ProviderPreset { name: "fireworks", env_var_name: "FIREWORKS_API_KEY", base_url: "https://api.fireworks.ai/inference/v1" },
        ProviderPreset { name: "cerebras", env_var_name: "CEREBRAS_API_KEY", base_url: "https://api.cerebras.ai/v1" },
        ProviderPreset { name: "sambanova", env_var_name: "SAMBANOVA_API_KEY", base_url: "https://api.sambanova.ai/v1" },
        ProviderPreset { name: "deepinfra", env_var_name: "DEEPINFRA_API_KEY", base_url: "https://api.deepinfra.com/v1/openai" },
        ProviderPreset { name: "perplexity", env_var_name: "PERPLEXITY_API_KEY", base_url: "https://api.perplexity.ai" },
        ProviderPreset { name: "cohere", env_var_name: "COHERE_API_KEY", base_url: "https://api.cohere.ai/compatibility/v1" },
    ]
}

pub fn preset(name: &str) -> Option<ProviderPreset> {
    presets().into_iter().find(|p| p.name == name)
}

impl ProviderPreset {
    pub fn to_spec(&self) -> ProviderSpec {
        ProviderSpec {
            name: self.name.to_string(),
            env_var_name: self.env_var_name.to_string(),
            secret_name: format!("provider:{}", self.name),
            base_url: Some(self.base_url.to_string()),
        }
    }
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

    #[test]
    fn presets_include_the_four_original_providers() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        for name in ["openai", "anthropic", "opencode-zen", "nvidia"] {
            assert!(names.contains(&name), "missing original preset {name}");
        }
    }

    #[test]
    fn v0_1_18_catalog_expansion_added_at_least_13_new_providers_with_unique_names_and_https_urls() {
        let presets = presets();
        assert!(presets.len() >= 17, "expected at least 17 total providers (4 original + 13 new), got {}", presets.len());

        let mut names: Vec<&str> = presets.iter().map(|p| p.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate provider preset name");

        for preset in &presets {
            assert!(preset.base_url.starts_with("https://"), "{} base_url should be https", preset.name);
            assert!(preset.env_var_name.ends_with("_API_KEY"), "{} env var should end in _API_KEY", preset.name);
        }
    }

    #[test]
    fn sync_missing_presets_registers_every_preset_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");

        let added_first = sync_missing_presets(&path).unwrap();
        assert_eq!(added_first, presets().len());

        let providers = load(&path).unwrap();
        for preset in presets() {
            assert!(providers.iter().any(|p| p.name == preset.name), "missing {}", preset.name);
        }

        let added_second = sync_missing_presets(&path).unwrap();
        assert_eq!(added_second, 0, "second run should be a no-op");
    }

    #[test]
    fn sync_missing_presets_never_overwrites_an_already_registered_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        let first_preset = presets().first().unwrap().name.to_string();
        add(&path, ProviderSpec {
            name: first_preset.clone(),
            env_var_name: "CUSTOM_KEY".into(),
            secret_name: "provider:custom".into(),
            base_url: Some("https://custom.example.com".into()),
        })
        .unwrap();

        sync_missing_presets(&path).unwrap();

        let provider = find(&path, &first_preset).unwrap().unwrap();
        assert_eq!(provider.env_var_name, "CUSTOM_KEY");
    }

    #[test]
    fn preset_to_spec_derives_secret_name_convention() {
        let spec = preset("nvidia").unwrap().to_spec();
        assert_eq!(spec.secret_name, "provider:nvidia");
        assert_eq!(spec.env_var_name, "NVIDIA_API_KEY");
        assert_eq!(spec.base_url.as_deref(), Some("https://integrate.api.nvidia.com/v1"));
    }

    #[test]
    fn preset_returns_none_for_unknown_name() {
        assert!(preset("does-not-exist").is_none());
    }
}
