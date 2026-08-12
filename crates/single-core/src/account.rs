//! Multi-account credential switching: capture an agent's *current* login
//! state into a named profile, then swap between profiles later (e.g. two
//! separate Claude Code accounts). Real storage locations, verified by
//! direct inspection on the reference machine (not assumed):
//!
//! - **claude**: `~/.claude/.credentials.json` (OAuth tokens) plus the
//!   `oauthAccount`/`userID` fields inside `~/.claude.json` (which also
//!   holds unrelated config — MCP servers, settings — so only those two
//!   fields are captured/restored, the rest of the file is left alone,
//!   same merge approach as `single-agent-sdk::formats::claude`).
//! - **codex**: `~/.codex/auth.json` is a pure auth file (`auth_mode`,
//!   `OPENAI_API_KEY`, `tokens`, `last_refresh`) — safe to snapshot/
//!   restore wholesale.
//! - **agy**: best-effort. Its real state directory (`~/.gemini/
//!   antigravity-cli/`) has no file literally named "auth"; the two files
//!   captured (`jetski_state.pbtxt`, `settings.json`) are the most likely
//!   candidates (both mode 600, i.e. treated as sensitive by the CLI
//!   itself) but this has **not** been confirmed to be the complete login
//!   state. Flagged as `unverified_complete: true` everywhere it's
//!   surfaced, rather than silently claiming full support.
//! - **opencode**: intentionally **unsupported**. Its login state lives as
//!   rows across several tables (`credential`, `account`,
//!   `account_state`, `control_account`) inside a single live SQLite
//!   database (`~/.local/share/opencode/opencode.db`) that also holds
//!   session/message/project history. Swapping the whole file would nuke
//!   unrelated app state; surgical row-level swap risks violating
//!   foreign-key relationships in a schema this project doesn't own.
//!   Rather than risk corrupting a real database, this is left
//!   unsupported until a safer mechanism is confirmed.
//! - **perplexity** (`pplx`): unsupported — not logged in on the
//!   reference machine and not a coding agent in the first place (see
//!   `docs/install-methods.md`).
//!
//! Every write here goes through the same backup-before-overwrite
//! discipline as `single-agent-sdk`'s config writers, and every snapshot
//! file is created with `0600` permissions since it can contain live
//! OAuth tokens. Nothing in this module ever prints or logs token
//! contents — only profile names and timestamps.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use single_protocol::{AccountProfileInfo, AccountStatus, AccountSwitchResult};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileMeta {
    captured_at: String,
    unverified_complete: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    status: AccountStatus,
}

/// Which agents support account switching at all, and how "complete" the
/// captured state is known to be.
fn support(agent: &str) -> Result<bool> {
    Ok(match agent {
        "claude" | "codex" => false, // fully verified
        "agy" => true,               // best-effort, flagged
        "opencode" => bail!("opencode account switching is unsupported: its login state lives in a live multi-table SQLite database (~/.local/share/opencode/opencode.db) shared with session history — see account.rs module docs for why a safe snapshot mechanism doesn't exist yet"),
        "perplexity" => bail!("pplx has no coding-agent session to switch between; not supported"),
        other => bail!("unknown agent: {other}"),
    })
}

fn profile_dir(accounts_root: &Path, agent: &str, name: &str) -> PathBuf {
    accounts_root.join(agent).join(name)
}

