use crate::context::Context;
use single_agent_sdk::adapters::for_agent_with_custom;
use single_protocol::{AuthState, CheckStatus, DoctorCheck, DoctorReport};

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
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else {
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
            let isolated_home = ctx.dirs.homes_dir().join(&agent.name);
            let auth = single_core::account::is_authenticated(&isolated_home, &agent.name);
            let check = match auth {
                AuthState::Authenticated => DoctorCheck {
                    name: format!("agent: {} auth", agent.name),
                    status: CheckStatus::Ok,
                    detail: "logged in".into(),
                },
                AuthState::NotAuthenticated => DoctorCheck {
                    name: format!("agent: {} auth", agent.name),
                    status: CheckStatus::Skipped,
                    detail: if adapter.login_supported() {
                        format!("not logged in — run `single agent login {}`", agent.name)
                    } else {
                        // `single agent login` would just error here — this
                        // agent's auth state is detectable (support() says
                        // so) but no real login command is wired up for it
                        // (e.g. agy: no auth/login subcommand exists at
                        // all in `agy --help`). Don't point at a command
                        // that doesn't work.
                        format!("not logged in — no `single agent login {}` support yet; run {}'s own login/auth command directly", agent.name, agent.name)
                    },
                },
                AuthState::Unsupported => DoctorCheck {
                    name: format!("agent: {} auth", agent.name),
                    status: CheckStatus::Skipped,
                    detail: "account/auth detection not supported for this agent".into(),
                },
            };
            checks.push(check);
        }
    }

    for (binary, purpose) in [
        ("pdftotext", "PDF text extraction for `single doc ingest` (poppler-utils)"),
        ("pdftoppm", "scanned-PDF rasterization for `single doc ingest` (poppler-utils)"),
        ("tesseract", "OCR for scanned PDFs/images for `single doc ingest`"),
    ] {
        let present = std::process::Command::new("which").arg(binary).output().map(|o| o.status.success()).unwrap_or(false);
        checks.push(DoctorCheck {
            name: format!("tool: {binary}"),
            status: if present { CheckStatus::Ok } else { CheckStatus::Skipped },
            detail: if present { "found".into() } else { format!("not installed — needed for {purpose}") },
        });
    }

    let docker_present = crate::docker::docker_available();
    checks.push(DoctorCheck {
        name: "tool: docker".into(),
        status: if docker_present { CheckStatus::Ok } else { CheckStatus::Skipped },
        detail: if docker_present {
            "found".into()
        } else {
            "not installed — needed only for agents/accounts with `single agent docker enable`".into()
        },
    });
    if docker_present {
        for setting in single_core::docker::status(&ctx.dirs.docker_registry_file(), None).unwrap_or_default() {
            if !setting.enabled {
                continue;
            }
            let container = single_core::docker::container_name(&setting.agent, setting.account.as_deref());
            let label = match &setting.account {
                Some(a) => format!("{}/{a}", setting.agent),
                None => setting.agent.clone(),
            };
            let (status, detail) = match crate::docker::is_running(&container) {
                Ok(Some(true)) => (CheckStatus::Ok, format!("container {container} running")),
                Ok(Some(false)) => (CheckStatus::Warn, format!("container {container} exists but is stopped")),
                Ok(None) => (CheckStatus::Warn, format!("container {container} not created yet (starts on next task run)")),
                Err(e) => (CheckStatus::Warn, format!("could not check container {container}: {e:#}")),
            };
            checks.push(DoctorCheck { name: format!("docker: {label}"), status, detail });
        }
    }

    match crate::qdrant_backend::resolve_url() {
        Some(url) => {
            let reachable = crate::qdrant_backend::ping(&url).is_ok();
            checks.push(DoctorCheck {
                name: "vector store (qdrant)".into(),
                status: if reachable { CheckStatus::Ok } else { CheckStatus::Warn },
                detail: if reachable { format!("{url} (reachable)") } else { format!("{url} (configured but unreachable)") },
            });
        }
        None => checks.push(DoctorCheck {
            name: "vector store (qdrant)".into(),
            status: CheckStatus::Skipped,
            detail: "not configured — set SINGLE_QDRANT_URL to enable `single memory search --semantic`".into(),
        }),
    }
    let embeddings_configured = crate::embeddings::is_configured();
    checks.push(DoctorCheck {
        name: "semantic memory search (embeddings)".into(),
        status: if embeddings_configured { CheckStatus::Ok } else { CheckStatus::Skipped },
        detail: if embeddings_configured {
            "configured".into()
        } else {
            "not configured — `single secret set embeddings:api_key <key>`; semantic search falls back to substring search until then".into()
        },
    });

    DoctorReport { checks }
}
