use crate::context::Context;
use crate::{bootstrap, doctor, integrations};
use single_agent_sdk::adapters::for_agent;
use single_core::registry::AgentDefinition;
use single_protocol::{
    AgentInfo, McpServerInfo, Request, Response, ResponseData, RuntimeStatus,
};

pub fn handle(ctx: &Context, request: Request) -> Response {
    match dispatch(ctx, request) {
        Ok(data) => Response::Ok { data },
        Err(e) => Response::Error { message: format!("{e:#}") },
    }
}

fn dispatch(ctx: &Context, request: Request) -> anyhow::Result<ResponseData> {
    match request {
        Request::Status => Ok(ResponseData::Status(status(ctx))),
        Request::Doctor => Ok(ResponseData::Doctor(doctor::run(ctx))),
        Request::AgentList => {
            let agents = ctx.registry.iter().map(to_agent_info).collect();
            Ok(ResponseData::Agents(agents))
        }
        Request::AgentInspect { name } => {
            let def = ctx
                .find_agent(&name)
                .ok_or_else(|| anyhow::anyhow!("no such agent: {name}"))?;
            Ok(ResponseData::Agent(to_agent_info(def)))
        }
        Request::McpList => {
            let servers = single_core::mcp::load(&ctx.dirs.root().join("mcp.toml"))?;
            let enabled_agents: Vec<String> = ctx.registry.iter().map(|a| a.name.clone()).collect();
            let infos = servers
                .into_iter()
                .map(|s| McpServerInfo {
                    name: s.name,
                    command: format!("{} {}", s.command, s.args.join(" ")).trim().to_string(),
                    enabled: s.enabled,
                    synced_to: enabled_agents.clone(),
                })
                .collect();
            Ok(ResponseData::McpServers(infos))
        }
        Request::Setup { dry_run } => Ok(ResponseData::SetupPlan(bootstrap::run(ctx, dry_run))),
        Request::InstallIntegrations { dry_run } => {
            Ok(ResponseData::IntegrationResult(integrations::install_all(ctx, dry_run)?))
        }
        Request::UninstallIntegrations => {
            Ok(ResponseData::IntegrationResult(integrations::uninstall_all(ctx, false)?))
        }
        Request::ProfileList => {
            Ok(ResponseData::Profiles(single_core::profile::list_profiles(&ctx.dirs)?))
        }
        Request::ProfileUse { name } => {
            single_core::profile::use_profile(&ctx.dirs, &name)?;
            Ok(ResponseData::Empty)
        }
    }
}

fn status(ctx: &Context) -> RuntimeStatus {
    let detected = ctx
        .registry
        .iter()
        .filter(|a| for_agent(&a.name).map(|ad| ad.discover().detected).unwrap_or(false))
        .count();
    RuntimeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        active_profile: ctx.resolved.active_profile.clone(),
        agents_known: ctx.registry.len(),
        agents_detected: detected,
        socket_path: ctx.dirs.socket_path().display().to_string(),
        db_path: ctx.dirs.db_path().display().to_string(),
    }
}

fn to_agent_info(def: &AgentDefinition) -> AgentInfo {
    let discovery = for_agent(&def.name).map(|a| a.discover());
    AgentInfo {
        name: def.name.clone(),
        adapter: def.adapter.clone(),
        command: def.command.clone(),
        detected: discovery.as_ref().map(|d| d.detected).unwrap_or(false),
        version: discovery.and_then(|d| d.version),
        install_method: def.install_method.clone(),
        bootstrap_install: def.bootstrap_install.clone(),
        unverified: def.unverified,
        capabilities: def.capabilities,
        config_paths: def.config_paths.clone(),
        notes: def.notes.clone(),
    }
}
