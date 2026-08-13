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
use single_protocol::{AccountProfileInfo, AccountStatus, AccountSwitchResult, AuthState};
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

/// Whether `agent`'s credential file(s) are present under `home` — the
/// same presence check `capture` already used, factored out so it can also
/// answer "is *anything* logged in" (see `is_authenticated`) without the
/// side effects of an actual capture.
pub fn has_live_login(home: &Path, agent: &str) -> bool {
    match agent {
        "claude" => home.join(".claude/.credentials.json").exists(),
        "codex" => home.join(".codex/auth.json").exists(),
        "agy" => {
            let dir = home.join(".gemini/antigravity-cli");
            ["jetski_state.pbtxt", "settings.json"].iter().any(|f| dir.join(f).exists())
        }
        _ => false,
    }
}

/// Picks whichever of `home` (SingleCLI's isolated home) or `real_home`
/// (the actual ambient `$HOME`) has a live login for `agent`, preferring
/// `home`. A user who ran the vendor CLI directly instead of `single agent
/// login` ends up logged in only under `real_home` — this is what lets
/// `capture` find that login instead of failing.
fn resolve_live_home<'a>(home: &'a Path, real_home: &'a Path, agent: &str) -> Option<&'a Path> {
    if has_live_login(home, agent) {
        Some(home)
    } else if has_live_login(real_home, agent) {
        Some(real_home)
    } else {
        None
    }
}

/// Auto-detected "is anything logged in right now" for `agent`, checked
/// across both homes. See `AuthState` docs for how this differs from the
/// manually-set `AccountStatus` on a captured profile.
pub fn is_authenticated(home: &Path, real_home: &Path, agent: &str) -> AuthState {
    if support(agent).is_err() {
        return AuthState::Unsupported;
    }
    if resolve_live_home(home, real_home, agent).is_some() {
        AuthState::Authenticated
    } else {
        AuthState::NotAuthenticated
    }
}

