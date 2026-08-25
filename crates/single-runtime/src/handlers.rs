use crate::context::Context;
use crate::{bootstrap, doctor, integrations, memory};
use anyhow::Context as _;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_agent_sdk::Discovery;
use single_core::registry::AgentDefinition;
use single_protocol::{AgentInfo, McpServerInfo, Request, Response, ResponseData, RuntimeStatus};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub fn handle(ctx: &Context, request: Request) -> Response {
    handle_with_registry(ctx, request, &crate::registry::TaskRegistry::default())
}

/// Same as `handle`, but with an explicit `TaskRegistry` — used by the real
/// daemon (`server.rs`), which creates one registry at startup and shares
/// it (cloned — cheap, it's an `Arc`) across every connection so a
/// `TaskCancel` sent on one connection can find a task started with
/// `background: true` on another. `handle`'s throwaway per-call registry
/// is only reachable from `client::send`'s no-daemon-running fallback,
/// where there's no persistent process for a later `TaskCancel` to even
/// reach anyway — a `background: true` run there still starts, but has
/// nothing keeping it alive once the one-shot CLI invocation exits.
pub fn handle_with_registry(
    ctx: &Context,
    request: Request,
    registry: &crate::registry::TaskRegistry,
) -> Response {
    match dispatch(ctx, request, registry) {
        Ok(data) => Response::Ok { data },
        Err(e) => Response::Error {
            message: format!("{e:#}"),
        },
    }
}

