use crate::context::Context;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::{AuthState, CheckStatus, DoctorCheck, DoctorReport};

pub fn run(ctx: &Context) -> DoctorReport {
    let mut checks = Vec::new();
    let real_home = crate::integrations::home_dir().ok();

    checks.push(DoctorCheck {
        name: "config directory".into(),
        status: CheckStatus::Ok,
        detail: ctx.dirs.root().display().to_string(),
    });

    match std::fs::metadata(ctx.dirs.root()) {
        Ok(meta) if meta.permissions().readonly() => checks.push(DoctorCheck {
            name: "config directory writable".into(),
            status: CheckStatus::Fail,
            detail: "directory is read-only".into(),
        }),
        Ok(_) => checks.push(DoctorCheck {
            name: "config directory writable".into(),
            status: CheckStatus::Ok,
            detail: "writable".into(),
        }),
        Err(e) => checks.push(DoctorCheck {
            name: "config directory writable".into(),
            status: CheckStatus::Fail,
            detail: e.to_string(),
        }),
    }

    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir()) else {
            checks.push(DoctorCheck {
                name: format!("agent: {}", agent.name),
                status: CheckStatus::Skipped,
                detail: "no adapter registered".into(),
            });
            continue;
        };
        let discovery = adapter.discover();
        let status = if discovery.detected {
            CheckStatus::Ok
        } else if agent.bootstrap_install.is_some() {
            CheckStatus::Warn
        } else {
            CheckStatus::Skipped
        };
        let detail = if discovery.detected {
            format!(
                "detected at {}{}",
                discovery.resolved_path.as_deref().unwrap_or("?"),
                discovery
                    .version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default()
            )
        } else if let Some(install) = &agent.bootstrap_install {
            format!("not installed; `single setup` would run: {}", install.command)
        } else {
            "not installed; no verified install method".into()
        };
        checks.push(DoctorCheck { name: format!("agent: {}", agent.name), status, detail });

        if discovery.detected {
            if let Some(real_home) = &real_home {
                let isolated_home = ctx.dirs.homes_dir().join(&agent.name);
                let auth = single_core::account::is_authenticated(&isolated_home, real_home, &agent.name);
                let check = match auth {
                    AuthState::Authenticated => DoctorCheck {
                        name: format!("agent: {} auth", agent.name),
                        status: CheckStatus::Ok,
                        detail: "logged in".into(),
                    },
                    AuthState::NotAuthenticated => {
                        let real_only = single_core::account::has_live_login(real_home, &agent.name)
                            && !single_core::account::has_live_login(&isolated_home, &agent.name);
                        if real_only {
                            DoctorCheck {
                                name: format!("agent: {} auth", agent.name),
                                status: CheckStatus::Warn,
                                detail: format!(
                                    "logged in on this machine's real home but SingleCLI's isolated home has no credentials yet — run `single account capture {a} <name>` to fix, or `single agent login {a}` again",
                                    a = agent.name
                                ),
                            }
                        } else {
                            DoctorCheck {
                                name: format!("agent: {} auth", agent.name),
                                status: CheckStatus::Skipped,
                                detail: "not logged in".into(),
                            }
                        }
                    }
                    AuthState::Unsupported => DoctorCheck {
                        name: format!("agent: {} auth", agent.name),
                        status: CheckStatus::Skipped,
                        detail: "account/auth detection not supported for this agent".into(),
                    },
                };
                checks.push(check);
            }
        }
    }

    DoctorReport { checks }
}
