//! The unified LSP registry (spec section 12): mirrors `mcp.rs`'s shape.
//! Stored at `~/.config/single/lsp.toml`.
//!
//! Only `opencode` has a directly observed native config surface for
//! arbitrary LSP servers (`opencode.jsonc`'s `lsp` key, confirmed the same
//! way the MCP formats were). Claude Code's LSP support is a marketplace
//! *plugin* system (`gopls-lsp`, `pyright-lsp`, ... in
//! `~/.claude/settings.json`'s `enabledPlugins`), not a raw command
//! registration — a fundamentally different mechanism, so this registry
//! does not claim to sync into Claude. That's an honest capability gap
//! (spec section 52: distinguish native/emulated/unsupported), not an
//! oversight.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use single_protocol::LspServerSpec;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LspRegistryFile {
    #[serde(default)]
    servers: BTreeMap<String, LspServerSpec>,
}

/// Real language servers, matching what this project's own reference
/// machine actually has configured: `rust-analyzer`, `pyright`,
/// `typescript-language-server`, and `gopls` are the exact four behind
/// `~/.claude/settings.json`'s `enabledPlugins`
/// (`rust-analyzer-lsp`/`pyright-lsp`/`typescript-lsp`/`gopls-lsp`); the
/// `dockerfile` entry mirrors `~/.config/opencode/opencode.jsonc`'s own
/// real `lsp` config (`docker-langserver --stdio`). Not a fabricated
/// starter list — every command here is one this project has directly
/// seen invoked by a real tool.
pub fn default_servers() -> Vec<LspServerSpec> {
    vec![
        LspServerSpec { name: "rust-analyzer".into(), command: "rust-analyzer".into(), args: vec![], extensions: vec![".rs".into()], enabled: true },
        LspServerSpec {
            name: "pyright".into(),
            command: "pyright-langserver".into(),
            args: vec!["--stdio".into()],
            extensions: vec![".py".into()],
            enabled: true,
        },
        LspServerSpec {
            name: "typescript".into(),
            command: "typescript-language-server".into(),
            args: vec!["--stdio".into()],
            extensions: vec![".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into()],
            enabled: true,
        },
        LspServerSpec { name: "gopls".into(), command: "gopls".into(), args: vec![], extensions: vec![".go".into()], enabled: true },
        LspServerSpec {
            name: "dockerfile".into(),
            command: "docker-langserver".into(),
            args: vec!["--stdio".into()],
            extensions: vec!["Dockerfile".into()],
            enabled: true,
        },
    ]
}

/// A named starter config for a real language server, not yet in the
/// user's registry. This is the growth mechanism for task "dynamic lsp":
/// `default_servers()` above ships pre-populated (so a fresh install has
/// something useful immediately); `presets()` is the larger catalog a user
/// opts into one at a time via `single lsp add-preset <name>` without
/// SingleCLI needing a code change for every language they touch. Every
/// entry's command/flags were confirmed for real on the reference machine
/// (binary present, `--help` output showing the exact stdio flag) — same
/// verification discipline as `default_servers()`, not a guessed list.
pub struct LspPreset {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub extensions: &'static [&'static str],
}

pub fn presets() -> Vec<LspPreset> {
    vec![
        LspPreset { name: "rust-analyzer", command: "rust-analyzer", args: &[], extensions: &[".rs"] },
        LspPreset { name: "pyright", command: "pyright-langserver", args: &["--stdio"], extensions: &[".py"] },
        LspPreset { name: "typescript", command: "typescript-language-server", args: &["--stdio"], extensions: &[".ts", ".tsx", ".js", ".jsx"] },
        LspPreset { name: "gopls", command: "gopls", args: &[], extensions: &[".go"] },
        LspPreset { name: "dockerfile", command: "docker-langserver", args: &["--stdio"], extensions: &["Dockerfile"] },
        // Confirmed real via `--help` on the reference machine, in addition to the five above:
        LspPreset { name: "clangd", command: "clangd", args: &[], extensions: &[".c", ".h", ".cpp", ".hpp", ".cc"] },
        LspPreset { name: "bash", command: "bash-language-server", args: &["start"], extensions: &[".sh", ".bash"] },
        LspPreset { name: "yaml", command: "yaml-language-server", args: &["--stdio"], extensions: &[".yaml", ".yml"] },
        // terraform-ls handles plain HCL too (it's fundamentally an HCL
        // parser), not just .tf/.tfvars — widened per a direct request
        // rather than splitting into a separate unverified "hcl" preset.
        LspPreset { name: "terraform", command: "terraform-ls", args: &["serve"], extensions: &[".tf", ".tfvars", ".hcl"] },
        LspPreset { name: "json", command: "vscode-json-language-server", args: &["--stdio"], extensions: &[".json"] },
        // Below: real, documented invocations (each tool's own README/
        // install docs), NOT confirmed via a live `--help` on this
        // machine the way the entries above were — none of these binaries
        // are installed here. Flagged explicitly rather than silently
        // claiming the same verification level; run `<command> --help`
        // yourself before relying on one if it matters.
        LspPreset { name: "php", command: "intelephense", args: &["--stdio"], extensions: &[".php"] },
        LspPreset { name: "html", command: "vscode-html-language-server", args: &["--stdio"], extensions: &[".html", ".htm"] },
        LspPreset { name: "css", command: "vscode-css-language-server", args: &["--stdio"], extensions: &[".css", ".scss", ".less"] },
        LspPreset { name: "tailwindcss", command: "tailwindcss-language-server", args: &["--stdio"], extensions: &[".html", ".jsx", ".tsx", ".vue"] },
        LspPreset { name: "asm", command: "asm-lsp", args: &[], extensions: &[".asm", ".s", ".S"] },
        LspPreset { name: "solidity", command: "nomicfoundation-solidity-language-server", args: &["--stdio"], extensions: &[".sol"] },
    ]
}