fn dispatch(
    ctx: &Context,
    request: Request,
    registry: &crate::registry::TaskRegistry,
) -> anyhow::Result<ResponseData> {
    match request {
        Request::Status => Ok(ResponseData::Status(status(ctx))),
        Request::Doctor => Ok(ResponseData::Doctor(doctor::run(ctx))),
        // Actual process exit happens in server.rs after this response is
        // flushed to the client — see its handle_connection.
        Request::Shutdown => Ok(ResponseData::Empty),
        Request::AgentList => {
            // Parallelized across agents for the same reason `status()` is
            // — see `cached_discover`'s doc comment.
            let agents = std::thread::scope(|scope| {
                let handles: Vec<_> = ctx
                    .registry
                    .iter()
                    .map(|def| scope.spawn(|| to_agent_info(def, ctx)))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
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
                    command: format!("{} {}", s.command, s.args.join(" "))
                        .trim()
                        .to_string(),
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
                .map(|p| single_protocol::McpPresetInfo {
                    name: p.name.to_string(),
                    command: p.command.to_string(),
                    args: p.args.iter().map(|s| s.to_string()).collect(),
                })
                .collect();
            Ok(ResponseData::McpPresets(presets))
        }
        Request::McpAddPreset { name } => {
            let preset = single_core::mcp::preset(&name).ok_or_else(|| {
                anyhow::anyhow!("no such preset: {name} (see `single mcp presets`)")
            })?;
            single_core::mcp::add(&ctx.dirs.mcp_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
        }
        Request::McpGatewaySetEnabled { enabled } => {
            single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), enabled)?;
            Ok(ResponseData::Empty)
        }
        Request::McpGatewayStatus => Ok(ResponseData::McpGatewayMode(
            single_core::mcp::gateway_mode(&ctx.dirs.mcp_gateway_file())?,
        )),
        Request::LspList => Ok(ResponseData::LspServers(single_core::lsp::load(
            &ctx.dirs.lsp_registry_file(),
        )?)),
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
        Request::LspEnable { name } => {
            if !single_core::lsp::set_enabled(&ctx.dirs.lsp_registry_file(), &name, true)? {
                anyhow::bail!("no such lsp server: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::LspDisable { name } => {
            if !single_core::lsp::set_enabled(&ctx.dirs.lsp_registry_file(), &name, false)? {
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
            let preset = single_core::lsp::preset(&name).ok_or_else(|| {
                anyhow::anyhow!("no such preset: {name} (see `single lsp presets`)")
            })?;
            single_core::lsp::add(&ctx.dirs.lsp_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
        }
        Request::ToolList => Ok(ResponseData::Tools(single_core::tools::load(
            &ctx.dirs.tools_registry_file(),
        )?)),
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
            Ok(ResponseData::SecretNames(
                single_core::secrets::SecretStore::list(&store)?,
            ))
        }
        Request::SecretSet { name, value } => {
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::set(&store, &name, &value)?;
            Ok(ResponseData::Empty)
        }
        Request::SecretGet { name } => {
            let store = single_core::secrets::SecretTool;
            Ok(ResponseData::SecretValue(
                single_core::secrets::SecretStore::get(&store, &name)?,
            ))
        }
        Request::SecretDelete { name } => {
            let store = single_core::secrets::SecretTool;
            if !single_core::secrets::SecretStore::delete(&store, &name)? {
                anyhow::bail!("no such secret: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::SkillList => Ok(ResponseData::Skills(single_core::skills::list(
            &ctx.dirs.skills_dir(),
        )?)),
        Request::SkillInstall { name, source_path } => {
            single_core::skills::install(
                &ctx.dirs.skills_dir(),
                &name,
                std::path::Path::new(&source_path),
            )?;
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
            let home = single_core::agent_home::ensure_bootstrapped(
                &ctx.dirs.homes_dir(),
                &real_home,
                "claude",
            )?;
            let claude_skills_dir = home.join(".claude").join("skills");
            let dest = single_core::skills::sync_to_claude(
                &ctx.dirs.skills_dir(),
                &claude_skills_dir,
                &name,
            )?;
            Ok(ResponseData::SkillSynced {
                path: dest.display().to_string(),
            })
        }
        Request::SkillStarterList => {
            let starters = single_core::skills::starter_set()
                .into_iter()
                .map(|s| single_protocol::SkillStarterInfo {
                    name: s.name.to_string(),
                    description: s.description.to_string(),
                })
                .collect();
            Ok(ResponseData::SkillStarters(starters))
        }
        Request::SkillInstallStarter { name } => {
            single_core::skills::install_starter(&ctx.dirs.skills_dir(), &name)?;
            Ok(ResponseData::Empty)
        }
        Request::MemoryStore {
            scope,
            source,
            project,
            agent,
            task,
            title,
            content,
            confidence,
            expires_in_seconds,
        } => {
            let conn = memory_db(ctx)?;
            let id = memory::store(
                &conn,
                memory::NewMemory {
                    scope,
                    source,
                    project: project.clone(),
                    agent,
                    task,
                    title: title.clone(),
                    content: content.clone(),
                    confidence,
                    expires_in_seconds,
                },
            )?;
            // Best-effort: index for semantic search if both an embeddings
            // key and SINGLE_QDRANT_URL are configured. Never fails the
            // write itself — see embeddings.rs's module docs.
            if let Some(url) = crate::qdrant_backend::resolve_url() {
                if let Ok(vector) = crate::embeddings::embed(&format!("{title}\n{content}")) {
                    let payload = serde_json::json!({ "memory_id": id, "project": project });
                    let _ = crate::qdrant_backend::upsert_point(
                        &url,
                        "single_memory",
                        id as u64,
                        &vector,
                        payload,
                    );
                }
            }
            Ok(ResponseData::MemoryId(id))
        }
        Request::MemorySearch {
            query,
            scope,
            project,
        } => {
            let conn = memory_db(ctx)?;
            let entries = memory::search(&conn, &query, scope, project.as_deref())?;
            Ok(ResponseData::MemoryEntries(entries))
        }
        Request::MemorySearchSemantic {
            query,
            scope,
            project,
            limit,
        } => {
            let conn = memory_db(ctx)?;
            let semantic: anyhow::Result<Vec<single_protocol::MemoryEntry>> = (|| {
                let url =
                    crate::qdrant_backend::resolve_url().context("SINGLE_QDRANT_URL is not set")?;
                let vector = crate::embeddings::embed(&query)?;
                let hits = crate::qdrant_backend::search(&url, "single_memory", &vector, limit)?;
                let mut entries = Vec::new();
                for hit in hits {
                    let Some(memory_id) = hit.payload.get("memory_id").and_then(|v| v.as_i64())
                    else {
                        continue;
                    };
                    if let Some(entry) = memory::get(&conn, memory_id)? {
                        if project.is_none() || entry.project == project {
                            entries.push(entry);
                        }
                    }
                }
                Ok(entries)
            })();
            match semantic {
                Ok(entries) => Ok(ResponseData::MemoryEntries(entries)),
                Err(e) => {
                    eprintln!("note: semantic memory search unavailable ({e:#}) — falling back to substring search");
                    Ok(ResponseData::MemoryEntries(memory::search(
                        &conn,
                        &query,
                        scope,
                        project.as_deref(),
                    )?))
                }
            }
        }
        Request::MemoryGet { id } => {
            let conn = memory_db(ctx)?;
            let entry =
                memory::get(&conn, id)?.ok_or_else(|| anyhow::anyhow!("no memory with id {id}"))?;
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
        Request::NoteLeave {
            project,
            from_agent,
            to_agent,
            topic,
            content,
        } => {
            let conn = notes_db(ctx)?;
            let id = single_core::notes::leave(
                &conn,
                project,
                &from_agent,
                to_agent.as_deref(),
                &topic,
                &content,
            )?;
            Ok(ResponseData::NoteId(id))
        }
        Request::NoteInbox {
            project,
            to_agent,
            unread_only,
        } => {
            let conn = notes_db(ctx)?;
            Ok(ResponseData::Notes(single_core::notes::inbox(
                &conn,
                project.as_deref(),
                &to_agent,
                unread_only,
            )?))
        }
        Request::NoteMarkRead { id } => {
            let conn = notes_db(ctx)?;
            if !single_core::notes::mark_read(&conn, id)? {
                anyhow::bail!("no unread note with id {id}");
            }
            Ok(ResponseData::Empty)
        }
        Request::DocumentIngest {
            path,
            project,
            title,
        } => {
            let conn = documents_db(ctx)?;
            let doc = crate::documents::ingest(
                &conn,
                &ctx.dirs.documents_dir(),
                std::path::Path::new(&path),
                project,
                title,
            )?;
            Ok(ResponseData::Document(to_document_info(doc)))
        }
        Request::DocumentList { project } => {
            let conn = documents_db(ctx)?;
            let docs = crate::documents::list(&conn, project.as_deref())?
                .into_iter()
                .map(to_document_info)
                .collect();
            Ok(ResponseData::Documents(docs))
        }
        Request::DocumentGet { id } => {
            let conn = documents_db(ctx)?;
            let doc = crate::documents::get(&conn, id)?
                .ok_or_else(|| anyhow::anyhow!("no document with id {id}"))?;
            Ok(ResponseData::Document(to_document_info(doc)))
        }
        Request::ContextShow { cwd } => Ok(ResponseData::Context(
            single_core::project_context::resolve(std::path::Path::new(&cwd)),
        )),
        Request::TaskRun {
            description,
            agent,
            cwd,
            use_worktree,
            account,
            real_home,
            no_memory_context,
            timeout_secs,
            background,
            allow_fallback,
        } => {
            if background {
                let record = crate::task::run_background(
                    ctx,
                    crate::task::OwnedRunTaskOptions {
                        description,
                        agent,
                        cwd: std::path::PathBuf::from(cwd),
                        use_worktree,
                        account,
                        real_home,
                        no_memory_context,
                        timeout: std::time::Duration::from_secs(timeout_secs),
                        allow_fallback,
                    },
                    registry.clone(),
                )?;
                return Ok(ResponseData::Task(record));
            }
            let conn = task_db(ctx)?;
            let record = crate::task::run(
                &conn,
                ctx,
                crate::task::RunTaskOptions {
                    description: &description,
                    agent: &agent,
                    cwd: std::path::Path::new(&cwd),
                    use_worktree,
                    account: account.as_deref(),
                    real_home,
                    no_memory_context,
                    timeout: std::time::Duration::from_secs(timeout_secs),
                    allow_fallback,
                },
            )?;
            Ok(ResponseData::Task(record))
        }
        Request::TaskList => {
            let conn = task_db(ctx)?;
            Ok(ResponseData::Tasks(crate::task::list(&conn)?))
        }
        Request::WorkspaceList => {
            let conn = task_db(ctx)?;
            Ok(ResponseData::Workspaces(crate::task::list_workspaces(&conn)?))
        }
        Request::TaskInspect { id } => {
            let conn = task_db(ctx)?;
            let record = crate::task::get(&conn, id)?
                .ok_or_else(|| anyhow::anyhow!("no task with id {id}"))?;
            Ok(ResponseData::Task(record))
        }
        Request::TaskCancel { id } => {
            if !registry.cancel(id) {
                anyhow::bail!(
                    "task #{id} isn't currently running in the background — nothing to cancel"
                );
            }
            Ok(ResponseData::Empty)
        }
        Request::TaskCleanup { id } => {
            let conn = task_db(ctx)?;
            crate::task::cleanup(&conn, ctx, id)?;
            Ok(ResponseData::Empty)
        }
        Request::Orchestrate {
            goal,
            agents,
            cwd,
            use_worktree,
            real_home,
            timeout_secs,
        } => {
            let conn = task_db(ctx)?;
            let records = crate::orchestrate::run(
                &conn,
                ctx,
                crate::orchestrate::OrchestrateOptions {
                    goal: &goal,
                    agents: &agents,
                    cwd: std::path::Path::new(&cwd),
                    use_worktree,
                    real_home,
                    timeout: std::time::Duration::from_secs(timeout_secs),
                },
            )?;
            Ok(ResponseData::OrchestrateResult(records))
        }
        Request::OrchestrateParallel {
            tasks,
            cwd,
            real_home,
            timeout_secs,
            background,
            orchestrator,
            goal,
            candidate_agents,
        } => {
            if orchestrator != single_protocol::OrchestratorMode::Fixed {
                let goal = goal.context("non-fixed orchestration requires a goal")?;
                let cwd_buf = std::path::PathBuf::from(cwd);
                if background {
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let _ = crate::orchestrate_graph::plan_and_run(
                            &ctx,
                            orchestrator,
                            &goal,
                            &candidate_agents,
                            &cwd_buf,
                            real_home,
                            std::time::Duration::from_secs(timeout_secs),
                        );
                    });
                    return Ok(ResponseData::OrchestrateResult(Vec::new()));
                }
                let records = crate::orchestrate_graph::plan_and_run(
                    ctx,
                    orchestrator,
                    &goal,
                    &candidate_agents,
                    &cwd_buf,
                    real_home,
                    std::time::Duration::from_secs(timeout_secs),
                )?;
                return Ok(ResponseData::OrchestrateResult(records));
            }
            let task_pairs: Vec<(String, String)> = tasks
                .into_iter()
                .map(|t| (t.agent, t.description))
                .collect();
            if background {
                // run_parallel is itself a blocking join() over every
                // sub-task's own thread — unlike TaskRun::background,
                // there's no pre-created row to hand back immediately
                // here (each sub-task creates its own inside its thread),
                // so the honest contract is: this returns an empty batch
                // right away, and the real per-task records show up in
                // `TaskList`/`TaskInspect` as each one starts and finishes.
                let ctx = ctx.clone();
                let cwd_buf = std::path::PathBuf::from(cwd);
                std::thread::spawn(move || {
                    let _ = crate::orchestrate::run_parallel(
                        &ctx,
                        crate::orchestrate::ParallelOrchestrateOptions {
                            tasks: &task_pairs,
                            cwd: &cwd_buf,
                            real_home,
                            timeout: std::time::Duration::from_secs(timeout_secs),
                        },
                    );
                });
                return Ok(ResponseData::OrchestrateResult(Vec::new()));
            }
            let records = crate::orchestrate::run_parallel(
                ctx,
                crate::orchestrate::ParallelOrchestrateOptions {
                    tasks: &task_pairs,
                    cwd: std::path::Path::new(&cwd),
                    real_home,
                    timeout: std::time::Duration::from_secs(timeout_secs),
                },
            )?;
            Ok(ResponseData::OrchestrateResult(records))
        }
        Request::OrchestrateGraph {
            nodes,
            cwd,
            real_home,
            timeout_secs,
            background,
            orchestrator,
            goal,
            candidate_agents,
        } => {
            if orchestrator != single_protocol::OrchestratorMode::Fixed {
                let goal = goal.context("non-fixed orchestration requires a goal")?;
                let cwd_buf = std::path::PathBuf::from(cwd);
                if background {
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let _ = crate::orchestrate_graph::plan_and_run(
                            &ctx,
                            orchestrator,
                            &goal,
                            &candidate_agents,
                            &cwd_buf,
                            real_home,
                            std::time::Duration::from_secs(timeout_secs),
                        );
                    });
                    return Ok(ResponseData::OrchestrateGraphResult(Vec::new()));
                }
                let records = crate::orchestrate_graph::plan_and_run(
                    ctx,
                    orchestrator,
                    &goal,
                    &candidate_agents,
                    &cwd_buf,
                    real_home,
                    std::time::Duration::from_secs(timeout_secs),
                )?;
                return Ok(ResponseData::OrchestrateGraphResult(records));
            }
            if background {
                let ctx = ctx.clone();
                let cwd_buf = std::path::PathBuf::from(cwd);
                std::thread::spawn(move || {
                    let _ = crate::orchestrate_graph::run(
                        &ctx,
                        crate::orchestrate_graph::GraphOrchestrateOptions {
                            nodes: &nodes,
                            cwd: &cwd_buf,
                            real_home,
                            timeout: std::time::Duration::from_secs(timeout_secs),
                        },
                    );
                });
                return Ok(ResponseData::OrchestrateGraphResult(Vec::new()));
            }
            let records = crate::orchestrate_graph::run(
                ctx,
                crate::orchestrate_graph::GraphOrchestrateOptions {
                    nodes: &nodes,
                    cwd: std::path::Path::new(&cwd),
                    real_home,
                    timeout: std::time::Duration::from_secs(timeout_secs),
                },
            )?;
            Ok(ResponseData::OrchestrateGraphResult(records))
        }
        Request::AccountCapture { agent, name, label } => {
            // Captures only from this agent's SingleCLI-managed home,
            // bootstrapped here if this is the first account operation for
            // this agent. The real ~/.claude etc. is never read for the
            // capture itself — only used to seed a brand-new isolated home's
            // non-credential config once (see agent_home::ensure_bootstrapped).
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(
                &ctx.dirs.homes_dir(),
                &real_home,
                &agent,
            )?;
            let info = single_core::account::capture(
                &ctx.dirs.accounts_dir(),
                &home,
                &agent,
                &name,
                label,
            )?;
            crate::state::open(&ctx.dirs.db_path())
                .and_then(|conn| {
                    crate::state::record_event(
                        &conn,
                        "account.captured",
                        &format!("{agent}/{name}"),
                    )
                })
                .ok();
            Ok(ResponseData::AccountProfile(info))
        }
        Request::AccountUse { agent, name } => {
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(
                &ctx.dirs.homes_dir(),
                &real_home,
                &agent,
            )?;
            let result =
                single_core::account::switch(&ctx.dirs.accounts_dir(), &home, &agent, &name)?;
            crate::state::open(&ctx.dirs.db_path())
                .and_then(|conn| {
                    crate::state::record_event(
                        &conn,
                        "account.switched",
                        &format!("{agent}/{name}"),
                    )
                })
                .ok();
            Ok(ResponseData::AccountSwitched(result))
        }
        Request::AccountList { agent } => Ok(ResponseData::AccountProfiles(
            single_core::account::list(&ctx.dirs.accounts_dir(), agent.as_deref())?,
        )),
        Request::AccountRemove { agent, name } => {
            if !single_core::account::remove(&ctx.dirs.accounts_dir(), &agent, &name)? {
                anyhow::bail!("no profile named '{name}' for agent '{agent}'");
            }
            Ok(ResponseData::Empty)
        }
        Request::AccountSetStatus {
            agent,
            name,
            status,
        } => {
            single_core::account::set_status(&ctx.dirs.accounts_dir(), &agent, &name, status)?;
            Ok(ResponseData::Empty)
        }
        Request::DockerEnable { agent, account } => {
            single_core::docker::set_enabled(
                &ctx.dirs.docker_registry_file(),
                &agent,
                account.as_deref(),
                true,
            )?;
            Ok(ResponseData::Empty)
        }
        Request::DockerDisable { agent, account } => {
            single_core::docker::set_enabled(
                &ctx.dirs.docker_registry_file(),
                &agent,
                account.as_deref(),
                false,
            )?;
            Ok(ResponseData::Empty)
        }
        Request::DockerStatus { agent } => {
            let settings =
                single_core::docker::status(&ctx.dirs.docker_registry_file(), agent.as_deref())?;
            let infos = settings
                .into_iter()
                .map(|s| {
                    let container_name =
                        single_core::docker::container_name(&s.agent, s.account.as_deref());
                    let running = crate::docker::is_running(&container_name).unwrap_or(None);
                    single_protocol::DockerContainerInfo {
                        agent: s.agent,
                        account: s.account,
                        container_name,
                        enabled: s.enabled,
                        running,
                    }
                })
                .collect();
            Ok(ResponseData::DockerContainerList(infos))
        }
        Request::DockerStop { agent, account } => {
            let container = single_core::docker::container_name(&agent, account.as_deref());
            crate::docker::stop(&container)?;
            let enabled = single_core::docker::is_enabled(
                &ctx.dirs.docker_registry_file(),
                &agent,
                account.as_deref(),
            )?;
            Ok(ResponseData::DockerContainerInfo(
                single_protocol::DockerContainerInfo {
                    agent,
                    account,
                    container_name: container,
                    enabled,
                    running: Some(false),
                },
            ))
        }
        Request::HooksEnable { agent } => {
            single_core::hooks::set_enabled(&ctx.dirs.hooks_registry_file(), &agent, true)?;
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(
                &ctx.dirs.homes_dir(),
                &real_home,
                &agent,
            )?;
            let settings_path = home.join(".claude/settings.json");
            let hook_command = format!(
                "{} internal claude-pretooluse-hook",
                resolve_single_binary_path()
            );
            let updated = single_agent_sdk::formats::claude_settings::apply_hook(
                &settings_path,
                &hook_command,
                single_core::hooks::CLAUDE_HOOK_TIMEOUT_SECS,
            )?;
            write_settings_with_backup(&settings_path, &updated)?;
            Ok(ResponseData::Empty)
        }
        Request::HooksDisable { agent } => {
            single_core::hooks::set_enabled(&ctx.dirs.hooks_registry_file(), &agent, false)?;
            let settings_path = ctx
                .dirs
                .homes_dir()
                .join(&agent)
                .join(".claude/settings.json");
            let hook_command = format!(
                "{} internal claude-pretooluse-hook",
                resolve_single_binary_path()
            );
            if let Some(updated) = single_agent_sdk::formats::claude_settings::remove_hook(
                &settings_path,
                &hook_command,
            )? {
                write_settings_with_backup(&settings_path, &updated)?;
            }
            Ok(ResponseData::Empty)
        }
        Request::HooksStatus => Ok(ResponseData::HooksStatus(single_core::hooks::status(
            &ctx.dirs.hooks_registry_file(),
        )?)),
        Request::ApprovalList => {
            let conn = preferences_db(ctx)?;
            let approvals = single_core::preferences::list_pending(&conn)?
                .into_iter()
                .map(to_approval_info)
                .collect();
            Ok(ResponseData::Approvals(approvals))
        }
        Request::ApprovalResolve {
            id,
            allow,
            remember,
        } => {
            let conn = preferences_db(ctx)?;
            single_core::preferences::resolve(&conn, id, allow, remember)?;
            Ok(ResponseData::Empty)
        }
        Request::PreferenceList => {
            let conn = preferences_db(ctx)?;
            let prefs = single_core::preferences::list_preferences(&conn)?
                .into_iter()
                .map(to_preference_info)
                .collect();
            Ok(ResponseData::Preferences(prefs))
        }
        Request::ProviderAdd {
            name,
            env_var_name,
            base_url,
        } => {
            let secret_name = format!("provider:{name}");
            single_core::providers::add(
                &ctx.dirs.providers_registry_file(),
                single_protocol::ProviderSpec {
                    name,
                    env_var_name,
                    secret_name,
                    base_url,
                },
            )?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderAddPreset { name } => {
            let preset = single_core::providers::preset(&name).ok_or_else(|| {
                anyhow::anyhow!("no such preset: {name} (see `single provider presets`)")
            })?;
            single_core::providers::add(&ctx.dirs.providers_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderPresetList => {
            let presets = single_core::providers::presets()
                .into_iter()
                .map(|p| single_protocol::ProviderPresetInfo {
                    name: p.name.to_string(),
                    env_var_name: p.env_var_name.to_string(),
                    base_url: p.base_url.to_string(),
                })
                .collect();
            Ok(ResponseData::ProviderPresets(presets))
        }
        Request::ProviderRemove { name } => {
            if !single_core::providers::remove(&ctx.dirs.providers_registry_file(), &name)? {
                anyhow::bail!("no such provider: {name}");
            }
            Ok(ResponseData::Empty)
        }
        Request::ProviderList => Ok(ResponseData::Providers(single_core::providers::load(
            &ctx.dirs.providers_registry_file(),
        )?)),
        Request::ConfiguredProviderList => {
            let configured = single_core::providers::configured(
                &ctx.dirs.providers_registry_file(),
                &ctx.dirs.provider_keys_registry_file(),
            )?;
            Ok(ResponseData::Providers(configured))
        }
        Request::ProviderInspect { name } => {
            let provider =
                single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                    .ok_or_else(|| anyhow::anyhow!("no such provider: {name}"))?;
            Ok(ResponseData::Provider(provider))
        }
        Request::ProviderSetKey { name, value } => {
            let provider =
                single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "no such provider: {name} (add it first with `single provider add`)"
                        )
                    })?;
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::set(&store, &provider.secret_name, &value)?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderSync {
            name,
            agents,
            dry_run,
        } => {
            let provider =
                single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                    .ok_or_else(|| anyhow::anyhow!("no such provider: {name}"))?;
            let store = single_core::secrets::SecretTool;
            let value = single_core::secrets::SecretStore::get(&store, &provider.secret_name)?
                .ok_or_else(|| anyhow::anyhow!("no key stored for provider '{name}'; run `single provider set-key {name} <value>` first"))?;
            let real_home = integrations::home_dir()?;
            let target_agents: Vec<String> = if agents.is_empty() {
                ctx.registry.iter().map(|a| a.name.clone()).collect()
            } else {
                agents
            };
            let mut results = Vec::new();
            for agent in target_agents {
                let home = single_core::agent_home::ensure_bootstrapped(
                    &ctx.dirs.homes_dir(),
                    &real_home,
                    &agent,
                )?;
                let mut result = single_agent_sdk::provider_sync::sync(
                    &agent,
                    &home,
                    &provider.env_var_name,
                    &value,
                    dry_run,
                )?;
                result.provider = name.clone();
                results.push(result);
            }
            Ok(ResponseData::ProviderSyncResults(results))
        }
        Request::ProviderAddKey {
            provider,
            label,
            agent,
            value,
        } => {
            let store = single_core::secrets::SecretTool;
            let secret_name = single_core::provider_keys::secret_name(&provider, &label);
            single_core::secrets::SecretStore::set(&store, &secret_name, &value)?;
            single_core::provider_keys::add(
                &ctx.dirs.provider_keys_registry_file(),
                single_protocol::ProviderKeySpec {
                    provider,
                    label,
                    agent,
                    secret_name,
                },
            )?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderListKeys { provider } => {
            let keys = single_core::provider_keys::list_for_provider(
                &ctx.dirs.provider_keys_registry_file(),
                &provider,
            )?;
            Ok(ResponseData::ProviderKeys(keys))
        }
        Request::ProviderRemoveKey { provider, label } => {
            let key = single_core::provider_keys::find(
                &ctx.dirs.provider_keys_registry_file(),
                &provider,
                &label,
            )?
            .ok_or_else(|| anyhow::anyhow!("no such key: {provider}:{label}"))?;
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::delete(&store, &key.secret_name)?;
            single_core::provider_keys::remove(
                &ctx.dirs.provider_keys_registry_file(),
                &provider,
                &label,
            )?;
            Ok(ResponseData::Empty)
        }
        Request::ProviderKeySync {
            provider,
            label,
            agent,
            dry_run,
        } => {
            let provider_spec =
                single_core::providers::find(&ctx.dirs.providers_registry_file(), &provider)?
                    .ok_or_else(|| anyhow::anyhow!("no such provider: {provider}"))?;
            let key = single_core::provider_keys::find(
                &ctx.dirs.provider_keys_registry_file(),
                &provider,
                &label,
            )?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no such key: {provider}:{label} (add it first with `single provider add-key`)"
                )
            })?;
            let store = single_core::secrets::SecretTool;
            let value = single_core::secrets::SecretStore::get(&store, &key.secret_name)?
                .ok_or_else(|| anyhow::anyhow!("key '{provider}:{label}' has no value stored"))?;
            let real_home = integrations::home_dir()?;
            let home = single_core::agent_home::ensure_bootstrapped(
                &ctx.dirs.homes_dir(),
                &real_home,
                &agent,
            )?;
            let mut result = single_agent_sdk::provider_sync::sync(
                &agent,
                &home,
                &provider_spec.env_var_name,
                &value,
                dry_run,
            )?;
            result.provider = provider;
            Ok(ResponseData::ProviderSyncResults(vec![result]))
        }
        Request::ProviderSetBillingKey { provider, value } => {
            let store = single_core::secrets::SecretTool;
            single_core::secrets::SecretStore::set(&store, &format!("billing:{provider}"), &value)?;
            Ok(ResponseData::Empty)
        }
        Request::BillingProviderList => {
            let store = single_core::secrets::SecretTool;
            let infos = single_core::billing::builtin_registry()
                .into_iter()
                .map(|p| {
                    let configured = single_core::secrets::SecretStore::get(
                        &store,
                        &format!("billing:{}", p.provider),
                    )
                    .ok()
                    .flatten()
                    .is_some();
                    single_protocol::BillingProviderInfo {
                        provider: p.provider.to_string(),
                        verified: p.verified,
                        admin_key_env_hint: p.admin_key_env_hint.to_string(),
                        admin_key_configured: configured,
                        notes: if p.notes.is_empty() {
                            None
                        } else {
                            Some(p.notes.to_string())
                        },
                    }
                })
                .collect();
            Ok(ResponseData::BillingProviders(infos))
        }
        Request::UsageShow { provider } => usage_summary(ctx, provider),
        Request::UsageRefresh => usage_summary(ctx, None),
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
        Request::PluginList => Ok(ResponseData::Plugins(single_core::plugins::load(
            &ctx.dirs.plugins_registry_file(),
        )?)),
        Request::PluginInspect { name } => {
            let plugin = single_core::plugins::find(&ctx.dirs.plugins_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such plugin: {name}"))?;
            Ok(ResponseData::Plugin(plugin))
        }
        Request::PluginSync {
            name,
            agents,
            dry_run,
        } => {
            let plugin = single_core::plugins::find(&ctx.dirs.plugins_registry_file(), &name)?
                .ok_or_else(|| anyhow::anyhow!("no such plugin: {name}"))?;
            let real_home = integrations::home_dir()?;
            let target_agents: Vec<String> = if agents.is_empty() {
                ctx.registry.iter().map(|a| a.name.clone()).collect()
            } else {
                agents
            };
            let mut results = Vec::new();
            for agent in target_agents {
                let selector = if agent == "opencode" {
                    plugin
                        .opencode_module
                        .clone()
                        .unwrap_or_else(|| plugin.target.clone())
                } else {
                    plugin.target.clone()
                };
                let (applied, detail) =
                    match for_agent_with_custom(&agent, &ctx.dirs.agents_dir(), &ctx.registry) {
                        Some(adapter) if dry_run => (
                            false,
                            format!(
                                "dry run: would run `{} plugin install {selector}`",
                                adapter.command()
                            ),
                        ),
                        Some(adapter) => {
                            let home = single_core::agent_home::ensure_bootstrapped(
                                &ctx.dirs.homes_dir(),
                                &real_home,
                                &agent,
                            )?;
                            match adapter.install_plugin(
                                &selector,
                                &home,
                                std::time::Duration::from_secs(60),
                            ) {
                                Ok(outcome) if outcome.success => (true, "installed".to_string()),
                                Ok(outcome) => (
                                    false,
                                    format!(
                                        "exited with {:?}: {}",
                                        outcome.exit_code, outcome.stderr
                                    ),
                                ),
                                Err(err) => (false, err.to_string()),
                            }
                        }
                        None => (false, format!("no adapter for agent '{agent}'")),
                    };
                results.push(single_protocol::PluginInstallResult {
                    plugin: name.clone(),
                    agent,
                    applied,
                    detail,
                });
            }
            Ok(ResponseData::PluginSyncResults(results))
        }
        Request::FallbackSet { chain } => {
            single_core::fallback::set(&ctx.dirs.fallback_registry_file(), chain)?;
            Ok(ResponseData::Empty)
        }
        Request::FallbackList => Ok(ResponseData::FallbackChains(single_core::fallback::load(&ctx.dirs.fallback_registry_file())?)),
        Request::FallbackRemove { first } => {
            let removed = single_core::fallback::remove(&ctx.dirs.fallback_registry_file(), &first)?;
            anyhow::ensure!(removed, "no fallback chain starts with that entry");
            Ok(ResponseData::Empty)
        }
        Request::TaskHookAdd { on, command, agent, workspace } => {
            single_core::task_hooks::add(&ctx.dirs.task_hooks_registry_file(), single_protocol::TaskHookRule { on, command, agent, workspace })?;
            Ok(ResponseData::Empty)
        }
        Request::TaskHookList => Ok(ResponseData::TaskHooks(single_core::task_hooks::list(&ctx.dirs.task_hooks_registry_file())?)),
        Request::TaskHookRemove { command } => {
            let removed = single_core::task_hooks::remove(&ctx.dirs.task_hooks_registry_file(), &command)?;
            Ok(ResponseData::TaskHookRemoved(removed))
        }
        Request::TaskHookTest { command } => {
            single_core::task_hooks::test(&ctx.dirs.task_hooks_registry_file(), &command)?;
            Ok(ResponseData::Empty)
        }
        Request::PluginPresetList => {
            let presets = single_core::plugins::presets()
                .into_iter()
                .map(|p| single_protocol::PluginPresetInfo {
                    name: p.name.to_string(),
                    target: p.target.to_string(),
                })
                .collect();
            Ok(ResponseData::PluginPresets(presets))
        }
        Request::PluginAddPreset { name } => {
            let preset = single_core::plugins::preset(&name).ok_or_else(|| {
                anyhow::anyhow!("no such preset: {name} (see `single plugin presets`)")
            })?;
            single_core::plugins::add(&ctx.dirs.plugins_registry_file(), preset.to_spec())?;
            Ok(ResponseData::Empty)
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
        Request::KgCreateRelation {
            from,
            to,
            relation_type,
        } => {
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
            let entity = crate::knowledge_graph::get_entity(&conn, &name)?
                .ok_or_else(|| anyhow::anyhow!("no such entity: {name}"))?;
            Ok(ResponseData::KgEntity(entity))
        }
        Request::KgQuery { term } => {
            let conn = kg_db(ctx)?;
            Ok(ResponseData::KgEntities(crate::knowledge_graph::query(
                &conn, &term,
            )?))
        }
        Request::KgReadGraph => {
            let conn = kg_db(ctx)?;
            Ok(ResponseData::KgGraph(crate::knowledge_graph::read_graph(
                &conn,
            )?))
        }
        Request::CacheSet {
            key,
            value,
            ttl_secs,
        } => {
            let url = redis_url()?;
            crate::redis_backend::set(&url, &key, &value, ttl_secs)?;
            Ok(ResponseData::Empty)
        }
        Request::CacheGet { key } => {
            let url = redis_url()?;
            Ok(ResponseData::CacheValue(crate::redis_backend::get(
                &url, &key,
            )?))
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
            Ok(ResponseData::CacheKeys(crate::redis_backend::list_keys(
                &url, &pattern,
            )?))
        }
        Request::CacheStatus => {
            let url = crate::redis_backend::resolve_url();
            let reachable = url
                .as_deref()
                .map(|u| crate::redis_backend::ping(u).is_ok())
                .unwrap_or(false);
            Ok(ResponseData::CacheStatus {
                configured: url.is_some(),
                url,
                reachable,
            })
        }
        Request::VectorUpsert {
            collection,
            id,
            vector,
            payload,
        } => {
            let url = qdrant_url()?;
            crate::qdrant_backend::upsert_point(&url, &collection, id, &vector, payload)?;
            Ok(ResponseData::Empty)
        }
        Request::VectorSearch {
            collection,
            vector,
            limit,
        } => {
            let url = qdrant_url()?;
            Ok(ResponseData::VectorHits(crate::qdrant_backend::search(
                &url,
                &collection,
                &vector,
                limit,
            )?))
        }
        Request::VectorDelete { collection, id } => {
            let url = qdrant_url()?;
            crate::qdrant_backend::delete_point(&url, &collection, id)?;
            Ok(ResponseData::Empty)
        }
        Request::VectorStatus => {
            let url = crate::qdrant_backend::resolve_url();
            let reachable = url
                .as_deref()
                .map(|u| crate::qdrant_backend::ping(u).is_ok())
                .unwrap_or(false);
            Ok(ResponseData::VectorStatus {
                configured: url.is_some(),
                url,
                reachable,
            })
        }
        Request::AgentInstall { name, dry_run } => Ok(ResponseData::AgentInstallResult(
            bootstrap::run_one(ctx, &name, dry_run)?,
        )),
        Request::Setup { dry_run } => Ok(ResponseData::SetupPlan(bootstrap::run(ctx, dry_run))),
        Request::InstallIntegrations { dry_run, real_home } => Ok(ResponseData::IntegrationResult(
            integrations::install_all(ctx, dry_run, real_home)?,
        )),
        Request::UninstallIntegrations { real_home } => Ok(ResponseData::IntegrationResult(
            integrations::uninstall_all(ctx, false, real_home)?,
        )),
        Request::ProfileList => Ok(ResponseData::Profiles(single_core::profile::list_profiles(
            &ctx.dirs,
        )?)),
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

/// Builds the Usage page's full summary: real `$` from every configured
/// billing-supported provider key (best-effort — one provider's API
/// failure doesn't blank out the others, it just gets skipped, since a
/// billing endpoint being briefly unreachable shouldn't hide every other
/// provider's real numbers) plus local-only run stats for every agent
/// that has no billing data at all. `provider_filter` narrows to one
/// provider's keys when set (used by `single usage show --provider`).
fn usage_summary(ctx: &Context, provider_filter: Option<String>) -> anyhow::Result<ResponseData> {
    let store = single_core::secrets::SecretTool;
    let mut provider_usage = Vec::new();

    let providers = single_core::billing::builtin_registry();
    for billing_provider in providers.iter().filter(|p| p.supported) {
        if let Some(filter) = &provider_filter {
            if filter != billing_provider.provider {
                continue;
            }
        }
        // A missing/unreachable keychain (e.g. secret-tool not installed)
        // means "no billing key configured for this provider" here, same
        // as everywhere else this codebase treats a missing optional
        // external capability — not a reason to fail the whole Usage
        // page for every other provider too.
        let Some(admin_key) = single_core::secrets::SecretStore::get(
            &store,
            &format!("billing:{}", billing_provider.provider),
        )
        .ok()
        .flatten() else {
            continue;
        };
        let keys = single_core::provider_keys::list_for_provider(
            &ctx.dirs.provider_keys_registry_file(),
            billing_provider.provider,
        )?;
        // openrouter has no separate admin key — its "admin key" *is* a
        // regular inference key, and usage is scoped to whichever key
        // authenticates the call, so it needs one fetch per labeled key
        // rather than one org-wide fetch like anthropic/openai.
        if billing_provider.provider == "openrouter" && !keys.is_empty() {
            for key in &keys {
                let Some(value) = single_core::secrets::SecretStore::get(&store, &key.secret_name)
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                if let Ok(mut records) =
                    crate::billing::fetch_usage("openrouter", &value, chrono::Utc::now())
                {
                    for record in &mut records {
                        record.key_label = Some(key.label.clone());
                        record.agent = key.agent.clone();
                    }
                    provider_usage.extend(records);
                }
            }
            continue;
        }
        let since = chrono::Utc::now() - chrono::Duration::days(30);
        if let Ok(mut records) =
            crate::billing::fetch_usage(billing_provider.provider, &admin_key, since)
        {
            for record in &mut records {
                if let Some(label) = &record.key_label {
                    record.agent = keys
                        .iter()
                        .find(|k| &k.label == label)
                        .and_then(|k| k.agent.clone());
                }
            }
            provider_usage.extend(records);
        }
    }

    // `Iterator::sum()` on an empty f64 sequence yields -0.0 (a real IEEE
    // 754 quirk, confirmed by direct execution, not assumed), which then
    // prints as the confusing "$-0.0000" — -0.0 == 0.0 is true, so this
    // normalizes only that case to plain positive zero for display,
    // without touching a real (non-zero) total.
    let total_usd: f64 = provider_usage.iter().map(|r| r.cost_usd).sum();
    let total_usd = if total_usd == 0.0 { 0.0 } else { total_usd };
    let conn = task_db(ctx)?;
    let agent_local_stats = crate::task::local_stats_by_agent(&conn)?;

    Ok(ResponseData::Usage(single_protocol::UsageSummary {
        provider_usage,
        agent_local_stats,
        total_usd,
        last_refreshed: Some(chrono::Utc::now().to_rfc3339()),
    }))
}

fn task_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    crate::task::ensure_schema(&conn)?;
    // Also needed here (not just via memory_db/kg_db/notes_db) since
    // task::run's context preamble reads all three best-effort on every
    // run — see build_context_preamble.
    crate::memory::ensure_schema(&conn)?;
    single_core::notes::ensure_schema(&conn)?;
    crate::knowledge_graph::ensure_schema(&conn)?;
    Ok(conn)
}

fn notes_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    single_core::notes::ensure_schema(&conn)?;
    Ok(conn)
}

fn documents_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    crate::documents::ensure_schema(&conn)?;
    memory::ensure_schema(&conn)?;
    Ok(conn)
}

fn to_document_info(doc: crate::documents::DocumentInfo) -> single_protocol::DocumentInfo {
    single_protocol::DocumentInfo {
        id: doc.id,
        title: doc.title,
        project: doc.project,
        source_path: doc.source_path,
        extracted_chars: doc.extracted_chars,
        memory_id: doc.memory_id,
        ingested_at: doc.ingested_at,
    }
}

/// The `single` binary's absolute path, so the hook command written into
/// an isolated home's settings.json works regardless of what `PATH` looks
/// like when Claude Code spawns the hook process — falls back to the bare
/// command name (relying on `PATH`) if `which` can't find it, same
/// resolution style `single-agent-sdk::discover` already uses.
fn resolve_single_binary_path() -> String {
    std::process::Command::new("which")
        .arg("single")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "single".to_string())
}

