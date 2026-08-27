# Custom Providers and OmniRoute Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user register any custom/local OpenAI-compatible LLM provider and have it actually show up in opencode's model picker (not just as an env var floating in its process), then deploy OmniRoute (a self-hosted free-tier LLM gateway) on this host and register it through that same mechanism.

**Architecture:** Extend `ProviderSpec` with a `models` field, add a new `formats::opencode::apply_provider` JSON writer (matching the existing `apply`/`apply_lsp` Value-returning convention in that file), wire it into `provider_sync::sync()`'s existing per-agent match, then deploy OmniRoute as a systemd user service and register it as a provider using the new capability — zero OmniRoute-specific code.

**Tech Stack:** Rust (single-protocol, single-core, single-agent-sdk, single-runtime, single-cli crates), Node.js/npm (OmniRoute itself), systemd user services.

**Spec:** `docs/superpowers/specs/2026-08-26-custom-providers-and-omniroute-design.md`

## Global Constraints

- All new struct fields are additive except `provider_sync::sync()`'s signature (internal function, not a public tool schema — no external caller breaks).
- Commit format: `type: description`, no `Co-Authored-By` or other trailer, sole author `Navin B. Ruas <founder@nbr.company>` (set locally in this repo).
- No `kind: Preset | Custom` label is added to providers — decided (user: "choose what's best," and the spec's own reasoning holds: it would be a label with no behavioral difference, `presets()`'s fixed list already answers "is this built-in").
- OmniRoute deploys on this host (Debian machine), NOT inside the `SingleCLI` or `The-Company` repos/directories.
- `opencode.jsonc`'s `provider.<name>.options.apiKey` must be the literal string `"{env:<VAR_NAME>}"` (an env-var *reference* opencode resolves itself at runtime), never the raw secret value — the raw secret only ever reaches opencode's process environment via the existing `resolve_env_for_agent` mechanism, never the config file.
- Node/npm are already available on this host via nvm: `node` v24.19.0, `npm` 11.17.0, global npm prefix `/home/navinbruas/.nvm/versions/node/v24.19.0`. `omniroute`'s binary will land at `/home/navinbruas/.nvm/versions/node/v24.19.0/bin/omniroute` after install — use this absolute path in the systemd unit (not bare `omniroute`, since systemd user services don't source nvm's shell hooks).
- Workspace version: currently `0.7.0` (post orchestration-lessons merge) → bump to `0.8.0` for this plan's feature addition, per this project's own SemVer convention (minor = new functionality, pre-1.0).

---

### Task 1: `ProviderSpec.models` field, `ModelSpec` struct, CLI `--model` flag

**Files:**
- Modify: `crates/single-protocol/src/lib.rs:1386-1391` (`ProviderSpec`), add `ModelSpec` struct nearby, `:395-399` (`Request::ProviderAdd`)
- Modify: `crates/single-cli/src/main.rs:854-863` (`ProviderCommand::Add`), `:449-454` area (add `parse_model_spec` near `parse_key_val`), `:2193` area (`ProviderCommand::Add` dispatch)
- Modify: `crates/single-runtime/src/handlers.rs:958-973` (`Request::ProviderAdd` handler), `:1810` (test literal)
- Modify: `crates/single-core/src/providers.rs:165, 185, 266, 306, 324, 347` (6 `ProviderSpec` literals)
- Modify: `crates/single-core/src/provider_keys.rs:225, 254, 274` (3 `ProviderSpec` literals)

**Interfaces:**
- Produces: `single_protocol::ModelSpec { pub id: String, pub name: String }`, `ProviderSpec.models: Vec<ModelSpec>`, `Request::ProviderAdd`'s new `models: Vec<ModelSpec>` field.

- [ ] **Step 1: Add `ModelSpec` and the new `ProviderSpec` field**

In `crates/single-protocol/src/lib.rs`, add right before the `ProviderSpec` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
}
```

Replace the `ProviderSpec` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSpec {
    pub name: String,
    pub env_var_name: String,
    pub secret_name: String,
    pub base_url: Option<String>,
    /// Models this provider exposes, declared when the provider is added
    /// via `--model <id>:<name>` (repeatable). Empty for every built-in
    /// preset and for a provider added without `--model`. Only consumed
    /// today by `formats::opencode::apply_provider` — a provider with no
    /// declared models has nothing to add to opencode's picker.
    #[serde(default)]
    pub models: Vec<ModelSpec>,
}
```