fn write_secure(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn backup(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let backup_path = path.with_extension(format!(
        "{}.bak-{timestamp}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}

/// Captures the agent's currently-live login state into a new named
/// profile. Fails if the agent isn't currently logged in (best-effort
/// check: the relevant file(s) must exist).
pub fn capture(accounts_root: &Path, home: &Path, agent: &str, name: &str, label: Option<String>) -> Result<AccountProfileInfo> {
    let unverified_complete = support(agent)?;
    let dir = profile_dir(accounts_root, agent, name);
    fs::create_dir_all(&dir)?;

    match agent {
        "claude" => {
            let creds_path = home.join(".claude/.credentials.json");
            if !creds_path.exists() {
                bail!("claude is not currently logged in ({} not found)", creds_path.display());
            }
            let creds = fs::read(&creds_path)?;
            write_secure(&dir.join("credentials.json"), &creds)?;

            let config_path = home.join(".claude.json");
            let oauth_fields = extract_json_fields(&config_path, &["oauthAccount", "userID"])?;
            write_secure(&dir.join("oauth_account.json"), serde_json::to_string_pretty(&oauth_fields)?.as_bytes())?;
        }
        "codex" => {
            let auth_path = home.join(".codex/auth.json");
            if !auth_path.exists() {
                bail!("codex is not currently logged in ({} not found)", auth_path.display());
            }
            let auth = fs::read(&auth_path)?;
            write_secure(&dir.join("auth.json"), &auth)?;
        }
        "agy" => {
            let state_dir = home.join(".gemini/antigravity-cli");
            let mut any_captured = false;
            for filename in ["jetski_state.pbtxt", "settings.json"] {
                let src = state_dir.join(filename);
                if src.exists() {
                    write_secure(&dir.join(filename), &fs::read(&src)?)?;
                    any_captured = true;
                }
            }
            if !any_captured {
                bail!("agy has no recognizable login state at {} (is it logged in?)", state_dir.display());
            }
        }
        _ => unreachable!("support() already rejected unsupported agents"),
    }

    let meta = ProfileMeta { captured_at: chrono::Utc::now().to_rfc3339(), unverified_complete, label, status: AccountStatus::Unknown };
    fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;

    Ok(AccountProfileInfo {
        agent: agent.to_string(),
        name: name.to_string(),
        label: meta.label,
        captured_at: meta.captured_at,
        unverified_complete,
        status: meta.status,
    })
}

/// Sets the manually-tracked usability status of a captured account (see
/// `AccountStatus` docs — SingleCLI never auto-detects this).
pub fn set_status(accounts_root: &Path, agent: &str, name: &str, status: AccountStatus) -> Result<()> {
    let dir = profile_dir(accounts_root, agent, name);
    let meta_path = dir.join("meta.json");
    if !meta_path.exists() {
        bail!("no profile named '{name}' for agent '{agent}'");
    }
    let mut meta: ProfileMeta = serde_json::from_str(&fs::read_to_string(&meta_path)?)?;
    meta.status = status;
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}

/// Swaps a captured profile into place as the agent's live login state,
/// backing up whatever was live first.
pub fn switch(accounts_root: &Path, home: &Path, agent: &str, name: &str) -> Result<AccountSwitchResult> {
    support(agent)?;
    let dir = profile_dir(accounts_root, agent, name);
    if !dir.exists() {
        bail!("no profile named '{name}' for agent '{agent}'");
    }

    let mut backed_up = Vec::new();

    match agent {
        "claude" => {
            let creds_path = home.join(".claude/.credentials.json");
            if let Some(b) = backup(&creds_path)? {
                backed_up.push(b.display().to_string());
            }
            write_secure(&creds_path, &fs::read(dir.join("credentials.json"))?)?;

            let config_path = home.join(".claude.json");
            if let Some(b) = backup(&config_path)? {
                backed_up.push(b.display().to_string());
            }
            let oauth_fields: Value = serde_json::from_slice(&fs::read(dir.join("oauth_account.json"))?)?;
            merge_json_fields(&config_path, &oauth_fields)?;
        }
        "codex" => {
            let auth_path = home.join(".codex/auth.json");
            if let Some(b) = backup(&auth_path)? {
                backed_up.push(b.display().to_string());
            }
            write_secure(&auth_path, &fs::read(dir.join("auth.json"))?)?;
        }
        "agy" => {
            let state_dir = home.join(".gemini/antigravity-cli");
            for filename in ["jetski_state.pbtxt", "settings.json"] {
                let snapshot = dir.join(filename);
                if !snapshot.exists() {
                    continue;
                }
                let live = state_dir.join(filename);
                if let Some(b) = backup(&live)? {
                    backed_up.push(b.display().to_string());
                }
                write_secure(&live, &fs::read(&snapshot)?)?;
            }
        }
        _ => unreachable!("support() already rejected unsupported agents"),
    }

    Ok(AccountSwitchResult { agent: agent.to_string(), name: name.to_string(), backed_up })
}

/// Materializes an isolated `$HOME` for `agent`/`name` at
/// `<accounts_root>/<agent>/<name>/home`, so this captured account can run
/// **concurrently** with other accounts of the same agent (spec: "2 claude
/// code, 3 codex... as i need") without any of them overwriting each
/// other's live credentials the way `switch` does. On every call, copies
/// `base_home`'s non-credential config the first time (so MCP servers /
/// settings carry over from the real install) and always re-overlays this
/// profile's captured credentials on top, so re-running after `capture`
/// picks up refreshed tokens. Returns the isolated home directory —
/// callers pass it as `$HOME` to the agent's subprocess (see
/// `single-agent-sdk::run::run_command_with_home`).
pub fn ensure_isolated_home(accounts_root: &Path, base_home: &Path, agent: &str, name: &str) -> Result<PathBuf> {
    support(agent)?;
    let profile = profile_dir(accounts_root, agent, name);
    if !profile.exists() {
        bail!("no profile named '{name}' for agent '{agent}'");
    }
    let isolated = profile.join("home");
    fs::create_dir_all(&isolated)?;

    match agent {
        "claude" => {
            let config_path = isolated.join(".claude.json");
            if !config_path.exists() {
                let base_config = base_home.join(".claude.json");
                if base_config.exists() {
                    fs::copy(&base_config, &config_path)?;
                }
            }
            fs::create_dir_all(isolated.join(".claude"))?;
            write_secure(&isolated.join(".claude/.credentials.json"), &fs::read(profile.join("credentials.json"))?)?;
            let oauth_fields: Value = serde_json::from_slice(&fs::read(profile.join("oauth_account.json"))?)?;
            merge_json_fields(&config_path, &oauth_fields)?;
        }
        "codex" => {
            let isolated_codex_dir = isolated.join(".codex");
            fs::create_dir_all(&isolated_codex_dir)?;
            let config_path = isolated_codex_dir.join("config.toml");
            if !config_path.exists() {
                let base_config = base_home.join(".codex").join("config.toml");
                if base_config.exists() {
                    fs::copy(&base_config, &config_path)?;
                }
            }
            write_secure(&isolated_codex_dir.join("auth.json"), &fs::read(profile.join("auth.json"))?)?;
        }
        "agy" => {
            let base_state_dir = base_home.join(".gemini/antigravity-cli");
            let isolated_state_dir = isolated.join(".gemini/antigravity-cli");
            fs::create_dir_all(&isolated_state_dir)?;
            for filename in ["jetski_state.pbtxt", "settings.json"] {
                let snapshot = profile.join(filename);
                if snapshot.exists() {
                    write_secure(&isolated_state_dir.join(filename), &fs::read(&snapshot)?)?;
                } else {
                    let base_file = base_state_dir.join(filename);
                    let dest = isolated_state_dir.join(filename);
                    if base_file.exists() && !dest.exists() {
                        fs::copy(&base_file, &dest)?;
                    }
                }
            }
        }
        _ => unreachable!("support() already rejected unsupported agents"),
    }

    Ok(isolated)
}

pub fn list(accounts_root: &Path, agent_filter: Option<&str>) -> Result<Vec<AccountProfileInfo>> {
    let mut results = Vec::new();
    if !accounts_root.exists() {
        return Ok(results);
    }
    for agent_entry in fs::read_dir(accounts_root)? {
        let agent_entry = agent_entry?;
        if !agent_entry.file_type()?.is_dir() {
            continue;
        }
        let agent = agent_entry.file_name().to_string_lossy().to_string();
        if let Some(filter) = agent_filter {
            if agent != filter {
                continue;
            }
        }
        for profile_entry in fs::read_dir(agent_entry.path())? {
            let profile_entry = profile_entry?;
            if !profile_entry.file_type()?.is_dir() {
                continue;
            }
            let name = profile_entry.file_name().to_string_lossy().to_string();
            let meta_path = profile_entry.path().join("meta.json");
            if let Ok(text) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<ProfileMeta>(&text) {
                    results.push(AccountProfileInfo {
                        agent: agent.clone(),
                        name,
                        label: meta.label,
                        captured_at: meta.captured_at,
                        unverified_complete: meta.unverified_complete,
                        status: meta.status,
                    });
                }
            }
        }
    }
    results.sort_by(|a, b| (a.agent.as_str(), a.name.as_str()).cmp(&(b.agent.as_str(), b.name.as_str())));
    Ok(results)
}

pub fn remove(accounts_root: &Path, agent: &str, name: &str) -> Result<bool> {
    let dir = profile_dir(accounts_root, agent, name);
    if !dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&dir)?;
    Ok(true)
}

fn extract_json_fields(path: &Path, fields: &[&str]) -> Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Default::default()));
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let full: Value = serde_json::from_str(&text)?;
    let mut extracted = serde_json::Map::new();
    if let Some(obj) = full.as_object() {
        for field in fields {
            if let Some(v) = obj.get(*field) {
                extracted.insert(field.to_string(), v.clone());
            }
        }
    }
    Ok(Value::Object(extracted))
}

