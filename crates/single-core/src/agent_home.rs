//! Per-agent SingleCLI-managed `$HOME` (see `paths.rs::homes_dir` doc
//! comment). SingleCLI used to run every agent CLI, sync its config, and
//! capture/switch its credentials directly against the real, ambient
//! `$HOME` (`~/.claude.json`, `~/.codex/`, ...). That meant SingleCLI's
//! behavior depended on — and could disturb — files it didn't own.
//!
//! Instead, each agent gets an isolated home under
//! `~/.config/single/homes/<agent>/`. The **first** time that directory is
//! used, it's bootstrapped with a one-time copy of that agent's real,
//! already-verified config/state paths (the same paths
//! `single-agent-sdk::formats` and `single-core::account` read/write) —
//! this is the only time the real home is touched. Every SingleCLI-driven
//! run after that (`task run`, `install-integrations`, `plugin sync`,
//! `account capture/use`, ...) reads and writes only inside the isolated
//! copy; the real `~/.claude`, `~/.codex`, etc. are left alone.
//!
//! **Credentials are never seeded.** The bootstrap copy carries over
//! non-auth config (MCP servers, settings) so a freshly-created isolated
//! home isn't empty, but `credential_paths_for` lists the specific files
//! that are stripped back out right after the copy (e.g. claude's
//! `.claude/.credentials.json`, codex's `.codex/auth.json`). A brand-new
//! isolated home always starts logged out, even on a machine where the
//! real home already has a live login — see `single-core::account`'s
//! module doc for why (`single account capture`/`is_authenticated` only
//! ever look inside the isolated home too).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn home_dir(homes_root: &Path, agent: &str) -> PathBuf {
    homes_root.join(agent)
}

/// Real, `$HOME`-relative paths that make up an agent's config/login
/// state — the same locations documented and verified elsewhere in this
/// project (`single-agent-sdk::formats::{claude,codex,opencode}`,
/// `single-core::account`'s per-agent doc comments), not a guessed list.
/// `agy` and any custom agent have no confirmed on-disk location, so
/// bootstrap for them is a no-op empty directory — SingleCLI simply has
/// nothing to seed the isolated home with.
fn real_paths_for(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &[".claude.json", ".claude"],
        "codex" => &[".codex"],
        "opencode" => &[".config/opencode"],
        "cursor" => &[".cursor"],
        "goose" => &[".config/goose"],
        "aider" => &[".aider.conf.yml"],
        "copilot" => &[".copilot"],
        _ => &[],
    }
}

/// Credential files (relative to a home dir) that must never survive the
/// bootstrap copy, even though they live inside a directory `real_paths_for`
/// otherwise copies wholesale for its non-auth config (MCP servers,
/// settings). Only claude and codex have a confirmed single-file credential
/// location inside those paths — see `single-core::account`'s per-agent
/// doc comments for the same paths used by `has_live_login`/`capture`.
fn credential_paths_for(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &[".claude/.credentials.json"],
        "codex" => &[".codex/auth.json"],
        _ => &[],
    }
}