(`#[serde(default)]` makes this backward-compatible with any already-serialized `providers.toml` predating this field.)

- [ ] **Step 2: Add `models` to `Request::ProviderAdd`**

Replace:

```rust
    ProviderAdd {
        name: String,
        env_var_name: String,
        base_url: Option<String>,
    },
```

with:

```rust
    ProviderAdd {
        name: String,
        env_var_name: String,
        base_url: Option<String>,
        models: Vec<ModelSpec>,
    },
```

- [ ] **Step 3: `cargo build` to find every literal needing the new field**

Run: `cargo build --workspace 2>&1 | grep -B1 "missing field"`
Expected: errors at each of the 10 `ProviderSpec { ... }` literals plus 1 `Request::ProviderAdd { ... }` literal listed in this task's Files section (11 `ProviderSpec` sites total: `providers.rs` ×6, `provider_keys.rs` ×3, `handlers.rs` ×1 test + 1 real construction covered by Step 4 below).

- [ ] **Step 4: Add `models: Vec::new()` to every test/preset `ProviderSpec` literal**

In `crates/single-core/src/providers.rs` (lines 165, 185, 266, 306, 324, 347) and `crates/single-core/src/provider_keys.rs` (lines 225, 254, 274) and `crates/single-runtime/src/handlers.rs`'s test literal (line 1810 — inside a `#[cfg(test)]` block, distinct from the real handler literal touched in Step 5): add `models: Vec::new(),` as a new line to each `ProviderSpec { ... }` literal. None of these represent a provider being added with declared models, so an empty vec is correct for all of them.

- [ ] **Step 5: Wire the real `--model` flag through to the real handler**

In `crates/single-cli/src/main.rs`, add a parser function near `parse_key_val` (around line 449):

```rust
fn parse_model_spec(s: &str) -> anyhow::Result<(String, String)> {
    let (id, name) = s
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected ID:NAME, got '{s}'"))?;
    Ok((id.to_string(), name.to_string()))
}
```

Replace the `ProviderCommand::Add` variant:

```rust
    Add {
        name: String,
        /// The environment variable name an agent needs to see this key as, e.g. ANTHROPIC_API_KEY.
        #[arg(long)]
        env_var: String,
        #[arg(long)]
        base_url: Option<String>,
        /// A model this provider exposes, as `id:display name` (e.g. `auto:Auto (best available)`). Repeatable.
        #[arg(long = "model", value_parser = parse_model_spec)]
        models: Vec<(String, String)>,
    },
```

Replace its dispatch arm (currently around line 2193):

```rust
            ProviderCommand::Add {
                name,
                env_var,
                base_url,
                models,
            } => {
                let response = client::send(
                    &socket_path,
                    Request::ProviderAdd {
                        name,
                        env_var_name: env_var,
                        base_url,
                        models: models
                            .into_iter()
                            .map(|(id, name)| single_protocol::ModelSpec { id, name })
                            .collect(),
                    },
                )?;
                render::print(response, false);
            }
```

In `crates/single-runtime/src/handlers.rs`, replace the real `Request::ProviderAdd` handler (around line 958-973):

```rust
        Request::ProviderAdd {
            name,
            env_var_name,
            base_url,
            models,
        } => {
            let secret_name = format!("provider:{name}");
            single_core::providers::add(
                &ctx.dirs.providers_registry_file(),
                single_protocol::ProviderSpec {
                    name,
                    env_var_name,
                    secret_name,
                    base_url,
                    models,
                },
            )?;
            Ok(ResponseData::Empty)
        }
```

- [ ] **Step 6: Build and test**

Run: `cargo build --workspace 2>&1 | tail -20` — expect clean.
Run: `cargo test -p single-core -p single-runtime -p single-cli 2>&1 | grep -E "^test result|FAILED"` — expect all `ok`, 0 failed (existing tests unaffected — none of them assert on the absence of a `models` field).

- [ ] **Step 7: Write a test proving `--model` round-trips**

Add to `crates/single-core/src/providers.rs`'s existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn add_and_load_preserves_declared_models() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers.toml");
        add(&path, ProviderSpec {
            name: "omniroute".into(),
            env_var_name: "OMNIROUTE_API_KEY".into(),
            secret_name: "provider:omniroute".into(),
            base_url: Some("http://localhost:20128/v1".into()),
            models: vec![single_protocol::ModelSpec { id: "auto".into(), name: "Auto (best available)".into() }],
        })
        .unwrap();

        let loaded = find(&path, "omniroute").unwrap().unwrap();
        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.models[0].id, "auto");
        assert_eq!(loaded.models[0].name, "Auto (best available)");
    }
