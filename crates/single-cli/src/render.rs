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

fn install_summary(method: &InstallMethod) -> String {
    match method {
        InstallMethod::Native { detail } => format!("native ({detail})"),
        InstallMethod::StandaloneBinary { detail } => format!("standalone ({detail})"),
        InstallMethod::PackageManager { detail } => format!("package manager ({detail})"),
        InstallMethod::Unsupported { reason } => format!("unsupported ({reason})"),
    }
}
