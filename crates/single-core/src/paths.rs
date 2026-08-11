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