```

Run: `cargo test -p single-core add_and_load_preserves_declared_models -- --nocapture` — expect PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/single-protocol/src/lib.rs crates/single-cli/src/main.rs crates/single-runtime/src/handlers.rs crates/single-core/src/providers.rs crates/single-core/src/provider_keys.rs
git commit -m "feat: add ProviderSpec.models and --model flag for declaring a custom provider's models"
```

---

### Task 2: opencode provider config writer

**Files:**
- Modify: `crates/single-agent-sdk/src/formats/opencode.rs`
- Test: same file's `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `single_protocol::{ProviderSpec, ModelSpec}` (Task 1).
- Produces: `pub fn apply_provider(path: &Path, provider: &ProviderSpec) -> Result<serde_json::Value>` — same Value-returning convention as this file's existing `apply`/`apply_lsp` (the caller renders + writes-with-backup, this function does not write to disk itself).

- [ ] **Step 1: Add `apply_provider`**

Add to `crates/single-agent-sdk/src/formats/opencode.rs`, after `apply_lsp` and before `remove_lsp`:

```rust
/// Writes a custom/local provider into `opencode.jsonc`'s `provider.<name>`
/// key, in opencode's own documented custom-OpenAI-compatible-provider
/// format (confirmed via opencode's docs, not guessed):
/// `{"npm": "@ai-sdk/openai-compatible", "name": ..., "options": {"baseURL":
/// ..., "apiKey": "{env:VAR}"}, "models": {...}}`. The `apiKey` value is
/// always the literal env-var-reference string `"{env:VAR}"`, never a raw
/// secret — opencode resolves that reference from its own process
/// environment at runtime, which SingleCLI populates separately via
/// `single_core::provider_keys::resolve_env_for_agent`. This function
/// never sees or needs the actual secret value.
pub fn apply_provider(path: &Path, provider: &single_protocol::ProviderSpec) -> Result<Value> {
    let mut root: Value = if path.exists() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let stripped = strip_jsonc_comments(&text);
        serde_json::from_str(&stripped)
            .with_context(|| format!("parsing {} as JSONC", path.display()))?
    } else {
        Value::Object(Map::new())
    };

    let root_obj = root.as_object_mut().context("opencode.jsonc root is not a JSON object")?;
    let provider_obj = root_obj
        .entry("provider")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("provider is not a JSON object")?;

    let mut models = Map::new();
    for model in &provider.models {
        let mut model_entry = Map::new();
        model_entry.insert("name".into(), Value::String(model.name.clone()));
        models.insert(model.id.clone(), Value::Object(model_entry));
    }

    let mut options = Map::new();
    if let Some(base_url) = &provider.base_url {
        options.insert("baseURL".into(), Value::String(base_url.clone()));
    }
    options.insert("apiKey".into(), Value::String(format!("{{env:{}}}", provider.env_var_name)));

    let mut entry = Map::new();
    entry.insert("npm".into(), Value::String("@ai-sdk/openai-compatible".into()));
    entry.insert("name".into(), Value::String(provider.name.clone()));
    entry.insert("options".into(), Value::Object(options));
    entry.insert("models".into(), Value::Object(models));
    provider_obj.insert(provider.name.clone(), Value::Object(entry));

    Ok(root)
}
```

Add `use single_protocol;` is not needed since the function signature already qualifies it as `single_protocol::ProviderSpec` — but the file's existing `use single_protocol::{LspServerSpec, McpServerSpec};` line needs `ProviderSpec` added to it. Replace:

```rust
use single_protocol::{LspServerSpec, McpServerSpec};
```

with:

```rust
use single_protocol::{LspServerSpec, McpServerSpec, ProviderSpec};
```

and use the bare `ProviderSpec` (not `single_protocol::ProviderSpec`) in `apply_provider`'s signature to match this file's existing import style:

```rust
pub fn apply_provider(path: &Path, provider: &ProviderSpec) -> Result<Value> {
```

- [ ] **Step 2: Write the tests**

Add to this file's `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn apply_provider_writes_the_confirmed_opencode_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        let provider = ProviderSpec {
            name: "omniroute".into(),
            env_var_name: "OMNIROUTE_API_KEY".into(),
            secret_name: "provider:omniroute".into(),
            base_url: Some("http://localhost:20128/v1".into()),
            models: vec![single_protocol::ModelSpec { id: "auto".into(), name: "Auto (best available)".into() }],
        };
        let result = apply_provider(&path, &provider).unwrap();
        assert_eq!(result["provider"]["omniroute"]["npm"], "@ai-sdk/openai-compatible");
        assert_eq!(result["provider"]["omniroute"]["name"], "omniroute");
        assert_eq!(result["provider"]["omniroute"]["options"]["baseURL"], "http://localhost:20128/v1");
        assert_eq!(result["provider"]["omniroute"]["options"]["apiKey"], "{env:OMNIROUTE_API_KEY}");
        assert_eq!(result["provider"]["omniroute"]["models"]["auto"]["name"], "Auto (best available)");
    }

    #[test]
    fn apply_provider_never_writes_a_raw_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        let provider = ProviderSpec {
            name: "testprov".into(),
            env_var_name: "TESTPROV_API_KEY".into(),
            secret_name: "provider:testprov".into(),
            base_url: None,
            models: vec![single_protocol::ModelSpec { id: "m1".into(), name: "Model One".into() }],
        };
        let result = apply_provider(&path, &provider).unwrap();
        let rendered = serde_json::to_string(&result).unwrap();
        assert!(rendered.contains("{env:TESTPROV_API_KEY}"));
        assert!(!rendered.contains("sk-"), "no raw secret-shaped string should ever appear");
    }

    #[test]
    fn apply_provider_preserves_existing_mcp_and_lsp_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        std::fs::write(&path, "{\n  \"mcp\": { \"git\": { \"type\": \"local\" } },\n  \"lsp\": { \"rust\": {} }\n}").unwrap();
        let provider = ProviderSpec {
            name: "omniroute".into(),
            env_var_name: "OMNIROUTE_API_KEY".into(),
            secret_name: "provider:omniroute".into(),
            base_url: Some("http://localhost:20128/v1".into()),
            models: vec![single_protocol::ModelSpec { id: "auto".into(), name: "Auto".into() }],
        };
        let result = apply_provider(&path, &provider).unwrap();
        assert_eq!(result["mcp"]["git"]["type"], "local");
        assert!(result["lsp"]["rust"].is_object());
        assert!(result["provider"]["omniroute"].is_object());
    }

    #[test]
    fn apply_provider_omits_base_url_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencode.jsonc");
        let provider = ProviderSpec {
            name: "testprov".into(),
            env_var_name: "TESTPROV_API_KEY".into(),
            secret_name: "provider:testprov".into(),
            base_url: None,
            models: vec![single_protocol::ModelSpec { id: "m1".into(), name: "Model One".into() }],
        };
        let result = apply_provider(&path, &provider).unwrap();
        assert!(result["provider"]["testprov"]["options"].get("baseURL").is_none());
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p single-agent-sdk apply_provider -- --nocapture`
Expected: all 4 PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/single-agent-sdk/src/formats/opencode.rs
git commit -m "feat: add opencode.jsonc custom-provider writer (apply_provider)"
```

