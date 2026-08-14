//! Canonical SingleCLI directory layout, per spec section 7.
//!
//! `~/.config/single/` holds config, profiles, agent registry overrides, and
//! runtime state. Only the subdirectories Phase 1 actually reads/writes are
//! created eagerly; the rest of the documented layout (mcp/, lsp/, skills/,
//! ...) is created on first use by later phases.

use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SingleDirs {
    root: PathBuf,
}

impl SingleDirs {
    /// Resolves `~/.config/single` (or `$SINGLE_CONFIG_DIR` override, used by tests
    /// and by anyone who wants an isolated instance).
    pub fn discover() -> Result<Self> {
        if let Ok(dir) = std::env::var("SINGLE_CONFIG_DIR") {
            return Ok(Self { root: PathBuf::from(dir) });
        }
        let base = directories::BaseDirs::new()
            .context("could not determine home/config directory for this platform")?;
        Ok(Self { root: base.config_dir().join("single") })
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.root.join("agents")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.state_dir().join("runtime.sock")
    }

    pub fn db_path(&self) -> PathBuf {
        self.state_dir().join("single.db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn mcp_registry_file(&self) -> PathBuf {
        self.root.join("mcp.toml")
    }

    /// Whether `single install-integrations` should sync only the
    /// `single-mcp` dynamic gateway into agents' native MCP config,
    /// instead of every enabled server in `mcp_registry_file()` — see
    /// `single_core::mcp::gateway_mode`.
    pub fn mcp_gateway_file(&self) -> PathBuf {
        self.root.join("mcp_gateway.toml")
    }

    pub fn lsp_registry_file(&self) -> PathBuf {
        self.root.join("lsp.toml")
    }

    pub fn tools_registry_file(&self) -> PathBuf {
        self.root.join("tools.toml")
    }

    pub fn permissions_file(&self) -> PathBuf {
        self.root.join("permissions.toml")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// Where account-switch profile snapshots live. Deliberately outside
    /// `state/` (which Phase 1 documented as cache/derived data) since
    /// these can contain live credentials and are user-authored, not
    /// regenerable.
    pub fn accounts_dir(&self) -> PathBuf {
        self.root.join("accounts")
    }

    pub fn providers_registry_file(&self) -> PathBuf {
        self.root.join("providers.toml")
    }

    pub fn plugins_registry_file(&self) -> PathBuf {
        self.root.join("plugins.toml")
    }

    /// Root of every agent's SingleCLI-managed `$HOME` (see
    /// `single-core::agent_home`). SingleCLI runs agent CLIs, syncs their
    /// config, and captures/switches their credentials against
    /// `homes_dir()/<agent>/`, not the real, ambient `$HOME` — the real
    /// home is only ever read once, to bootstrap an agent's isolated home
    /// the first time it's used.
    pub fn homes_dir(&self) -> PathBuf {
        self.root.join("homes")
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.state_dir().join("artifacts")
    }

    /// Where a finished task's full captured stdout+stderr lives.
    pub fn task_artifact_path(&self, id: i64) -> PathBuf {
        self.artifacts_dir().join(format!("task-{id}.txt"))
    }

    /// Where a task's output is tee'd *while it's still running* (see
    /// `single-agent-sdk::run::run_command_live`) — the TUI's task-detail
    /// view reads this file for a live view, then switches to
    /// `task_artifact_path` once the task finishes. Same naming
    /// convention on both ends (runtime writer, TUI reader) rather than a
    /// value passed over the socket, since both already agree on
    /// `SingleDirs`.
    pub fn task_live_output_path(&self, id: i64) -> PathBuf {
        self.artifacts_dir().join(format!("task-{id}.live.txt"))
    }

    /// Where ingested documents' original files live, one subdirectory per
    /// document id (see `single-runtime::documents`). The OCR'd/extracted
    /// text itself is stored as an ordinary memory entry, not here — this
    /// only holds the source file plus enough to track provenance.
    pub fn documents_dir(&self) -> PathBuf {
        self.state_dir().join("documents")
    }

    /// Creates the subset of the directory tree Phase 1 needs. Never touches
    /// anything outside `self.root`.
    pub fn ensure_created(&self) -> Result<()> {
        for dir in [self.profiles_dir(), self.agents_dir(), self.state_dir(), self.logs_dir()] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }
}

/// Project-local override directory (`.single/` in a project root), per spec
/// section 28.
pub fn project_config_path(project_root: &std::path::Path) -> PathBuf {
    project_root.join(".single").join("config.yaml")
}

/// The real, ambient `$HOME` — used **only** as a one-time bootstrap
/// source for `agent_home::ensure_bootstrapped`, and by anything that
/// needs to spawn an agent's real interactive login (which has to run
/// against a real filesystem location the OS/browser flow can round-trip
/// to). Nothing else in SingleCLI should read or write through this path
/// directly — see `agent_home`'s module doc for why.
pub fn real_home_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SINGLE_HOME_DIR") {
        return Ok(PathBuf::from(dir));
    }
    directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .context("could not determine home directory")
}