fn write_settings_with_backup(
    path: &std::path::Path,
    contents: &serde_json::Value,
) -> anyhow::Result<()> {
    if path.exists() {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let backup = path.with_extension(format!("json.bak-{timestamp}"));
        let _ = std::fs::copy(path, &backup);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(contents)?)?;
    Ok(())
}

fn preferences_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    single_core::preferences::ensure_schema(&conn)?;
    Ok(conn)
}

fn to_approval_info(a: single_core::preferences::Approval) -> single_protocol::ApprovalInfo {
    let status = match a.status {
        single_core::preferences::ApprovalStatus::Pending => "pending",
        single_core::preferences::ApprovalStatus::Allowed => "allowed",
        single_core::preferences::ApprovalStatus::Denied => "denied",
    };
    single_protocol::ApprovalInfo {
        id: a.id,
        resource: a.resource,
        context: a.context,
        status: status.to_string(),
        created_at: a.created_at,
        resolved_at: a.resolved_at,
    }
}

fn to_preference_info(p: single_core::preferences::Preference) -> single_protocol::PreferenceInfo {
    let decision = match p.decision {
        single_core::permissions::Decision::Deny => "deny",
        single_core::permissions::Decision::Ask => "ask",
        single_core::permissions::Decision::Allow => "allow",
    };
    single_protocol::PreferenceInfo {
        id: p.id,
        pattern: p.pattern,
        decision: decision.to_string(),
        confidence: p.confidence,
        learned_from: p.learned_from,
        created_at: p.created_at,
    }
}