---

### Task 3: Wire `apply_provider` into `provider_sync::sync()`

**Files:**
- Modify: `crates/single-agent-sdk/src/provider_sync.rs`
- Modify: `crates/single-runtime/src/handlers.rs:1027-1066` (`Request::ProviderSync`), `:1111-1147` (`Request::ProviderKeySync`)

**Interfaces:**
- Consumes: `formats::opencode::apply_provider` (Task 2), `ProviderSpec.models` (Task 1).
- Produces: `sync()`'s new signature `pub fn sync(agent: &str, home: &Path, provider: &ProviderSpec, value: &str, dry_run: bool) -> Result<ProviderSyncResult>` (was `env_var_name: &str` in place of `provider: &ProviderSpec` — the one real signature change in this plan).

- [ ] **Step 1: Change `sync()`'s signature and add the opencode arm**

In `crates/single-agent-sdk/src/provider_sync.rs`, replace the `use` line and `sync` function:

```rust
use crate::backup::backup_before_write;
use crate::formats;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use single_protocol::{ProviderSpec, ProviderSyncResult};
use std::path::Path;

pub fn sync(agent: &str, home: &Path, provider: &ProviderSpec, value: &str, dry_run: bool) -> Result<ProviderSyncResult> {
    match agent {
        "claude" => sync_claude(home, &provider.env_var_name, value, dry_run),
        "codex" if provider.env_var_name == "OPENAI_API_KEY" => sync_codex(home, value, dry_run),
        "codex" => Ok(unsupported(
            "codex",
            home,
            &format!("codex's auth.json only has a slot for OPENAI_API_KEY, not '{}'", provider.env_var_name),
        )),
        "opencode" => sync_opencode(home, provider, dry_run),
        other => Ok(unsupported(other, home, "no verified provider-key config location for this agent")),
    }
}

fn sync_opencode(home: &Path, provider: &ProviderSpec, dry_run: bool) -> Result<ProviderSyncResult> {
    if provider.models.is_empty() {
        return Ok(unsupported(
            "opencode",
            home,
            "no models declared for this provider (add it with `--model id:name`); nothing to add to opencode's model picker",
        ));
    }
    let path = home.join(".config").join("opencode").join("opencode.jsonc");
    let updated = formats::opencode::apply_provider(&path, provider)?;
    let rendered = serde_json::to_string_pretty(&updated)?;
    write_result(&provider.name, "opencode", &path, &rendered, dry_run)
}
```

