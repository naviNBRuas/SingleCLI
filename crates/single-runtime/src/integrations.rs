//! `single install-integrations` / `single uninstall-integrations`: syncs
//! SingleCLI's unified MCP registry into every enabled agent's real config
//! file (spec section 6), and the inverse.

use crate::context::Context;
use anyhow::Result;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::IntegrationResult;

pub fn install_all(ctx: &Context, dry_run: bool) -> Result<IntegrationResult> {
    let home = home_dir()?;
    let servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir()) else { continue };
        writes.push(adapter.configure_mcp(&home, &servers, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn uninstall_all(ctx: &Context, dry_run: bool) -> Result<IntegrationResult> {
    let home = home_dir()?;
    let servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    let names: Vec<String> = servers.iter().map(|s| s.name.clone()).collect();

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir()) else { continue };
        writes.push(adapter.remove_mcp(&home, &names, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn home_dir() -> Result<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("SINGLE_HOME_DIR") {
        return Ok(std::path::PathBuf::from(dir));
    }
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))
}