fn redis_url() -> anyhow::Result<String> {
    crate::redis_backend::resolve_url().ok_or_else(|| {
        anyhow::anyhow!(
            "no Redis configured; set SINGLE_REDIS_URL to enable the working-memory cache"
        )
    })
}

fn qdrant_url() -> anyhow::Result<String> {
    crate::qdrant_backend::resolve_url().ok_or_else(|| {
        anyhow::anyhow!("no Qdrant configured; set SINGLE_QDRANT_URL to enable the vector store")
    })
}

fn kg_db(ctx: &Context) -> anyhow::Result<rusqlite::Connection> {
    let conn = crate::state::open(&ctx.dirs.db_path())?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    crate::knowledge_graph::ensure_schema(&conn)?;
    Ok(conn)
}

fn status(ctx: &Context) -> RuntimeStatus {
    // Parallelized across agents (see `cached_discover`'s doc comment) so a
    // cold cache costs roughly one agent's `which`+`--version` latency, not
    // the sum of all of them; a warm cache costs nothing but HashMap
    // lookups either way.
    let detected = std::thread::scope(|scope| {
        let handles: Vec<_> = ctx
            .registry
            .iter()
            .map(|a| scope.spawn(|| cached_discover(a, ctx).is_some_and(|d| d.detected)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(false))
            .filter(|detected| *detected)
            .count()
    });
    RuntimeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        active_profile: ctx.resolved.active_profile.clone(),
        agents_known: ctx.registry.len(),
        agents_detected: detected,
        socket_path: ctx.dirs.socket_path().display().to_string(),
        db_path: ctx.dirs.db_path().display().to_string(),
    }
}