(Everything below `sync_claude`/`sync_codex`/`unsupported`/`write_result` stays unchanged — only the `use` line, `sync`'s signature/body, and the new `sync_opencode` function are added/changed. `crates/single-agent-sdk/src/lib.rs:6` already has `pub mod formats;`, so `formats::opencode::apply_provider` resolves directly, no extra import needed beyond adding `formats` to this file's own `use crate::` line as shown above.)

- [ ] **Step 2: Update both call sites in `handlers.rs`**

In the `Request::ProviderSync` arm (around line 1056), replace:

```rust
                let mut result = single_agent_sdk::provider_sync::sync(
                    &agent,
                    &home,
                    &provider.env_var_name,
                    &value,
                    dry_run,
                )?;
```

with:

```rust
                let mut result = single_agent_sdk::provider_sync::sync(
                    &agent,
                    &home,
                    &provider,
                    &value,
                    dry_run,
                )?;
```

In the `Request::ProviderKeySync` arm (around line 1139), replace:

```rust
            let mut result = single_agent_sdk::provider_sync::sync(
                &agent,
                &home,
                &provider_spec.env_var_name,
                &value,
                dry_run,
            )?;
```

with:

```rust
            let mut result = single_agent_sdk::provider_sync::sync(
                &agent,
                &home,
                &provider_spec,
                &value,
                dry_run,
            )?;
```

- [ ] **Step 3: Update `provider_sync.rs`'s existing tests for the new signature**

The four existing tests in this file (`claude_sync_preserves_other_settings`, `codex_sync_only_accepts_openai_api_key`, `unverified_agents_are_refused_not_faked`, `dry_run_never_writes`) all call `sync("claude"|"codex"|"opencode", dir.path(), "ANTHROPIC_API_KEY"|..., "value", ...)` with a bare `&str` env var name. Update each call to construct a minimal `ProviderSpec` instead:

