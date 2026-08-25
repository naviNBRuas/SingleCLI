//! `single install-integrations` / `single uninstall-integrations`: syncs
//! SingleCLI's unified MCP/LSP registries into every enabled agent's
//! **SingleCLI-managed home** (`single_core::agent_home` — bootstrapped
//! from the real `$HOME` once, never written into again after that; see
//! that module's doc comment for why).

use crate::context::Context;
use anyhow::Result;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::IntegrationResult;

pub fn install_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult> {
    let home_root = home_dir()?;
    let registry_servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    let gateway_spec = single_core::mcp::gateway_server_spec();
    // Gateway mode: sync only the single-mcp dynamic gateway (it proxies
    // to every enabled server itself) instead of the full registry list —
    // see single_core::mcp::gateway_mode's doc comment. Whichever mode is
    // *not* active this run, its names are stale leftovers from a
    // previous sync under the other mode: `configure_mcp` only merges the
    // names it's given, it never prunes names it wasn't told about, so
    // toggling gateway mode and re-syncing would otherwise leave both the
    // gateway entry *and* every individual server registered side by
    // side — defeating gateway mode's whole point (avoid registering N
    // servers individually). Removing the other mode's names first keeps
    // each sync a clean switch, not an accumulation.
    let (mcp_servers, stale_names): (Vec<_>, Vec<String>) = if single_core::mcp::gateway_mode(&ctx.dirs.mcp_gateway_file())? {
        (vec![gateway_spec], registry_servers.iter().map(|s| s.name.clone()).collect())
    } else {
        (registry_servers, vec![gateway_spec.name])
    };
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = if real_home {
            home_root.clone()
        } else {
            single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &home_root, &agent.name)?
        };
        if !stale_names.is_empty() {
            writes.push(adapter.remove_mcp(&home, &stale_names, dry_run)?);
        }
        writes.push(adapter.configure_mcp(&home, &mcp_servers, dry_run)?);
        writes.push(adapter.configure_lsp(&home, &lsp_servers, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn uninstall_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult> {
    let home_root = home_dir()?;
    let mcp_servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    // Also remove the gateway entry name regardless of which mode is
    // currently active — an uninstall should clean up either shape
    // (direct per-server entries or the single gateway entry) a prior
    // sync may have left behind, not just whichever one gateway_mode()
    // says is "current" right now.
    let mcp_names: Vec<String> =
        mcp_servers.iter().map(|s| s.name.clone()).chain(std::iter::once(single_core::mcp::gateway_server_spec().name)).collect();
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;
    let lsp_names: Vec<String> = lsp_servers.iter().map(|s| s.name.clone()).collect();

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = if real_home {
            home_root.clone()
        } else {
            single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &home_root, &agent.name)?
        };
        writes.push(adapter.remove_mcp(&home, &mcp_names, dry_run)?);
        writes.push(adapter.remove_lsp(&home, &lsp_names, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn home_dir() -> Result<std::path::PathBuf> {
    single_core::paths::real_home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn test_ctx(dir: &std::path::Path) -> Context {
        let dirs = single_core::SingleDirs::from_root(dir.to_path_buf());
        dirs.ensure_created().unwrap();
        Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() }
    }

    fn claude_mcp_server_names(dir: &std::path::Path) -> Vec<String> {
        let path = dir.join("homes").join("claude").join(".claude.json");
        let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut names: Vec<String> = root["mcpServers"].as_object().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    #[test]
    fn switching_gateway_mode_replaces_rather_than_accumulates_mcp_entries() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());

        install_all(&ctx, false, false).unwrap();
        let direct_names = claude_mcp_server_names(dir.path());
        assert!(direct_names.contains(&"fetch".to_string()));
        assert!(!direct_names.contains(&"single-mcp".to_string()));

        single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), true).unwrap();
        install_all(&ctx, false, false).unwrap();
        let gateway_names = claude_mcp_server_names(dir.path());
        assert_eq!(gateway_names, vec!["single-mcp".to_string()], "enabling gateway mode must remove the old direct entries, not add to them");

        single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), false).unwrap();
        install_all(&ctx, false, false).unwrap();
        let back_to_direct = claude_mcp_server_names(dir.path());
        assert_eq!(back_to_direct, direct_names, "disabling gateway mode must remove the single-mcp entry, not leave it alongside the direct ones");
    }

    #[test]
    fn uninstall_removes_the_gateway_entry_even_when_gateway_mode_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());

        single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), true).unwrap();
        install_all(&ctx, false, false).unwrap();
        assert_eq!(claude_mcp_server_names(dir.path()), vec!["single-mcp".to_string()]);

        single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), false).unwrap();
        uninstall_all(&ctx, false, false).unwrap();
        assert!(claude_mcp_server_names(dir.path()).is_empty(), "uninstall must clean up a gateway entry left over from a prior gateway-mode sync too");
    }

    #[test]
    fn real_home_writes_the_actual_home_not_the_isolated_copy() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let real_home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", real_home.path());

        install_all(&ctx, false, true).unwrap();

        let real_claude_json = real_home.path().join(".claude.json");
        assert!(real_claude_json.exists(), "--real-home must write into the real $HOME, not the isolated homes/ dir");
        let isolated_claude_json = dir.path().join("homes").join("claude").join(".claude.json");
        assert!(!isolated_claude_json.exists(), "--real-home must not also bootstrap/write the isolated home");

        std::env::remove_var("HOME");
    }

    #[test]
    fn without_real_home_still_writes_the_isolated_copy_only() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        install_all(&ctx, false, false).unwrap();
        assert!(dir.path().join("homes").join("claude").join(".claude.json").exists());
    }
}
