use single_protocol::{CheckStatus, InstallMethod, Response, ResponseData};

/// Prints a `Response`, either as pretty text or as JSON (`single ... --json`),
/// and exits non-zero on `Response::Error` so shell scripting behaves.
pub fn print(response: Response, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(&response).unwrap());
        if matches!(response, Response::Error { .. }) {
            std::process::exit(1);
        }
        return;
    }

    match response {
        Response::Error { message } => {
            eprintln!("error: {message}");
            std::process::exit(1);
        }
        Response::Ok { data } => print_data(data),
    }
}

fn print_data(data: ResponseData) {
    match data {
        ResponseData::Status(s) => {
            println!("SingleCLI runtime v{}", s.version);
            println!("  profile:  {}", s.active_profile);
            println!("  agents:   {}/{} detected", s.agents_detected, s.agents_known);
            println!("  socket:   {}", s.socket_path);
            println!("  db:       {}", s.db_path);
        }
        ResponseData::Doctor(report) => {
            for check in report.checks {
                let icon = match check.status {
                    CheckStatus::Ok => "✓",
                    CheckStatus::Warn => "!",
                    CheckStatus::Fail => "✗",
                    CheckStatus::Skipped => "-",
                };
                println!("[{icon}] {}: {}", check.name, check.detail);
            }
        }
        ResponseData::Agents(agents) => {
            for agent in agents {
                let dot = if agent.detected { "●" } else { "○" };
                let version = agent.version.as_deref().unwrap_or("-");
                let flag = if agent.unverified { " (unverified)" } else { "" };
                println!("{dot} {:<12} {:<20} {}{flag}", agent.name, version, install_summary(&agent.install_method));
            }
        }
        ResponseData::Agent(agent) => {
            println!("{}", agent.name);
            println!("  adapter:      {}", agent.adapter);
            println!("  command:      {}", agent.command);
            println!("  detected:     {}", agent.detected);
            println!("  version:      {}", agent.version.as_deref().unwrap_or("-"));
            println!("  install:      {}", install_summary(&agent.install_method));
            if let Some(install) = &agent.bootstrap_install {
                println!("  bootstrap:    {} (source: {})", install.command, install.source);
            }
            println!(
                "  capabilities: mcp={} lsp={} tools={} sessions={} streaming={} structured_output={}",
                agent.capabilities.mcp,
                agent.capabilities.lsp,
                agent.capabilities.tools,
                agent.capabilities.sessions,
                agent.capabilities.streaming,
                agent.capabilities.structured_output
            );
            if !agent.config_paths.is_empty() {
                println!("  config:       {}", agent.config_paths.join(", "));
            }
            if let Some(notes) = &agent.notes {
                println!("  notes:        {notes}");
            }
        }
        ResponseData::McpServers(servers) => {
            if servers.is_empty() {
                println!("(no mcp servers registered)");
            }
            for server in servers {
                let flag = if server.enabled { "enabled" } else { "disabled" };
                println!("{:<12} {:<40} [{flag}]", server.name, server.command);
            }
        }
        ResponseData::McpServer(server) => {
            println!("{}", server.name);
            println!("  command: {} {}", server.command, server.args.join(" "));
            println!("  enabled: {}", server.enabled);
            if !server.env.is_empty() {
                println!("  env:     {}", server.env.keys().cloned().collect::<Vec<_>>().join(", "));
            }
        }
        ResponseData::LspServers(servers) => {
            if servers.is_empty() {
                println!("(no lsp servers registered)");
            }
            for server in servers {
                let flag = if server.enabled { "enabled" } else { "disabled" };
                println!("{:<12} {:<30} {} [{flag}]", server.name, server.command, server.extensions.join(","));
            }
        }
        ResponseData::LspServer(server) => {
            println!("{}", server.name);
            println!("  command:    {} {}", server.command, server.args.join(" "));
            println!("  extensions: {}", server.extensions.join(", "));
            println!("  enabled:    {}", server.enabled);
        }
        ResponseData::Tools(tools) => {
            if tools.is_empty() {
                println!("(no tools registered)");
            }
            for tool in tools {
                let flag = if tool.enabled { "enabled" } else { "disabled" };
                println!("{:<12} {:?} {:<40} [{flag}]", tool.name, tool.risk_level, tool.description);
            }
        }
        ResponseData::Tool(tool) => {
            println!("{}", tool.name);
            println!("  description: {}", tool.description);
            println!("  risk:        {:?}", tool.risk_level);
            println!("  enabled:     {}", tool.enabled);
        }
        ResponseData::SecretNames(names) => {
            if names.is_empty() {
                println!("(no secrets stored)");
            }
            for name in names {
                println!("{name}");
            }
        }
        ResponseData::SecretValue(value) => match value {
            Some(v) => println!("{v}"),
            None => {
                eprintln!("(no such secret)");
                std::process::exit(1);
            }
        },
        ResponseData::Skills(skills) => {
            if skills.is_empty() {
                println!("(no skills installed)");
            }
            for skill in skills {
                println!("{skill}");
            }
        }
        ResponseData::SkillContents(entries) => {
            for entry in entries {
                println!("{entry}");
            }
        }
        ResponseData::MemoryId(id) => println!("stored as #{id}"),
        ResponseData::MemoryEntry(entry) => print_memory_entry(&entry),
        ResponseData::MemoryEntries(entries) => {
            if entries.is_empty() {
                println!("(no matching memories)");
            }
            for entry in entries {
                println!(
                    "#{:<5} {:<10} {:<18} {}",
                    entry.id,
                    format!("{:?}", entry.scope).to_lowercase(),
                    entry.created_at,
                    entry.title
                );
            }
        }
        ResponseData::Context(ctx) => {
            println!("cwd:       {}", ctx.cwd);
            println!("repo root: {}", ctx.repo_root.as_deref().unwrap_or("(not a git repo)"));
            println!("branch:    {}", ctx.branch.as_deref().unwrap_or("-"));
            println!("changed:   {}", if ctx.changed_files.is_empty() { "(clean)".to_string() } else { ctx.changed_files.join(", ") });
            println!("docs:      {}", if ctx.project_docs.is_empty() { "(none found)".to_string() } else { ctx.project_docs.join(", ") });
        }
        ResponseData::Task(task) => print_task(&task),
        ResponseData::Tasks(tasks) => {
            if tasks.is_empty() {
                println!("(no tasks)");
            }
            for task in tasks {
                println!(
                    "#{:<5} {:<10} {:<10} {:<18} {}",
                    task.id,
                    format!("{:?}", task.status).to_lowercase(),
                    task.agent,
                    task.created_at,
                    task.description
                );
            }
        }
        ResponseData::AccountProfile(info) => print_account_profile(&info),
        ResponseData::AccountProfiles(profiles) => {
            if profiles.is_empty() {
                println!("(no captured account profiles)");
            }
            for p in profiles {
                let flag = if p.unverified_complete { " (best-effort — not confirmed complete)" } else { "" };
                println!("{:<10} {:<16} {}{flag}", p.agent, p.name, p.captured_at);
            }
        }
        ResponseData::AccountSwitched(result) => {
            println!("switched {} to profile '{}'", result.agent, result.name);
            for backup in result.backed_up {
                println!("  backup: {backup}");
            }
        }
        ResponseData::Provider(p) => print_provider(&p),
        ResponseData::Providers(providers) => {
            if providers.is_empty() {
                println!("(no providers registered)");
            }
            for p in providers {
                println!("{:<16} {:<20} {}", p.name, p.env_var_name, p.base_url.as_deref().unwrap_or("-"));
            }
        }
        ResponseData::ProviderSyncResults(results) => {
            for r in results {
                let mark = if r.applied { "✓" } else { "·" };
                println!("  {mark} {:<12} {} — {}", r.agent, r.config_path, r.detail);
                if let Some(backup) = r.backup_path {
                    println!("      backup: {backup}");
                }
            }
        }
        ResponseData::KgEntityId(id) => println!("#{id}"),
        ResponseData::KgEntity(e) => print_kg_entity(&e),
        ResponseData::KgEntities(entities) => {
            if entities.is_empty() {
                println!("(no matching entities)");
            }
            for e in entities {
                println!("{:<20} {:<14} {} observation(s)", e.name, e.entity_type, e.observations.len());
            }
        }
        ResponseData::KgGraph(graph) => {
            println!("Entities:");
            for e in &graph.entities {
                println!("  {:<20} {}", e.name, e.entity_type);
                for o in &e.observations {
                    println!("    - {o}");
                }
            }
            println!("Relations:");
            for r in &graph.relations {
                println!("  {} --[{}]--> {}", r.from_entity, r.relation_type, r.to_entity);
            }
        }
        ResponseData::SetupPlan(plan) => {
            let mode = if plan.dry_run { "dry run" } else { "applied" };
            println!("Setup plan ({mode}):");
            for action in plan.actions {
                let mark = if action.executed { "✓" } else { "·" };
                println!("  {mark} {:<12} {:?}: {}", action.agent, action.action, action.detail);
            }
        }
        ResponseData::IntegrationResult(result) => {
            let mode = if result.dry_run { "dry run" } else { "applied" };
            println!("Integrations ({mode}):");
            for write in result.writes {
                let mark = if write.applied { "✓" } else { "·" };
                println!("  {mark} {:<12} {} — {}", write.agent, write.config_path, write.detail);
                if let Some(backup) = write.backup_path {
                    println!("      backup: {backup}");
                }
            }
        }
        ResponseData::Profiles(profiles) => {
            if profiles.is_empty() {
                println!("(no profiles defined yet — using defaults)");
            }
            for profile in profiles {
                println!("{profile}");
            }
        }
        ResponseData::Empty => {}
    }
}