```rust
    #[test]
    fn claude_sync_preserves_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(".claude/settings.json"), r#"{"theme":"dark","env":{"OTHER_VAR":"x"}}"#).unwrap();

        let provider = ProviderSpec { name: "anthropic".into(), env_var_name: "ANTHROPIC_API_KEY".into(), secret_name: "provider:anthropic".into(), base_url: None, models: vec![] };
        let result = sync("claude", dir.path(), &provider, "sk-test-123", false).unwrap();
        assert!(result.applied);

        let written: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap()).unwrap();
        assert_eq!(written["theme"], "dark");
        assert_eq!(written["env"]["OTHER_VAR"], "x");
        assert_eq!(written["env"]["ANTHROPIC_API_KEY"], "sk-test-123");
    }

    #[test]
    fn codex_sync_only_accepts_openai_api_key() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
        std::fs::write(dir.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{}}"#).unwrap();

        let openai_provider = ProviderSpec { name: "openai".into(), env_var_name: "OPENAI_API_KEY".into(), secret_name: "provider:openai".into(), base_url: None, models: vec![] };
        let ok = sync("codex", dir.path(), &openai_provider, "sk-openai-1", false).unwrap();
        assert!(ok.applied);
        let written: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(written["auth_mode"], "apikey");
        assert_eq!(written["OPENAI_API_KEY"], "sk-openai-1");

        let other_provider = ProviderSpec { name: "other".into(), env_var_name: "SOME_OTHER_KEY".into(), secret_name: "provider:other".into(), base_url: None, models: vec![] };
        let rejected = sync("codex", dir.path(), &other_provider, "value", false).unwrap();
        assert!(!rejected.applied);
    }

    #[test]
    fn unverified_agents_are_refused_not_faked() {
        let dir = tempfile::tempdir().unwrap();
        let provider = ProviderSpec { name: "test".into(), env_var_name: "OPENAI_API_KEY".into(), secret_name: "provider:test".into(), base_url: None, models: vec![] };
        let result = sync("perplexity", dir.path(), &provider, "x", false).unwrap();
        assert!(!result.applied);
    }

    #[test]
    fn dry_run_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let provider = ProviderSpec { name: "anthropic".into(), env_var_name: "ANTHROPIC_API_KEY".into(), secret_name: "provider:anthropic".into(), base_url: None, models: vec![] };
        let result = sync("claude", dir.path(), &provider, "sk-test", true).unwrap();
        assert!(!result.applied);
        assert!(!dir.path().join(".claude/settings.json").exists());
    }
```

Note: `unverified_agents_are_refused_not_faked` now uses `"perplexity"` instead of `"opencode"` as its example, since `"opencode"` is no longer universally unsupported — it's supported when `models` is non-empty. This test's intent (a genuinely unsupported agent stays refused) is unchanged.

- [ ] **Step 4: Add new tests for the opencode arm**

Add to the same test module:

```rust
    #[test]
    fn opencode_sync_is_unsupported_without_declared_models() {
        let dir = tempfile::tempdir().unwrap();
        let provider = ProviderSpec { name: "omniroute".into(), env_var_name: "OMNIROUTE_API_KEY".into(), secret_name: "provider:omniroute".into(), base_url: Some("http://localhost:20128/v1".into()), models: vec![] };
        let result = sync("opencode", dir.path(), &provider, "placeholder", false).unwrap();
        assert!(!result.applied);
        assert!(result.detail.contains("no models declared"));
    }

    #[test]
    fn opencode_sync_writes_provider_config_when_models_are_declared() {
        let dir = tempfile::tempdir().unwrap();
        let provider = ProviderSpec {
            name: "omniroute".into(),
            env_var_name: "OMNIROUTE_API_KEY".into(),
            secret_name: "provider:omniroute".into(),
            base_url: Some("http://localhost:20128/v1".into()),
            models: vec![single_protocol::ModelSpec { id: "auto".into(), name: "Auto (best available)".into() }],
        };
        let result = sync("opencode", dir.path(), &provider, "placeholder", false).unwrap();
        assert!(result.applied);
        let path = dir.path().join(".config/opencode/opencode.jsonc");
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["provider"]["omniroute"]["options"]["apiKey"], "{env:OMNIROUTE_API_KEY}");
    }

    #[test]
    fn opencode_sync_backs_up_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".config/opencode")).unwrap();
        std::fs::write(dir.path().join(".config/opencode/opencode.jsonc"), "{\"mcp\": {}}").unwrap();
        let provider = ProviderSpec {
            name: "omniroute".into(),
            env_var_name: "OMNIROUTE_API_KEY".into(),
            secret_name: "provider:omniroute".into(),
            base_url: Some("http://localhost:20128/v1".into()),
            models: vec![single_protocol::ModelSpec { id: "auto".into(), name: "Auto".into() }],
        };
        let result = sync("opencode", dir.path(), &provider, "placeholder", false).unwrap();
        assert!(result.backup_path.is_some());
    }
```

- [ ] **Step 5: Build and test**

Run: `cargo build --workspace 2>&1 | tail -20` — expect clean.
Run: `cargo test -p single-agent-sdk -p single-runtime 2>&1 | grep -E "^test result|FAILED"` — expect all `ok`, 0 failed.

- [ ] **Step 6: Commit**

```bash
git add crates/single-agent-sdk/src/provider_sync.rs crates/single-runtime/src/handlers.rs
git commit -m "feat: wire opencode custom-provider writer into provider_sync::sync"
```

---

### Task 4: Deploy OmniRoute on this host

