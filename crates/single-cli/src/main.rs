mod client;
mod daemon;
mod render;

use clap::{Parser, Subcommand};
use single_core::SingleDirs;
use single_protocol::{LspServerSpec, McpServerSpec, Request, RiskLevel, ToolSpec};
use std::collections::BTreeMap;

#[derive(Parser)]
#[command(name = "single", about = "SingleCLI — unified control plane for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show runtime status.
    Status,
    /// Diagnose installed agent CLIs, config, and runtime health.
    Doctor,
    /// Install missing agent CLIs and sync SingleCLI's config into all of them.
    Setup {
        /// Actually run install commands and write config. Without this, only shows the plan.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Manage the agent registry.
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
    /// Manage the unified MCP registry.
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
    /// Manage the unified LSP registry.
    Lsp {
        #[command(subcommand)]
        action: LspCommand,
    },
    /// Manage the tool registry (metadata only — no execution engine yet).
    Tool {
        #[command(subcommand)]
        action: ToolCommand,
    },
    /// Manage secrets (OS keychain-backed; Linux via secret-tool in Phase 2).
    Secret {
        #[command(subcommand)]
        action: SecretCommand,
    },
    /// Manage skills (local directories under ~/.config/single/skills).
    Skill {
        #[command(subcommand)]
        action: SkillCommand,
    },
    /// Manage profiles.
    Profile {
        #[command(subcommand)]
        action: ProfileCommand,
    },
    /// Sync SingleCLI's MCP registry into every agent's native config.
    InstallIntegrations {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove SingleCLI-managed entries from every agent's native config.
    UninstallIntegrations {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum AgentCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        command: String,
        /// Extra arguments passed to `command`, in order (may start with `-`, e.g. `-y`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Remove {
        name: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum LspCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        command: String,
        /// File extensions this server handles, e.g. .rs .toml
        #[arg(long, value_delimiter = ' ')]
        extensions: Vec<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Remove {
        name: String,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ToolCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        name: String,
        description: String,
        #[arg(long, value_enum, default_value = "medium")]
        risk: RiskArg,
    },
    Inspect {
        name: String,
        #[arg(long)]
        json: bool,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum RiskArg {
    Low,
    Medium,
    High,
}

#[derive(Subcommand)]
enum SecretCommand {
    List,
    Set { name: String, value: String },
    Get { name: String },
    Delete { name: String },
}

#[derive(Subcommand)]
enum SkillCommand {
    List,
    Install { name: String, source_path: String },
    Remove { name: String },
    Inspect { name: String },
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Use { name: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let dirs = SingleDirs::discover()?;
    dirs.ensure_created()?;
    let socket_path = dirs.socket_path();

    let Some(command) = cli.command else {
        daemon::ensure_running(&dirs)?;
        return single_tui::run(&socket_path);
    };

    match command {
        Command::Status => {
            let response = client::send(&socket_path, Request::Status)?;
            render::print(response, false);
        }
        Command::Doctor => {
            let response = client::send(&socket_path, Request::Doctor)?;
            render::print(response, false);
        }
        Command::Setup { yes, json } => {
            if !yes {
                eprintln!("Dry run (pass --yes to actually install and configure). This may run vendor install scripts over the network.");
            }
            let response = client::send(&socket_path, Request::Setup { dry_run: !yes })?;
            render::print(response, json);
        }
        Command::Agent { action } => match action {
            AgentCommand::List { json } => {
                let response = client::send(&socket_path, Request::AgentList)?;
                render::print(response, json);
            }
            AgentCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::AgentInspect { name })?;
                render::print(response, json);
            }
        },
        Command::Mcp { action } => match action {
            McpCommand::List { json } => {
                let response = client::send(&socket_path, Request::McpList)?;
                render::print(response, json);
            }
            McpCommand::Add { name, command, args } => {
                let server = McpServerSpec { name, command, args, env: BTreeMap::new(), enabled: true };
                let response = client::send(&socket_path, Request::McpAdd { server })?;
                render::print(response, false);
            }
            McpCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::McpRemove { name })?;
                render::print(response, false);
            }
            McpCommand::Enable { name } => {
                let response = client::send(&socket_path, Request::McpEnable { name })?;
                render::print(response, false);
            }
            McpCommand::Disable { name } => {
                let response = client::send(&socket_path, Request::McpDisable { name })?;
                render::print(response, false);
            }
            McpCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::McpInspect { name })?;
                render::print(response, json);
            }
        },
        Command::Lsp { action } => match action {
            LspCommand::List { json } => {
                let response = client::send(&socket_path, Request::LspList)?;
                render::print(response, json);
            }
            LspCommand::Add { name, command, extensions, args } => {
                let server = LspServerSpec { name, command, args, extensions, enabled: true };
                let response = client::send(&socket_path, Request::LspAdd { server })?;
                render::print(response, false);
            }
            LspCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::LspRemove { name })?;
                render::print(response, false);
            }
            LspCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::LspInspect { name })?;
                render::print(response, json);
            }
        },
        Command::Tool { action } => match action {
            ToolCommand::List { json } => {
                let response = client::send(&socket_path, Request::ToolList)?;
                render::print(response, json);
            }
            ToolCommand::Add { name, description, risk } => {
                let risk_level = match risk {
                    RiskArg::Low => RiskLevel::Low,
                    RiskArg::Medium => RiskLevel::Medium,
                    RiskArg::High => RiskLevel::High,
                };
                let tool = ToolSpec { name, description, risk_level, enabled: true };
                let response = client::send(&socket_path, Request::ToolAdd { tool })?;
                render::print(response, false);
            }
            ToolCommand::Inspect { name, json } => {
                let response = client::send(&socket_path, Request::ToolInspect { name })?;
                render::print(response, json);
            }
            ToolCommand::Enable { name } => {
                let response = client::send(&socket_path, Request::ToolEnable { name })?;
                render::print(response, false);
            }
            ToolCommand::Disable { name } => {
                let response = client::send(&socket_path, Request::ToolDisable { name })?;
                render::print(response, false);
            }
        },
        Command::Secret { action } => match action {
            SecretCommand::List => {
                let response = client::send(&socket_path, Request::SecretList)?;
                render::print(response, false);
            }
            SecretCommand::Set { name, value } => {
                let response = client::send(&socket_path, Request::SecretSet { name, value })?;
                render::print(response, false);
            }
            SecretCommand::Get { name } => {
                let response = client::send(&socket_path, Request::SecretGet { name })?;
                render::print(response, false);
            }
            SecretCommand::Delete { name } => {
                let response = client::send(&socket_path, Request::SecretDelete { name })?;
                render::print(response, false);
            }
        },
        Command::Skill { action } => match action {
            SkillCommand::List => {
                let response = client::send(&socket_path, Request::SkillList)?;
                render::print(response, false);
            }
            SkillCommand::Install { name, source_path } => {
                let response = client::send(&socket_path, Request::SkillInstall { name, source_path })?;
                render::print(response, false);
            }
            SkillCommand::Remove { name } => {
                let response = client::send(&socket_path, Request::SkillRemove { name })?;
                render::print(response, false);
            }
            SkillCommand::Inspect { name } => {
                let response = client::send(&socket_path, Request::SkillInspect { name })?;
                render::print(response, false);
            }
        },
        Command::Profile { action } => match action {
            ProfileCommand::List => {
                let response = client::send(&socket_path, Request::ProfileList)?;
                render::print(response, false);
            }
            ProfileCommand::Use { name } => {
                let response = client::send(&socket_path, Request::ProfileUse { name })?;
                render::print(response, false);
            }
        },
        Command::InstallIntegrations { yes, json } => {
            if !yes {
                eprintln!("Dry run (pass --yes to actually write config files; backups are made either way).");
            }
            let response = client::send(&socket_path, Request::InstallIntegrations { dry_run: !yes })?;
            render::print(response, json);
        }
        Command::UninstallIntegrations { yes } => {
            if !yes {
                anyhow::bail!("this removes SingleCLI-managed MCP entries from every agent's config; pass --yes to confirm");
            }
            let response = client::send(&socket_path, Request::UninstallIntegrations)?;
            render::print(response, false);
        }
    }

    Ok(())
}