fn print_memory_entry(entry: &single_protocol::MemoryEntry) {
    println!("#{}", entry.id);
    println!("  scope:      {:?}", entry.scope);
    println!("  source:     {:?}", entry.source);
    println!("  title:      {}", entry.title);
    println!("  content:    {}", entry.content);
    println!("  confidence: {}", entry.confidence);
    println!("  created:    {}", entry.created_at);
    if let Some(project) = &entry.project {
        println!("  project:    {project}");
    }
    if let Some(agent) = &entry.agent {
        println!("  agent:      {agent}");
    }
    if let Some(task) = &entry.task {
        println!("  task:       {task}");
    }
    if let Some(expires) = &entry.expires_at {
        println!("  expires:    {expires}");
    }
}

fn print_task(task: &single_protocol::TaskRecord) {
    println!("#{}", task.id);
    println!("  status:      {:?}", task.status);
    println!("  agent:       {}", task.agent);
    println!("  description: {}", task.description);
    if let Some(worktree) = &task.worktree_path {
        println!("  worktree:    {worktree}");
    }
    if let Some(artifact) = &task.artifact_path {
        println!("  artifact:    {artifact}");
    }
    if let Some(code) = task.exit_code {
        println!("  exit code:   {code}");
    }
    if task.timed_out {
        println!("  timed out:   true");
    }
    if let Some(summary) = &task.summary {
        println!("  summary:     {summary}");
    }
    println!("  created:     {}", task.created_at);
    println!("  updated:     {}", task.updated_at);
}