**Files:** none in the SingleCLI repo — this task deploys real infrastructure on the local Debian machine, outside any git repo.

**Interfaces:** none — this task's only deliverable is a running, boot-persistent OmniRoute instance at `http://localhost:20128`.

- [ ] **Step 1: Install OmniRoute**

Run: `npm install -g omniroute`
Expected: installs cleanly under the nvm-managed prefix (`/home/navinbruas/.nvm/versions/node/v24.19.0`). Confirm with: `which omniroute` — expect `/home/navinbruas/.nvm/versions/node/v24.19.0/bin/omniroute`.

- [ ] **Step 2: Run the first-time setup wizard**

Run: `omniroute setup`
This is interactive — follow its prompts for a zero-config start (the `auto` model works immediately with no API keys, per OmniRoute's own docs). Do not register specific provider API keys inside OmniRoute at this stage — the `auto` model's pooled free-tier access is the baseline this plan targets; registering higher-quality provider keys inside OmniRoute itself is a user choice for later, not part of this plan.

- [ ] **Step 3: Verify it runs correctly in the foreground first**

Run: `omniroute` (foreground, no args — this is the bare command that starts the gateway+dashboard on port 20128 per its own docs)
In a separate terminal, verify: `curl http://localhost:20128/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"auto","messages":[{"role":"user","content":"Hello!"}]}'`
Expected: a real JSON chat-completion response, not an error. Stop the foreground process (Ctrl-C) once confirmed.

- [ ] **Step 4: Create the systemd user service**

Run: `mkdir -p ~/.config/systemd/user`

Write `~/.config/systemd/user/omniroute.service`:

```ini
[Unit]
Description=OmniRoute AI gateway
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/home/navinbruas/.nvm/versions/node/v24.19.0/bin/omniroute
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

(Absolute path in `ExecStart` because systemd user services don't source nvm's shell-profile hooks — a bare `omniroute` would fail to resolve.)

- [ ] **Step 5: Enable and start the service**

```bash
systemctl --user daemon-reload
systemctl --user enable --now omniroute.service
loginctl enable-linger "$USER"
```

(`loginctl enable-linger` makes the user service survive after the user logs out / across reboots without an active session — needed since this is meant to run persistently, not just while a terminal is open.)

- [ ] **Step 6: Verify the service is actually running and healthy**

```bash
systemctl --user status omniroute.service
curl http://localhost:20128/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"auto","messages":[{"role":"user","content":"Hello via systemd"}]}'
```

Expected: `systemctl --user status` shows `active (running)`; the `curl` returns a real response, proving the systemd-managed instance (not the earlier foreground one) is what's actually serving requests.

- [ ] **Step 7: Verify boot persistence is configured (without actually rebooting)**

Run: `systemctl --user is-enabled omniroute.service` — expect `enabled`.
Run: `loginctl show-user "$USER" | grep Linger` — expect `Linger=yes`.

This task has no code commit — it's a system deployment step. Note its completion in whatever mechanism is tracking this plan's progress (ledger, if using subagent-driven-development).

---

### Task 5: Register OmniRoute as a SingleCLI provider for opencode

**Files:** none — this task only runs `single` CLI commands, using the capability built in Tasks 1-3, against the real OmniRoute instance deployed in Task 4.

**Interfaces:**
- Consumes: Task 1's `--model` flag, Task 3's opencode provider-sync arm, Task 4's running OmniRoute instance.

- [ ] **Step 1: Register the provider**

```bash
single provider add omniroute \
  --env-var OMNIROUTE_API_KEY \
  --base-url http://localhost:20128/v1 \
  --model auto:"Auto (best available)"
```

Expected: succeeds silently (matches `ProviderAdd`'s `ResponseData::Empty` pattern). Verify with: `single provider inspect omniroute` — expect to see `base_url: http://localhost:20128/v1` and one declared model.

- [ ] **Step 2: Set a placeholder key**

```bash
single provider set-key omniroute placeholder
```

(OmniRoute ignores the API key value by default per its own docs — a placeholder is sufficient; SingleCLI's own machinery still requires *some* value to be present before `provider sync`/`provider add-key` will proceed, per `ProviderSetKey`'s existing behavior.)

- [ ] **Step 3: Bind a labeled key for opencode specifically**

```bash
single provider add-key omniroute --agent opencode placeholder
```

This is what makes `resolve_env_for_agent` inject `OMNIROUTE_API_KEY=placeholder` into opencode's spawned process environment at task-run time (existing mechanism, confirmed in the spec's investigation — no code from this plan touches this path).

