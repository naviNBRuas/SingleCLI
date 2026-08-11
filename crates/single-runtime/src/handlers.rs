use crate::context::Context;
use crate::{bootstrap, doctor, integrations, memory};
use single_agent_sdk::adapters::for_agent_with_custom;
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
            let agents = ctx.registry.iter().map(|def| to_agent_info(def, ctx)).collect();
            Ok(ResponseData::Agents(agents))
        }
        Request::AgentInspect { name } => {
            let def = ctx
                .find_agent(&name)
                .ok_or_else(|| anyhow::anyhow!("no such agent: {name}"))?;
            Ok(ResponseData::Agent(to_agent_info(def, ctx)))
        }
        Request::McpList => {
            let servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
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
        Request::McpAdd { server } => {
            single_core::mcp::add(&ctx.dirs.mcp_registry_file(), server)?;
            Ok(ResponseData::Empty)
        }
        Request::McpRemove { name } => {
            if !single_core::mcp::remove(&ctx.dirs.mcp_registry_file(), &name)? {
                anyhow::bail!("no such mcp server: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::McpEnable { name } => {
            if !single_core::mcp::set_enabled(&ctx.dirs.mcp_registry_file(), &name, true)? {
                anyhow::bail!("no such mcp server: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::McpDisable { name } => {
            if !single_core::mcp::set_enabled(&ctx.dirs.mcp_registry_file(), &name, false)? {
                anyhow::bail!("no such mcp server: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::McpInspect { name } => {
            let server = single_core::mcp::find(&ctx.dirs.mcp_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such mcp server: {name}"))?;
            Ok(ResponseData::McpServer(server))
        }
        Request::LspList => {
            Ok(ResponseData::LspServers(single_core::lsp::load(&ctx.dirs.lsp_registry_file())?))
        }
        Request::LspAdd { server } => {
            single_core::lsp::add(&ctx.dirs.lsp_registry_file(), server)?;
            Ok(ResponseData::Empty)
        }
        Request::LspRemove { name } => {
            if !single_core::lsp::remove(&ctx.dirs.lsp_registry_file(), &name)? {
                anyhow::bail!("no such lsp server: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::LspInspect { name } => {
            let server = single_core::lsp::find(&ctx.dirs.lsp_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such lsp server: {name}"))?;
            Ok(ResponseData::LspServer(server))
        }
        Request::ToolList => {
            Ok(ResponseData::Tools(single_core::tools::load(&ctx.dirs.tools_registry_file())?))
        }
        Request::ToolAdd { tool } => {
            single_core::tools::add(&ctx.dirs.tools_registry_file(), tool)?;
            Ok(ResponseData::Empty)
        }
        Request::ToolInspect { name } => {
            let tool = single_core::tools::find(&ctx.dirs.tools_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such tool: {name}"))?;
            Ok(ResponseData::Tool(tool))
        }
        Request::ToolEnable { name } => {
            if !single_core::tools::set_enabled(&ctx.dirs.tools_registry_file(), &name, true)? {
                anyhow::bail!("no such tool: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::ToolDisable { name } => {
            if !single_core::tools::set_enabled(&ctx.dirs.tools_registry_file(), &name, false)? {
                anyhow::bail!("no such tool: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::SecretList => {
            let store = single_core::secrets::SecretTool;
            Ok(ResponseData::SecretNames(single_core::secrets::SecretStore::list(&store)?))
        }
        Request::SecretSet { name, value } => {
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::set(&store, &name, &value)?;
            Ok(ResponseData::Empty)
        }
        Request::SecretGet { name } => {
            let store = single_core::secrets::SecretTool;
            Ok(ResponseData::SecretValue(single_core::secrets::SecretStore::get(&store, &name)?))
        }
        Request::SecretDelete { name } => {
            let store = single_core::secrets::SecretTool;
            if !single_core::secrets::SecretStore::delete(&store, &name)? {
                anyhow::bail!("no such secret: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::SkillList => Ok(ResponseData::Skills(single_core::skills::list(&ctx.dirs.skills_dir())?)),
        Request::SkillInstall { name, source_path } => {
            single_core::skills::install(&ctx.dirs.skills_dir(), &name, std::path::Path::new(&source_path))?;
            Ok(ResponseData::Empty)
        }
        Request::SkillRemove { name } => {
            if !single_core::skills::remove(&ctx.dirs.skills_dir(), &name)? {
                anyhow::bail!("no such skill: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::SkillInspect { name } => {
            let contents = single_core::skills::inspect(&ctx.dirs.skills_dir(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such skill: {name}"))?;
            Ok(ResponseData::SkillContents(contents))
        }
        Request::MemoryStore { scope, source, project, agent, task, title, content, confidence, expires_in_seconds } => {
            let conn = memory_db(ctx)?;
            let id = memory::store(&conn, memory::NewMemory {
                scope, source, project, agent, task, title, content, confidence, expires_in_seconds,
            })?;
            Ok(ResponseData::MemoryId(id))
        }
        Request::MemorySearch { query, scope, project } => {
            let conn = memory_db(ctx)?;
            let entries = memory::search(&conn, &query, scope, project.as_deref())?;
            Ok(ResponseData::MemoryEntries(entries))
        }
        Request::MemoryGet { id } => {
            let conn = memory_db(ctx)?;
            let entry = memory::get(&conn, id)?.ok_or_else(|| anyhow::anyhow!("no memory with id {id}"))?;
            Ok(ResponseData::MemoryEntry(entry))
        }
        Request::MemoryDelete { id } => {
            let conn = memory_db(ctx)?;
            if !memory::delete(&conn, id)? {
                anyhow::bail!("no memory with id {id}");
            }
            Ok(ResponseData::Empty)
        }
        Request::MemoryList { scope } => {
            let conn = memory_db(ctx)?;
            Ok(ResponseData::MemoryEntries(memory::list(&conn, scope)?))
        }
        Request::ContextShow { cwd } => {
            Ok(ResponseData::Context(single_core::project_context::resolve(std::path::Path::new(&cwd))))
        }
        Request::TaskRun { description, agent, cwd, use_worktree, timeout_secs } => {
            let conn = task_db(ctx)?;
            let record = crate::task::run(&conn, ctx, crate::task::RunTaskOptions {
                description: &description,
                agent: &agent,
                cwd: std::path::Path::new(&cwd),
                use_worktree,
                timeout: std::time::Duration::from_secs(timeout_secs),
            })?;
            Ok(ResponseData::Task(record))
        }
        Request::TaskList => {
            let conn = task_db(ctx)?;
            Ok(ResponseData::Tasks(crate::task::list(&conn)?))
        }
        Request::TaskInspect { id } => {
            let conn = task_db(ctx)?;
            let record = crate::task::get(&conn, id)?.ok_or_else(|| anyhow::anyhow!("no task with id {id}"))?;
            Ok(ResponseData::Task(record))
        }
        Request::AccountCapture { agent, name } => {
            let home = integrations::home_dir()?;
            let info = single_core::account::capture(&ctx.dirs.accounts_dir(), &home, &agent, &name)?;
            crate::state::open(&ctx.dirs.db_path())
                .and_then(|conn| crate::state::record_event(&conn, "account.captured", &format!("{agent}/{name}")))
                .ok();
            Ok(ResponseData::AccountProfile(info))
        }
        Request::AccountUse { agent, name } => {
            let home = integrations::home_dir()?;
            let result = single_core::account::switch(&ctx.dirs.accounts_dir(), &home, &agent, &name)?;
            crate::state::open(&ctx.dirs.db_path())
                .and_then(|conn| crate::state::record_event(&conn, "account.switched", &format!("{agent}/{name}")))
                .ok();
            Ok(ResponseData::AccountSwitched(result))
        }
        Request::AccountList { agent } => {
            Ok(ResponseData::AccountProfiles(single_core::account::list(&ctx.dirs.accounts_dir(), agent.as_deref())?))
        }
        Request::AccountRemove { agent, name } => {
            if !single_core::account::remove(&ctx.dirs.accounts_dir(), &agent, &name)? {
                anyhow::bail!("no profile named '{name}' for agent '{agent}'");
            }
            Ok(ResponseData::Empty)
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

fn memory_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    memory::ensure_schema(&conn)?;
    Ok(conn)
}

fn task_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    crate::task::ensure_schema(&conn)?;
    Ok(conn)
}

fn status(ctx: &Context) -> RuntimeStatus {
    let detected = ctx
        .registry
        .iter()
        .filter(|a| for_agent_with_custom(&a.name, &ctx.dirs.agents_dir()).map(|ad| ad.discover().detected).unwrap_or(false))
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

fn to_agent_info(def: &AgentDefinition, ctx: &Context) -> AgentInfo {
    let discovery = for_agent_with_custom(&def.name, &ctx.dirs.agents_dir()).map(|a| a.discover());
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
