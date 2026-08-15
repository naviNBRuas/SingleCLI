//! `single install-integrations` / `single uninstall-integrations`: syncs
//! SingleCLI's unified MCP/LSP registries into every enabled agent's
//! **SingleCLI-managed home** (`single_core::agent_home` — bootstrapped
//! from the real `$HOME` once, never written into again after that; see
//! that module's doc comment for why).

use crate::context::Context;
use anyhow::Result;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::IntegrationResult;

pub fn install_all(ctx: &Context, dry_run: bool) -> Result<IntegrationResult> {
    let real_home = home_dir()?;
    // Gateway mode: sync only the single-mcp dynamic gateway (it proxies
    // to every enabled server itself) instead of the full registry list —
    // see single_core::mcp::gateway_mode's doc comment.
    let mcp_servers = if single_core::mcp::gateway_mode(&ctx.dirs.mcp_gateway_file())? {
        vec![single_core::mcp::gateway_server_spec()]
    } else {
        single_core::mcp::load(&ctx.dirs.mcp_registry_file())?
    };
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent.name)?;
        writes.push(adapter.configure_mcp(&home, &mcp_servers, dry_run)?);
        writes.push(adapter.configure_lsp(&home, &lsp_servers, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn uninstall_all(ctx: &Context, dry_run: bool) -> Result<IntegrationResult> {
    let real_home = home_dir()?;
    let mcp_servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    let mcp_names: Vec<String> = mcp_servers.iter().map(|s| s.name.clone()).collect();
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;
    let lsp_names: Vec<String> = lsp_servers.iter().map(|s| s.name.clone()).collect();

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent.name)?;
        writes.push(adapter.remove_mcp(&home, &mcp_names, dry_run)?);
        writes.push(adapter.remove_lsp(&home, &lsp_names, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn home_dir() -> Result<std::path::PathBuf> {
    single_core::paths::real_home_dir()
}