fn print_account_profile(info: &single_protocol::AccountProfileInfo) {
    println!("{}/{}", info.agent, info.name);
    println!("  captured: {}", info.captured_at);
    if info.unverified_complete {
        println!("  note:     best-effort capture — not confirmed to cover 100% of this agent's login state");
    }
}

fn print_provider(p: &single_protocol::ProviderSpec) {
    println!("{}", p.name);
    println!("  env var:    {}", p.env_var_name);
    println!("  secret:     {} (use `single secret get {}` to check, `single provider set-key` to change)", p.secret_name, p.secret_name);
    if let Some(url) = &p.base_url {
        println!("  base url:   {url}");
    }
}

fn print_kg_entity(e: &single_protocol::KgEntity) {
    println!("{}", e.name);
    println!("  type:    {}", e.entity_type);
    println!("  created: {}", e.created_at);
    for o in &e.observations {
        println!("  - {o}");
    }
}

fn install_summary(method: &InstallMethod) -> String {
    match method {
        InstallMethod::Native { detail } => format!("native ({detail})"),
        InstallMethod::StandaloneBinary { detail } => format!("standalone ({detail})"),
        InstallMethod::PackageManager { detail } => format!("package manager ({detail})"),
        InstallMethod::Unsupported { reason } => format!("unsupported ({reason})"),
    }
}
