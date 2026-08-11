//! `single setup`: install any registry agent that isn't detected, using
//! its verified bootstrap install command, then hand off to
//! `integrations::apply_all` to sync SingleCLI's MCP registry into every
//! agent (freshly installed or pre-existing).
//!
//! Running an install script is not easily reversible, so this module never
//! executes anything unless explicitly told `dry_run: false` — callers
//! (the CLI) are expected to require an explicit `--yes`/confirmation
//! before doing that. See docs/architecture.md.

use crate::context::Context;
use single_agent_sdk::adapters::for_agent;
use single_protocol::{SetupAction, SetupActionKind, SetupPlan};
use std::process::Command;

pub fn run(ctx: &Context, dry_run: bool) -> SetupPlan {
    let mut actions = Vec::new();

    for agent in &ctx.registry {
        let Some(adapter) = for_agent(&agent.name) else { continue };
        let discovery = adapter.discover();

        if discovery.detected {
            actions.push(SetupAction {
                agent: agent.name.clone(),
                action: SetupActionKind::AlreadyInstalled,
                detail: discovery
                    .version
                    .clone()
                    .unwrap_or_else(|| "installed (version unknown)".into()),
                executed: false,
            });
            continue;
        }

        let Some(install) = &agent.bootstrap_install else {
            actions.push(SetupAction {
                agent: agent.name.clone(),
                action: SetupActionKind::Unsupported,
                detail: match &agent.install_method {
                    single_protocol::InstallMethod::Unsupported { reason } => reason.clone(),
                    _ => "no verified bootstrap install command".into(),
                },
                executed: false,
            });
            continue;
        };

        if dry_run {
            actions.push(SetupAction {
                agent: agent.name.clone(),
                action: SetupActionKind::Install,
                detail: format!("{} (source: {})", install.command, install.source),
                executed: false,
            });
            continue;
        }

        let result = Command::new("sh").arg("-c").arg(&install.command).status();
        let (executed, detail) = match result {
            Ok(status) if status.success() => (true, format!("ran: {}", install.command)),
            Ok(status) => (false, format!("install command exited with {status}: {}", install.command)),
            Err(e) => (false, format!("failed to run install command: {e}")),
        };
        actions.push(SetupAction { agent: agent.name.clone(), action: SetupActionKind::Install, detail, executed });
    }

    SetupPlan { actions, dry_run }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use single_core::SingleDirs;

    /// Only ever exercises `dry_run: true` — a test that ran `dry_run:
    /// false` would actually shell out and try to install real software,
    /// which is exactly what spec section 34/section 45 says never to do
    /// from an automated test.
    fn test_context() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let dirs = SingleDirs::from_root(dir.path().to_path_buf());
        dirs.ensure_created().unwrap();
        let resolved = single_core::ResolvedConfig::default();
        let registry = single_core::builtin_registry();
        (dir, Context { dirs, resolved, registry })
    }

    #[test]
    fn dry_run_never_executes_anything() {
        let (_dir, ctx) = test_context();
        let plan = run(&ctx, true);
        assert!(plan.dry_run);
        assert!(plan.actions.iter().all(|a| !a.executed), "dry run must never execute");
    }

    #[test]
    fn perplexity_is_never_marked_already_installed_on_a_clean_machine() {
        // pplx is genuinely unlikely to be on a fresh test machine; this
        // pins the expectation that an unrecognized agent falls through to
        // Install (dry run) rather than silently skipping it.
        let (_dir, ctx) = test_context();
        let plan = run(&ctx, true);
        let perplexity = plan.actions.iter().find(|a| a.agent == "perplexity").unwrap();
        assert_ne!(perplexity.action, SetupActionKind::AlreadyInstalled);
    }

    #[test]
    fn every_action_has_a_recognized_agent() {
        let (_dir, ctx) = test_context();
        let plan = run(&ctx, true);
        let names: Vec<_> = plan.actions.iter().map(|a| a.agent.as_str()).collect();
        assert_eq!(names, ["claude", "codex", "opencode", "agy", "perplexity"]);
    }
}
