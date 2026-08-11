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
use single_protocol::{AccountProfileInfo, AccountSwitchResult};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileMeta {
    captured_at: String,
    unverified_complete: bool,
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
pub fn capture(accounts_root: &Path, home: &Path, agent: &str, name: &str) -> Result<AccountProfileInfo> {
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

    let meta = ProfileMeta { captured_at: chrono::Utc::now().to_rfc3339(), unverified_complete };
    fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;

    Ok(AccountProfileInfo { agent: agent.to_string(), name: name.to_string(), captured_at: meta.captured_at, unverified_complete })
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
                        captured_at: meta.captured_at,
                        unverified_complete: meta.unverified_complete,
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

        capture(accounts_root.path(), home.path(), "claude", "work").unwrap();

        // Simulate logging into a second account: different creds, different oauthAccount,
        // but numStartups (unrelated config) also changes to prove it's untouched by switch.
        fs::write(home.path().join(".claude/.credentials.json"), r#"{"claudeAiOauth":{"token":"secretC"}}"#).unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"numStartups": 99, "oauthAccount": {"email":"b@example.com"}, "userID": "user-b"}"#).unwrap();
        capture(accounts_root.path(), home.path(), "claude", "personal").unwrap();

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
        capture(accounts_root.path(), home.path(), "codex", "work").unwrap();

        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{"access":"secretD"}}"#).unwrap();
        switch(accounts_root.path(), home.path(), "codex", "work").unwrap();

        let auth: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["access"], "secretB");
    }

    #[test]
    fn capture_fails_when_not_logged_in() {
        let home = tempfile::tempdir().unwrap();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), "codex", "x").is_err());
    }

    #[test]
    fn opencode_and_perplexity_are_explicitly_unsupported() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), "opencode", "x").is_err());
        assert!(capture(accounts_root.path(), home.path(), "perplexity", "x").is_err());
    }

    #[test]
    fn snapshot_files_are_written_with_owner_only_permissions() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work").unwrap();
        let perms = fs::metadata(accounts_root.path().join("codex/work/auth.json")).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn list_and_remove_work() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), "claude", "work").unwrap();
        capture(accounts_root.path(), home.path(), "codex", "work").unwrap();

        let all = list(accounts_root.path(), None).unwrap();
        assert_eq!(all.len(), 2);
        let claude_only = list(accounts_root.path(), Some("claude")).unwrap();
        assert_eq!(claude_only.len(), 1);

        assert!(remove(accounts_root.path(), "claude", "work").unwrap());
        assert!(!remove(accounts_root.path(), "claude", "work").unwrap());
        assert_eq!(list(accounts_root.path(), None).unwrap().len(), 1);
    }
}