/// Best-effort human-readable identity for the currently-live login under
/// `home`, used to auto-label an account right after `single agent login`.
/// `None` when the agent's credential format has no identity field
/// (e.g. codex's `auth.json`) or nothing is logged in yet — callers should
/// fall back to a generated name in that case.
pub fn derive_label(home: &Path, agent: &str) -> Option<String> {
    match agent {
        "claude" => {
            let fields = extract_json_fields(&home.join(".claude.json"), &["oauthAccount", "userID"]).ok()?;
            fields
                .get("oauthAccount")
                .and_then(|o| o.get("email"))
                .and_then(|v| v.as_str())
                .or_else(|| fields.get("userID").and_then(|v| v.as_str()))
                .map(str::to_string)
        }
        _ => None,
    }
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
/// profile. Checks SingleCLI's isolated `home` first; if nothing is there
/// (e.g. the user logged in via the vendor CLI directly instead of `single
/// agent login`), falls back to `real_home` and, on success, also syncs
/// the just-captured credentials into `home` so future isolated runs pick
/// them up. Fails only if neither home has a live login.
pub fn capture(accounts_root: &Path, home: &Path, real_home: &Path, agent: &str, name: &str, label: Option<String>) -> Result<AccountProfileInfo> {
    let unverified_complete = support(agent)?;
    let dir = profile_dir(accounts_root, agent, name);
    fs::create_dir_all(&dir)?;

    let Some(source) = resolve_live_home(home, real_home, agent) else {
        bail!(
            "{agent} is not currently logged in (checked isolated home {} and real home {})",
            home.display(),
            real_home.display()
        );
    };
    let found_only_in_real_home = source == real_home;

    match agent {
        "claude" => {
            let creds = fs::read(source.join(".claude/.credentials.json"))?;
            write_secure(&dir.join("credentials.json"), &creds)?;

            let config_path = source.join(".claude.json");
            let oauth_fields = extract_json_fields(&config_path, &["oauthAccount", "userID"])?;
            write_secure(&dir.join("oauth_account.json"), serde_json::to_string_pretty(&oauth_fields)?.as_bytes())?;
        }
        "codex" => {
            let auth = fs::read(source.join(".codex/auth.json"))?;
            write_secure(&dir.join("auth.json"), &auth)?;
        }
        "agy" => {
            let state_dir = source.join(".gemini/antigravity-cli");
            for filename in ["jetski_state.pbtxt", "settings.json"] {
                let src = state_dir.join(filename);
                if src.exists() {
                    write_secure(&dir.join(filename), &fs::read(&src)?)?;
                }
            }
        }
        _ => unreachable!("support() already rejected unsupported agents"),
    }

    let meta = ProfileMeta { captured_at: chrono::Utc::now().to_rfc3339(), unverified_complete, label, status: AccountStatus::Unknown };
    fs::write(dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;

    if found_only_in_real_home {
        // The isolated home had no credentials of its own (that's why we
        // fell back) — sync what we just captured into it so subsequent
        // isolated runs (and `is_authenticated` checks against `home`
        // alone) see this login too, without disturbing anything else.
        write_credentials_into(home, &dir, agent)?;
    }

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
    let backed_up = write_credentials_into(home, &dir, agent)?;
    Ok(AccountSwitchResult { agent: agent.to_string(), name: name.to_string(), backed_up })
}

/// Writes a captured profile's credential files into `target_home` as its
/// live login state, backing up whatever was already there first. Shared
/// by `switch` (explicit account swap) and `capture`'s real-home fallback
/// (syncing the isolated home right after finding a login only in the
/// real one) so the per-agent file layout only lives in one place.
fn write_credentials_into(target_home: &Path, profile: &Path, agent: &str) -> Result<Vec<String>> {
    let mut backed_up = Vec::new();

    match agent {
        "claude" => {
            let creds_path = target_home.join(".claude/.credentials.json");
            if let Some(b) = backup(&creds_path)? {
                backed_up.push(b.display().to_string());
            }
            write_secure(&creds_path, &fs::read(profile.join("credentials.json"))?)?;

            let config_path = target_home.join(".claude.json");
            if let Some(b) = backup(&config_path)? {
                backed_up.push(b.display().to_string());
            }
            let oauth_fields: Value = serde_json::from_slice(&fs::read(profile.join("oauth_account.json"))?)?;
            merge_json_fields(&config_path, &oauth_fields)?;
        }
        "codex" => {
            let auth_path = target_home.join(".codex/auth.json");
            if let Some(b) = backup(&auth_path)? {
                backed_up.push(b.display().to_string());
            }
            write_secure(&auth_path, &fs::read(profile.join("auth.json"))?)?;
        }
        "agy" => {
            let state_dir = target_home.join(".gemini/antigravity-cli");
            for filename in ["jetski_state.pbtxt", "settings.json"] {
                let snapshot = profile.join(filename);
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

    Ok(backed_up)
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

        capture(accounts_root.path(), home.path(), home.path(), "claude", "work", None).unwrap();

        // Simulate logging into a second account: different creds, different oauthAccount,
        // but numStartups (unrelated config) also changes to prove it's untouched by switch.
        fs::write(home.path().join(".claude/.credentials.json"), r#"{"claudeAiOauth":{"token":"secretC"}}"#).unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"numStartups": 99, "oauthAccount": {"email":"b@example.com"}, "userID": "user-b"}"#).unwrap();
        capture(accounts_root.path(), home.path(), home.path(), "claude", "personal", None).unwrap();

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
        capture(accounts_root.path(), home.path(), home.path(), "codex", "work", None).unwrap();

        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{"access":"secretD"}}"#).unwrap();
        switch(accounts_root.path(), home.path(), "codex", "work").unwrap();

        let auth: Value = serde_json::from_str(&fs::read_to_string(home.path().join(".codex/auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["access"], "secretB");
    }

    #[test]
    fn capture_fails_when_not_logged_in() {
        let home = tempfile::tempdir().unwrap();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), home.path(), "codex", "x", None).is_err());
    }

    #[test]
    fn opencode_and_perplexity_are_explicitly_unsupported() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        assert!(capture(accounts_root.path(), home.path(), home.path(), "opencode", "x", None).is_err());
        assert!(capture(accounts_root.path(), home.path(), home.path(), "perplexity", "x", None).is_err());
    }

    #[test]
    fn snapshot_files_are_written_with_owner_only_permissions() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), home.path(), "codex", "work", None).unwrap();
        let perms = fs::metadata(accounts_root.path().join("codex/work/auth.json")).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn isolated_homes_let_two_accounts_of_the_same_agent_coexist() {
        let home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();
        capture(accounts_root.path(), home.path(), home.path(), "codex", "work", Some("work@example.com".into())).unwrap();

        fs::write(home.path().join(".codex/auth.json"), r#"{"auth_mode":"apikey","tokens":{"access":"secretPersonal"}}"#).unwrap();
        capture(accounts_root.path(), home.path(), home.path(), "codex", "personal", Some("me@example.com".into())).unwrap();

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
        capture(accounts_root.path(), home.path(), home.path(), "claude", "work", None).unwrap();
        capture(accounts_root.path(), home.path(), home.path(), "codex", "work", None).unwrap();

        let all = list(accounts_root.path(), None).unwrap();
        assert_eq!(all.len(), 2);
        let claude_only = list(accounts_root.path(), Some("claude")).unwrap();
        assert_eq!(claude_only.len(), 1);

        assert!(remove(accounts_root.path(), "claude", "work").unwrap());
        assert!(!remove(accounts_root.path(), "claude", "work").unwrap());
        assert_eq!(list(accounts_root.path(), None).unwrap().len(), 1);
    }

    #[test]
    fn capture_falls_back_to_real_home_and_syncs_isolated_home() {
        // Isolated home exists but has never had claude credentials written
        // into it (the scenario when a user ran `claude` directly instead
        // of `single agent login claude`). Real home has a live login.
        let isolated_home = tempfile::tempdir().unwrap();
        let real_home = setup_fake_home();
        let accounts_root = tempfile::tempdir().unwrap();

        assert!(!has_live_login(isolated_home.path(), "claude"));
        assert!(has_live_login(real_home.path(), "claude"));

        let info = capture(accounts_root.path(), isolated_home.path(), real_home.path(), "claude", "work", None).unwrap();
        assert_eq!(info.agent, "claude");

        // The isolated home must now also have live credentials, synced as
        // part of the same capture call.
        assert!(has_live_login(isolated_home.path(), "claude"));
        let synced: Value =
            serde_json::from_str(&fs::read_to_string(isolated_home.path().join(".claude/.credentials.json")).unwrap()).unwrap();
        assert_eq!(synced["claudeAiOauth"]["token"], "secretA");
    }

    #[test]
    fn is_authenticated_checks_both_homes_and_flags_unsupported_agents() {
        let isolated_home = tempfile::tempdir().unwrap();
        let real_home = setup_fake_home();

        assert_eq!(is_authenticated(isolated_home.path(), real_home.path(), "codex"), AuthState::Authenticated);
        assert_eq!(is_authenticated(isolated_home.path(), isolated_home.path(), "codex"), AuthState::NotAuthenticated);
        assert_eq!(is_authenticated(isolated_home.path(), real_home.path(), "opencode"), AuthState::Unsupported);
    }

    #[test]
    fn derive_label_reads_claude_email_but_not_codex() {
        let home = setup_fake_home();
        assert_eq!(derive_label(home.path(), "claude").as_deref(), Some("a@example.com"));
        assert_eq!(derive_label(home.path(), "codex"), None);
    }
}
