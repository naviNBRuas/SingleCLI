mod client;
mod daemon;
mod render;

use clap::{Parser, Subcommand};
use single_core::SingleDirs;
use single_protocol::Request;

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