- [ ] **Step 4: Sync the provider into opencode's config**

```bash
single provider sync omniroute --agents opencode --yes
```

Expected output: a `ProviderSyncResults` entry for `opencode` with `applied: true` (this is the real, live exercise of Task 3's `sync_opencode` function against the real isolated `opencode` home on this machine).

- [ ] **Step 5: Verify the real file was actually written**

```bash
cat ~/.config/single/homes/opencode/.config/opencode/opencode.jsonc | grep -A6 '"omniroute"'
```

Expected: a `provider.omniroute` block matching the confirmed format — `"npm": "@ai-sdk/openai-compatible"`, `"options": {"baseURL": "http://localhost:20128/v1", "apiKey": "{env:OMNIROUTE_API_KEY}"}`, `"models": {"auto": {"name": "Auto (best available)"}}`.

(Note: this writes into opencode's SingleCLI-managed *isolated* home, per `agent_home.rs`'s bootstrapping — not `opencode`'s real ambient `~/.config/opencode/opencode.jsonc`, unless `provider sync` was run with `--real-home`, which this step deliberately does not use, matching the project's default isolated-home posture.)

- [ ] **Step 6: End-to-end confirmation via a real task dispatch**

```bash
single task run "say hello" --agent opencode --real-home
```

(`--real-home` here is deliberate and different from Step 5's isolated-home sync: per `agent_home.rs`'s `HomeRequirement::RealRequired` — set for `opencode` back in the orchestration-lessons plan — opencode needs `real_home:true` to actually authenticate/run. Since Step 4-5 synced into the *isolated* home, first re-run Step 4's sync with `--real-home` too: `single provider sync omniroute --agents opencode --yes --real-home`, THEN run this task-dispatch check.)

Expected: the task completes successfully, proving the full chain — OmniRoute running, provider registered, opencode config written, opencode able to actually pick and use the `omniroute/auto` model for a real dispatch.

This task has no code commit — it's a live configuration/verification step. Note its completion in the plan's progress tracking.

---

### Task 6: Version bump and CHANGELOG

**Files:**
- Modify: `Cargo.toml` (workspace version)
- Modify: `CHANGELOG.md`

**Interfaces:** None.

- [ ] **Step 1: Bump the workspace version**

In `Cargo.toml`'s `[workspace.package]`, change `version = "0.7.0"` to `version = "0.8.0"`.

- [ ] **Step 2: Update CHANGELOG.md**

Read the current top of `CHANGELOG.md` first (`head -20 CHANGELOG.md`) — it should show an empty `## [Unreleased]` heading (left that way by the orchestration-lessons plan's own Task 8) followed by `## [0.7.0]`. Fill the `## [Unreleased]` heading with:

```markdown
- Added: `single provider add` gains a repeatable `--model <id>:<name>`
  flag, and `single provider sync` can now write a custom/local provider
  into opencode's own `opencode.jsonc` `provider` key (via
  `@ai-sdk/openai-compatible`) — previously, syncing a provider only
  injected its API key as an env var into an agent's process, which got
  the credential to opencode but never told opencode's model picker the
  provider existed. `provider_sync::sync`'s API-key value is never
  written into opencode's config — only an `{env:VAR}` reference, matching
  how the actual secret already reaches opencode's process.
```

Then rename that heading to `## [0.8.0]` and add a fresh empty `## [Unreleased]` above it — same Keep a Changelog release pattern as the orchestration-lessons plan's own version-bump task.

- [ ] **Step 3: Full workspace verification**

```bash
cargo build --workspace 2>&1 | tail -30
cargo test --workspace 2>&1 | tail -60
cargo clippy --workspace 2>&1 | tail -60
```

Expected: clean build, all tests pass, no new clippy warnings beyond what already existed on `main` before this plan (the orchestration-lessons plan left one known/accepted `clippy::type_complexity` warning on `AGENT_SLOTS` — don't re-flag that one).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump workspace version to 0.8.0 for custom-provider support"
```

- [ ] **Step 5: Push**

```bash
git push origin main
```

(Or to the feature branch if this plan is executed in an isolated worktree — push to `main` only after the branch is merged, matching the orchestration-lessons plan's own convention.)