/// Per-agent discovery result, cached for `DISCOVERY_TTL`: `Discovery`
/// shells out `which` + `<cmd> --version` (two subprocess spawns), and
/// `Context::load()` builds a fresh `Context` on every single request (see
/// `server.rs`'s `handle_connection`) — without this cache, every
/// `Status`/`AgentList` call re-spawns those two processes for every
/// registered agent (15+ agents today), and the TUI fires both requests on
/// every refresh. An install/uninstall of an agent CLI is picked up within
/// one TTL window rather than instantly, same trade-off `daemon restart`
/// already exists for ($PATH itself is only re-read at daemon spawn time).
/// One lock per agent name (rather than one lock over the whole map) so
/// discovering different agents never blocks on each other — but
/// discovering the *same* agent from two concurrent requests (e.g.
/// `Status` and `AgentList`, both parallelizing across every registered
/// agent, fired together by `single-tui`'s `App::refresh`) now serializes
/// on that one agent's lock instead of both redundantly shelling out
/// `which`/`--version` for it at once: the second caller blocks until the
/// first finishes, then sees the just-cached result and returns instantly
/// rather than repeating the work. Without this, a cold TUI launch was
/// paying for up to 2x the subprocess spawns every registered agent needs.
static DISCOVERY_LOCKS: OnceLock<Mutex<HashMap<String, std::sync::Arc<Mutex<Option<(Discovery, Instant)>>>>>> = OnceLock::new();
const DISCOVERY_TTL: Duration = Duration::from_secs(45);

fn cached_discover(def: &AgentDefinition, ctx: &Context) -> Option<Discovery> {
    let locks = DISCOVERY_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let slot = locks.lock().unwrap().entry(def.name.clone()).or_default().clone();

    let mut slot = slot.lock().unwrap();
    if let Some((discovery, at)) = slot.as_ref() {
        if at.elapsed() < DISCOVERY_TTL {
            return Some(discovery.clone());
        }
    }
    let discovery = for_agent_with_custom(&def.name, &ctx.dirs.agents_dir(), &ctx.registry)?.discover();
    *slot = Some((discovery.clone(), Instant::now()));
    Some(discovery)
}

fn to_agent_info(def: &AgentDefinition, ctx: &Context) -> AgentInfo {
    let discovery = cached_discover(def, ctx);
    let isolated_home = ctx.dirs.homes_dir().join(&def.name);
    let authenticated = single_core::account::is_authenticated(&isolated_home, &def.name);
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
        authenticated,
    }
}
