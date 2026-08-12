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
        Request::McpPresetList => {
            let presets = single_core::mcp::presets()
                .into_iter()
                .map(|p| single_protocol::McpPresetInfo { name: p.name.to_string(), command: p.command.to_string(), args: p.args.iter().map(|s| s.to_string()).collect() })
                .collect();
            Ok(ResponseData::McpPresets(presets))
        }
        Request::McpAddPreset { name } => {
            let preset = single_core::mcp::preset(&name)
                .ok_or_else(|| anyhow::anyhow!("no such preset: {name} (see `single mcp presets`)"))?;
            single_core::mcp::add(&ctx.dirs.mcp_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
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
        Request::LspPresetList => {
            let presets = single_core::lsp::presets()
                .into_iter()
                .map(|p| single_protocol::LspPresetInfo {
                    name: p.name.to_string(),
                    command: p.command.to_string(),
                    args: p.args.iter().map(|s| s.to_string()).collect(),
                    extensions: p.extensions.iter().map(|s| s.to_string()).collect(),
                })
                .collect();
            Ok(ResponseData::LspPresets(presets))
        }
        Request::LspAddPreset { name } => {
            let preset = single_core::lsp::preset(&name)
                .ok_or_else(|| anyhow::anyhow!("no such preset: {name} (see `single lsp presets`)"))?;
            single_core::lsp::add(&ctx.dirs.lsp_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
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
        Request::SkillSyncClaude { name } => {
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, "claude")?;
            let claude_skills_dir = home.join(".claude").join("skills");
            let dest = single_core::skills::sync_to_claude(&ctx.dirs.skills_dir(), &claude_skills_dir, &name)?;
            Ok(ResponseData::SkillSynced { path: dest.display().to_string() })
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
        Request::TaskRun { description, agent, cwd, use_worktree, account, real_home, timeout_secs } => {
            let conn = task_db(ctx)?;
            let record = crate::task::run(&conn, ctx, crate::task::RunTaskOptions {
                description: &description,
                agent: &agent,
                cwd: std::path::Path::new(&cwd),
                use_worktree,
                account: account.as_deref(),
                real_home,
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
        Request::Orchestrate { goal, agents, cwd, use_worktree, real_home, timeout_secs } => {
            let conn = task_db(ctx)?;
            let records = crate::orchestrate::run(&conn, ctx, crate::orchestrate::OrchestrateOptions {
                goal: &goal,
                agents: &agents,
                cwd: std::path::Path::new(&cwd),
                use_worktree,
                real_home,
                timeout: std::time::Duration::from_secs(timeout_secs),
            })?;
            Ok(ResponseData::OrchestrateResult(records))
        }
        Request::AccountCapture { agent, name, label } => {
            // Captures from this agent's SingleCLI-managed home, not the
            // real ~/.claude etc. — that's the home single actually runs
            // the agent against (single_core::agent_home docs), bootstrapped
            // here if this is the first account operation for this agent.
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent)?;
            let info = single_core::account::capture(&ctx.dirs.accounts_dir(), &home, &agent, &name, label)?;
            crate::state::open(&ctx.dirs.db_path())
                .and_then(|conn| crate::state::record_event(&conn, "account.captured", &format!("{agent}/{name}")))
                .ok();
            Ok(ResponseData::AccountProfile(info))
        }
        Request::AccountUse { agent, name } => {
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent)?;
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
        Request::AccountSetStatus { agent, name, status } => {
            single_core::account::set_status(&ctx.dirs.accounts_dir(), &agent, &name, status)?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderAdd { name, env_var_name, base_url } => {
            let secret_name = format!("provider:{name}");
            single_core::providers::add(&ctx.dirs.providers_registry_file(), single_protocol::ProviderSpec {
                name, env_var_name, secret_name, base_url,
            })?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderAddPreset { name } => {
            let preset = single_core::providers::preset(&name)
                .ok_or_else(|| anyhow::anyhow!("no such preset: {name} (see `single provider presets`)"))?;
            single_core::providers::add(&ctx.dirs.providers_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderPresetList => {
            let presets = single_core::providers::presets()
                .into_iter()
                .map(|p| single_protocol::ProviderPresetInfo { name: p.name.to_string(), env_var_name: p.env_var_name.to_string(), base_url: p.base_url.to_string() })
                .collect();
            Ok(ResponseData::ProviderPresets(presets))
        }
        Request::ProviderRemove { name } => {
            if !single_core::providers::remove(&ctx.dirs.providers_registry_file(), &name)? {
                anyhow::bail!("no such provider: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::ProviderList => {
            Ok(ResponseData::Providers(single_core::providers::load(&ctx.dirs.providers_registry_file())?))
        }
        Request::ProviderInspect { name } => {
            let provider = single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such provider: {name}"))?;
            Ok(ResponseData::Provider(provider))
        }
        Request::ProviderSetKey { name, value } => {
            let provider = single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such provider: {name} (add it first with `single provider add`)"))?;
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::set(&store, &provider.secret_name, &value)?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderSync { name, agents, dry_run } => {
            let provider = single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such provider: {name}"))?;
            let store = single_core::secrets::SecretTool;
            let value = single_core::secrets::SecretStore::get(&store, &provider.secret_name)?
                .ok_or_else(|| anyhow::anyhow!("no key stored for provider '{name}'; run `single provider set-key {name} <value>` first"))?;
            let real_home = integrations::home_dir()?;
            let target_agents: Vec<String> = if agents.is_empty() { ctx.registry.iter().map(|a| a.name.clone()).collect() } else { agents };
            let mut results = Vec::new();
            for agent in target_agents {
                let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent)?;
                let mut result = single_agent_sdk::provider_sync::sync(&agent, &home, &provider.env_var_name, &value, dry_run)?;
                result.provider = name.clone();
                results.push(result);
            }
            Ok(ResponseData::ProviderSyncResults(results))
        }
        Request::PluginAdd { plugin } => {
            single_core::plugins::add(&ctx.dirs.plugins_registry_file(), plugin)?;
            Ok(ResponseData::Empty)
        }
        Request::PluginRemove { name } => {
            if !single_core::plugins::remove(&ctx.dirs.plugins_registry_file(), &name)? {
                anyhow::bail!("no such plugin: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::PluginList => {
            Ok(ResponseData::Plugins(single_core::plugins::load(&ctx.dirs.plugins_registry_file())?))
        }
        Request::PluginInspect { name } => {
            let plugin = single_core::plugins::find(&ctx.dirs.plugins_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such plugin: {name}"))?;
            Ok(ResponseData::Plugin(plugin))
        }
        Request::PluginSync { name, agents, dry_run } => {
            let plugin = single_core::plugins::find(&ctx.dirs.plugins_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such plugin: {name}"))?;
            let real_home = integrations::home_dir()?;
            let target_agents: Vec<String> = if agents.is_empty() { ctx.registry.iter().map(|a| a.name.clone()).collect() } else { agents };
            let mut results = Vec::new();
            for agent in target_agents {
                let selector = if agent == "opencode" {
                    plugin.opencode_module.clone().unwrap_or_else(|| plugin.target.clone())
                } else {
                    plugin.target.clone()
                };
                let (applied, detail) = match for_agent_with_custom(&agent, &ctx.dirs.agents_dir()) {
                    Some(adapter) if dry_run => {
                        (false, format!("dry run: would run `{} plugin install {selector}`", adapter.command()))
                    }
                    Some(adapter) => {
                        let home = single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &real_home, &agent)?;
                        match adapter.install_plugin(&selector, &home, std::time::Duration::from_secs(60)) {
                            Ok(outcome) if outcome.success => (true, "installed".to_string()),
                            Ok(outcome) => (false, format!("exited with {:?}: {}", outcome.exit_code, outcome.stderr)),
                            Err(err) => (false, err.to_string()),
                        }
                    }
                    None => (false, format!("no adapter for agent '{agent}'")),
                };
                results.push(single_protocol::PluginInstallResult { plugin: name.clone(), agent, applied, detail });
            }
            Ok(ResponseData::PluginSyncResults(results))
        }
        Request::KgCreateEntity { name, entity_type } => {
            let conn = kg_db(ctx)?;
            crate::knowledge_graph::create_entity(&conn, &name, &entity_type)?;
            Ok(ResponseData::Empty)
        }
        Request::KgAddObservation { entity, content } => {
            let conn = kg_db(ctx)?;
            let id = crate::knowledge_graph::add_observation(&conn, &entity, &content)?;
            Ok(ResponseData::KgEntityId(id))
        }
        Request::KgCreateRelation { from, to, relation_type } => {
            let conn = kg_db(ctx)?;
            let id = crate::knowledge_graph::create_relation(&conn, &from, &to, &relation_type)?;
            Ok(ResponseData::KgEntityId(id))
        }
        Request::KgDeleteEntity { name } => {
            let conn = kg_db(ctx)?;
            if !crate::knowledge_graph::delete_entity(&conn, &name)? {
                anyhow::bail!("no such entity: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::KgGetEntity { name } => {
            let conn = kg_db(ctx)?;
            let entity = crate::knowledge_graph::get_entity(&conn, &name)?.ok_or_else(|| anyhow::anyhow!("no such entity: {name}"))?;
            Ok(ResponseData::KgEntity(entity))
        }
        Request::KgQuery { term } => {
            let conn = kg_db(ctx)?;
            Ok(ResponseData::KgEntities(crate::knowledge_graph::query(&conn, &term)?))
        }
        Request::KgReadGraph => {
            let conn = kg_db(ctx)?;
            Ok(ResponseData::KgGraph(crate::knowledge_graph::read_graph(&conn)?))
        }
        Request::CacheSet { key, value, ttl_secs } => {
            let url = redis_url()?;
            crate::redis_backend::set(&url, &key, &value, ttl_secs)?;
            Ok(ResponseData::Empty)
        }
        Request::CacheGet { key } => {
            let url = redis_url()?;
            Ok(ResponseData::CacheValue(crate::redis_backend::get(&url, &key)?))
        }
        Request::CacheDelete { key } => {
            let url = redis_url()?;
            if !crate::redis_backend::delete(&url, &key)? {
                anyhow::bail!("no such key: {key}");
            }
            Ok(ResponseData::Empty)
        }
        Request::CacheList { pattern } => {
            let url = redis_url()?;
            Ok(ResponseData::CacheKeys(crate::redis_backend::list_keys(&url, &pattern)?))
        }
        Request::CacheStatus => {
            let url = crate::redis_backend::resolve_url();
            let reachable = url.as_deref().map(|u| crate::redis_backend::ping(u).is_ok()).unwrap_or(false);
            Ok(ResponseData::CacheStatus { configured: url.is_some(), url, reachable })
        }
        Request::VectorUpsert { collection, id, vector, payload } => {
            let url = qdrant_url()?;
            crate::qdrant_backend::upsert_point(&url, &collection, id, &vector, payload)?;
            Ok(ResponseData::Empty)
        }
        Request::VectorSearch { collection, vector, limit } => {
            let url = qdrant_url()?;
            Ok(ResponseData::VectorHits(crate::qdrant_backend::search(&url, &collection, &vector, limit)?))
        }
        Request::VectorDelete { collection, id } => {
            let url = qdrant_url()?;
            crate::qdrant_backend::delete_point(&url, &collection, id)?;
            Ok(ResponseData::Empty)
        }
        Request::VectorStatus => {
            let url = crate::qdrant_backend::resolve_url();
            let reachable = url.as_deref().map(|u| crate::qdrant_backend::ping(u).is_ok()).unwrap_or(false);
            Ok(ResponseData::VectorStatus { configured: url.is_some(), url, reachable })
        }
        Request::AgentInstall { name, dry_run } => {
            Ok(ResponseData::AgentInstallResult(bootstrap::run_one(ctx, &name, dry_run)?))
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

fn redis_url() -> anyhow::Result<String> {
    crate::redis_backend::resolve_url()
        .ok_or_else(|| anyhow::anyhow!("no Redis configured; set SINGLE_REDIS_URL to enable the working-memory cache"))
}

fn qdrant_url() -> anyhow::Result<String> {
    crate::qdrant_backend::resolve_url()
        .ok_or_else(|| anyhow::anyhow!("no Qdrant configured; set SINGLE_QDRANT_URL to enable the vector store"))
}

fn kg_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    crate::knowledge_graph::ensure_schema(&conn)?;
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