/// Ensures `homes_root/<agent>` exists, bootstrapping it from `real_home`
/// only if it doesn't exist yet. An already-bootstrapped isolated home is
/// never re-synced from the real one on subsequent calls — once it
/// exists, it's independent, which is the whole point.
pub fn ensure_bootstrapped(homes_root: &Path, real_home: &Path, agent: &str) -> Result<PathBuf> {
    let dest = home_dir(homes_root, agent);
    if dest.exists() {
        return Ok(dest);
    }
    std::fs::create_dir_all(&dest).with_context(|| format!("creating {}", dest.display()))?;
    for rel in real_paths_for(agent) {
        let src = real_home.join(rel);
        if !src.exists() {
            continue;
        }
        let dst = dest.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    for rel in credential_paths_for(agent) {
        let path = dest.join(rel);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing seeded credential {}", path.display()))?;
        }
    }
    Ok(dest)
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        // Symlinks (e.g. a `lib64 -> lib` venv link under a plugin's own
        // state dir) are skipped rather than followed: `fs::copy` can't
        // handle a symlink pointing at a directory (errors loudly), and
        // resolving+copying its target risks pulling in something outside
        // this agent's real config/state paths entirely — not what
        // `real_paths_for` promises to bootstrap. Bootstrap is best-effort
        // config seeding, not a full mirror.
        if file_type.is_symlink() {
            continue;
        }
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstraps_known_paths_on_first_use_only() {
        let real_home = tempfile::tempdir().unwrap();
        std::fs::write(real_home.path().join(".claude.json"), r#"{"numStartups":1}"#).unwrap();
        std::fs::create_dir_all(real_home.path().join(".claude")).unwrap();
        std::fs::write(real_home.path().join(".claude/settings.json"), r#"{"theme":"dark"}"#).unwrap();

        let homes_root = tempfile::tempdir().unwrap();
        let isolated = ensure_bootstrapped(homes_root.path(), real_home.path(), "claude").unwrap();
        assert_eq!(std::fs::read_to_string(isolated.join(".claude.json")).unwrap(), r#"{"numStartups":1}"#);
        assert_eq!(std::fs::read_to_string(isolated.join(".claude/settings.json")).unwrap(), r#"{"theme":"dark"}"#);

        // Real home changes after bootstrap; the isolated copy must NOT pick it up.
        std::fs::write(real_home.path().join(".claude.json"), r#"{"numStartups":99}"#).unwrap();
        let isolated_again = ensure_bootstrapped(homes_root.path(), real_home.path(), "claude").unwrap();
        assert_eq!(std::fs::read_to_string(isolated_again.join(".claude.json")).unwrap(), r#"{"numStartups":1}"#);
    }

    #[test]
    fn bootstrap_seeds_non_auth_config_but_never_credentials() {
        let real_home = tempfile::tempdir().unwrap();
        std::fs::write(real_home.path().join(".claude.json"), r#"{"numStartups":1}"#).unwrap();
        std::fs::create_dir_all(real_home.path().join(".claude")).unwrap();
        std::fs::write(real_home.path().join(".claude/.credentials.json"), r#"{"token":"a"}"#).unwrap();
        std::fs::write(real_home.path().join(".claude/settings.json"), r#"{"theme":"dark"}"#).unwrap();
        std::fs::create_dir_all(real_home.path().join(".codex")).unwrap();
        std::fs::write(real_home.path().join(".codex/auth.json"), r#"{"tokens":"b"}"#).unwrap();
        std::fs::write(real_home.path().join(".codex/config.toml"), "profile = \"default\"").unwrap();

        let homes_root = tempfile::tempdir().unwrap();
        let claude_isolated = ensure_bootstrapped(homes_root.path(), real_home.path(), "claude").unwrap();
        assert!(!claude_isolated.join(".claude/.credentials.json").exists());
        assert_eq!(std::fs::read_to_string(claude_isolated.join(".claude/settings.json")).unwrap(), r#"{"theme":"dark"}"#);

        let codex_isolated = ensure_bootstrapped(homes_root.path(), real_home.path(), "codex").unwrap();
        assert!(!codex_isolated.join(".codex/auth.json").exists());
        assert_eq!(std::fs::read_to_string(codex_isolated.join(".codex/config.toml")).unwrap(), "profile = \"default\"");
    }

    #[test]
    fn agents_with_no_confirmed_real_path_get_an_empty_isolated_home() {
        let real_home = tempfile::tempdir().unwrap();
        let homes_root = tempfile::tempdir().unwrap();
        let isolated = ensure_bootstrapped(homes_root.path(), real_home.path(), "agy").unwrap();
        assert!(isolated.exists());
        assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn bootstrap_skips_symlinks_instead_of_erroring_on_a_dir_symlink() {
        // Reproduces a real case: Claude Code's own security-plugin venv
        // leaves a `lib64 -> lib` symlink under `~/.claude/`, which used to
        // make `fs::copy` fail outright ("neither a regular file nor a
        // symlink to a regular file") and abort the whole bootstrap.
        use std::os::unix::fs::symlink;

        let real_home = tempfile::tempdir().unwrap();
        std::fs::write(real_home.path().join(".claude.json"), r#"{}"#).unwrap();
        std::fs::create_dir_all(real_home.path().join(".claude/venv/lib")).unwrap();
        std::fs::write(real_home.path().join(".claude/venv/lib/marker.txt"), "present").unwrap();
        std::fs::write(real_home.path().join(".claude/.credentials.json"), r#"{"token":"a"}"#).unwrap();
        symlink(real_home.path().join(".claude/venv/lib"), real_home.path().join(".claude/venv/lib64")).unwrap();

        let homes_root = tempfile::tempdir().unwrap();
        let isolated = ensure_bootstrapped(homes_root.path(), real_home.path(), "claude").unwrap();
        assert_eq!(std::fs::read_to_string(isolated.join(".claude/venv/lib/marker.txt")).unwrap(), "present");
        assert!(!isolated.join(".claude/venv/lib64").exists());
        // Credentials still get stripped even inside a directory copied wholesale.
        assert!(!isolated.join(".claude/.credentials.json").exists());
    }
}