fn merge_json_fields(path: &Path, fields: &Value) -> Result<()> {
    let mut root: Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(path)?)?
    } else {
        Value::Object(Default::default())
    };
    let root_obj = root.as_object_mut().context("target config root is not a JSON object")?;
    if let Some(fields_obj) = fields.as_object() {
        for (k, v) in fields_obj {
            root_obj.insert(k.clone(), v.clone());
        }
    }
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_fake_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(home.path().join(".claude/.credentials.json"), r#"{"claudeAiOauth":{"token":"secretA"}}"#).unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"numStartups": 5, "oauthAccount": {"email":"a@example.com"}, "userID": "user-a"}"#).unwrap();

        fs::create_dir_all(home.path().join(".codex")).unwrap();
        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"chatgpt","tokens":{"access":"secretB"}}"#).unwrap();
        home
    }

    #[test]
    fn capture_then_switch_round_trips_claude_credentials_and_preserves_other_config() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();

        capture(accounts_root.path(), home.path(), "claude", "work", None).unwrap();

        // Simulate logging into a second account: different creds, different oauthAccount,
        // but numStartups (unrelated config) also changes to prove it's untouched by switch.
        fs::write(home.path().join(".claude/.credentials.json"), r#"{"claudeAiOauth":{"token":"secretC"}}"#).unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"numStartups": 99, "oauthAccount": {"email":"b@example.com"}, "userID": "user-b"}"#).unwrap();
        capture(accounts_root.path(), home.path(), "claude", "personal", None).unwrap();

        let result = switch(accounts_root.path(), home.path(), "claude", "work").unwrap();
        assert_eq!(result.backed_up.len(), 2);

        let creds: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".claude/.credentials.json")).unwrap()).unwrap();
        assert_eq!(creds["claudeAiOauth"]["token"], "secretA");

        let config: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".claude.json")).unwrap()).unwrap();
        assert_eq!(config["oauthAccount"]["email"], "a@example.com");
        assert_eq!(config["userID"], "user-a");
        // numStartups was 99 (from the "personal" session) and must survive the switch
        // untouched, proving this only merges the two account-identity fields.
        assert_eq!(config["numStartups"], 99);
    }

    #[test]
    fn codex_full_file_swap_round_trips() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work", None).unwrap();

        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{"access":"secretD"}}"#).unwrap();
        switch(accounts_root.path(), home.path(), "codex", "work").unwrap();

        let auth: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["access"], "secretB");
    }

    #[test]
    fn capture_fails_when_not_logged_in() {
        let home = tempfile::tempdir().unwrap();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), "codex", "x", None).is_err());
    }

    #[test]
    fn opencode_and_perplexity_are_explicitly_unsupported() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), "opencode", "x", None).is_err());
        assert!(capture(accounts_root.path(), home.path(), "perplexity", "x", None).is_err());
    }

    #[test]
    fn snapshot_files_are_written_with_owner_only_permissions() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work", None).unwrap();
        let perms = fs::metadata(accounts_root.path().join("codex/work/auth.json")).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn isolated_homes_let_two_accounts_of_the_same_agent_coexist() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work", Some("work@example.com".into())).unwrap();

        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{"access":"secretPersonal"}}"#).unwrap();
        capture(accounts_root.path(), home.path(), "codex", "personal", Some("me@example.com".into())).unwrap();

        let work_home = ensure_isolated_home(accounts_root.path(), home.path(), "codex", "work").unwrap();
        let personal_home = ensure_isolated_home(accounts_root.path(), home.path(), "codex", "personal").unwrap();
        assert_ne!(work_home, personal_home);

        let work_auth: Value = serde_json::from_str(&fs::read_to_string(work_home.join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(work_auth["tokens"]["access"], "secretB");
        let personal_auth: Value = serde_json::from_str(&fs::read_to_string(personal_home.join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(personal_auth["tokens"]["access"], "secretPersonal");

        // The real, live $HOME's auth.json must be untouched by either call.
        let live_auth: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(live_auth["tokens"]["access"], "secretPersonal");
    }

    #[test]
    fn list_and_remove_work() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "claude", "work", None).unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work", None).unwrap();

        let all = list(accounts_root.path(), None).unwrap();
        assert_eq!(all.len(), 2);
        let claude_only = list(accounts_root.path(), Some("claude")).unwrap();
        assert_eq!(claude_only.len(), 1);

        assert!(remove(accounts_root.path(), "claude", "work").unwrap());
        assert!(!remove(accounts_root.path(), "claude", "work").unwrap());
        assert_eq!(list(accounts_root.path(), None).unwrap().len(), 1);
    }
}