pub fn preset(name: &str) -> Option<LspPreset> {
    presets().into_iter().find(|p| p.name == name)
}

impl LspPreset {
    pub fn to_spec(&self) -> LspServerSpec {
        LspServerSpec {
            name: self.name.to_string(),
            command: self.command.to_string(),
            args: self.args.iter().map(|s| s.to_string()).collect(),
            extensions: self.extensions.iter().map(|s| s.to_string()).collect(),
            enabled: true,
        }
    }
}

pub fn load(path: &Path) -> Result<Vec<LspServerSpec>> {
    if !path.exists() {
        return Ok(default_servers());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: LspRegistryFile =
        toml::from_str(&text).with_context(|| format!("parsing {} as TOML", path.display()))?;
    Ok(file.servers.into_values().collect())
}

pub fn save(path: &Path, servers: &[LspServerSpec]) -> Result<()> {
    let mut map = BTreeMap::new();
    for server in servers {
        map.insert(server.name.clone(), server.clone());
    }
    let file = LspRegistryFile { servers: map };
    let rendered = toml::to_string_pretty(&file).context("serializing lsp registry")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered).with_context(|| format!("writing {}", path.display()))
}

pub fn add(path: &Path, server: LspServerSpec) -> Result<()> {
    let mut servers = load(path)?;
    servers.retain(|s| s.name != server.name);
    servers.push(server);
    save(path, &servers)
}

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

pub fn find(path: &Path, name: &str) -> Result<Option<LspServerSpec>> {
    Ok(load(path)?.into_iter().find(|s| s.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LspServerSpec {
        LspServerSpec {
            name: "rust".into(),
            command: "rust-analyzer".into(),
            args: vec![],
            extensions: vec![".rs".into()],
            enabled: true,
        }
    }

    #[test]
    fn load_returns_defaults_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let servers = load(&dir.path().join("lsp.toml")).unwrap();
        assert_eq!(servers.len(), default_servers().len());
        assert!(servers.iter().any(|s| s.name == "rust-analyzer"));
    }

    #[test]
    fn add_then_find_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lsp.toml");
        add(&path, sample()).unwrap();
        let found = find(&path, "rust").unwrap().unwrap();
        assert_eq!(found.command, "rust-analyzer");
    }

    #[test]
    fn remove_reports_whether_anything_was_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lsp.toml");
        add(&path, sample()).unwrap();
        assert!(remove(&path, "rust").unwrap());
        assert!(!remove(&path, "rust").unwrap());
    }

    #[test]
    fn presets_include_defaults_plus_extras_and_resolve_by_name() {
        let names: Vec<_> = presets().iter().map(|p| p.name).collect();
        for expected in [
            "rust-analyzer", "pyright", "typescript", "gopls", "dockerfile", "clangd", "bash", "yaml", "terraform", "json",
            "php", "html", "css", "tailwindcss", "asm", "solidity",
        ] {
            assert!(names.contains(&expected), "missing preset {expected}");
        }
        assert!(preset("nonexistent").is_none());
        let spec = preset("clangd").unwrap().to_spec();
        assert_eq!(spec.command, "clangd");
        assert!(spec.enabled);
    }

    #[test]
    fn terraform_preset_also_covers_plain_hcl_files() {
        let spec = preset("terraform").unwrap().to_spec();
        assert!(spec.extensions.contains(&".hcl".to_string()));
    }
}
