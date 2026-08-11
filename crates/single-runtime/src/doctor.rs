use crate::context::Context;
use single_agent_sdk::adapters::for_agent;
use single_protocol::{CheckStatus, DoctorCheck, DoctorReport};

pub fn run(ctx: &Context) -> DoctorReport {
    let mut checks = Vec::new();

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
        let Some(adapter) = for_agent(&agent.name) else {
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
    }

    DoctorReport { checks }
}
