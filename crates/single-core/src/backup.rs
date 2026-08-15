//! Full-setup backup/restore: everything under `SingleDirs::root()` plus
//! every secret in the OS keychain, packed into one password-encrypted
//! archive that can be moved to another machine.
//!
//! Deliberately **not** built as a `Request`/`ResponseData` round trip
//! through `single-runtime` the way every other SingleCLI feature is —
//! the passphrase that protects live OAuth tokens and API keys must never
//! cross the daemon's Unix socket or land in its event log. This module
//! runs entirely in-process instead, callable directly by both
//! `single-cli` (which already links `single-runtime` in-process for its
//! own fallback path, see `single-cli::client`) and `single-tui` (which
//! normally talks to the daemon exclusively over the socket, but has no
//! other way to reach this logic without either a new dependency on
//! `single-runtime` or duplicating the engine — `single-core` is the one
//! crate both already share, so that's where this lives).
//!
//! Archive format: a tar of every real file under `root()`, plus one
//! synthetic entry (`SECRETS_MANIFEST_NAME`) holding every OS-keychain
//! secret as TOML — chosen as a name no real SingleCLI config file could
//! ever collide with — encrypted as a whole with `age`'s passphrase
//! (scrypt) mode.
//!
//! **Caveat not solved here**: if `single-runtimed` is actively running
//! and writing to `state/single.db` during an export, the snapshot could
//! catch it mid-write (SQLite's `-wal`/`-shm` companion files are backed
//! up as plain files, not through a consistent checkpoint). Callers
//! (`single backup export`, the TUI's Backup tab) should tell the user to
//! stop the daemon first — enforcing that is a UX concern for those
//! call sites, not this engine.

use crate::paths::SingleDirs;
use crate::secrets::{SecretStore, SecretTool};
use age::secrecy::SecretString;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Reserved tar entry name for the secrets manifest — not a real relative
/// path under `SingleDirs::root()`, so it can never collide with an
/// actual config file during either pack or unpack.
const SECRETS_MANIFEST_NAME: &str = "__singlecli_secrets__.toml";

/// Skipped during export: a live socket (meaningless once copied to
/// another machine) and operational log noise (not "setup").
fn is_excluded(relative: &Path) -> bool {
    relative == Path::new("state/runtime.sock") || relative.starts_with("logs")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupItemResult {
    pub path: String,
    pub success: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct BackupReport {
    pub dry_run: bool,
    pub files: Vec<BackupItemResult>,
    pub secrets: Vec<BackupItemResult>,
}

/// Walks `dirs.root()`, tars every real file plus a secrets manifest built
/// from every entry in the OS keychain, encrypts the tar with `passphrase`
/// (age scrypt/passphrase mode), and writes it to `dest_path`. Returns any
/// warnings (currently just "keychain unavailable") — a missing
/// `secret-tool` binary degrades to "no secrets captured" rather than
/// failing the whole export, same as `embeddings.rs`'s treatment of a
/// missing optional external capability elsewhere in this codebase.
pub fn export(dirs: &SingleDirs, dest_path: &Path, passphrase: &SecretString) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        add_dir_recursive(&mut builder, dirs.root(), dirs.root())?;

        let store = SecretTool;
        let mut secrets = BTreeMap::new();
        match SecretStore::list(&store) {
            Ok(names) => {
                for name in names {
                    match SecretStore::get(&store, &name) {
                        Ok(Some(value)) => {
                            secrets.insert(name, value);
                        }
                        Ok(None) => {}
                        Err(e) => warnings.push(format!("failed to read secret '{name}': {e:#}")),
                    }
                }
            }
            Err(e) => warnings.push(format!("keychain unavailable, no secrets included in this backup: {e:#}")),
        }
        let manifest = SecretsManifest { secrets };
        let rendered = toml::to_string_pretty(&manifest).context("serializing secrets manifest")?;
        let mut header = tar::Header::new_gnu();
        header.set_size(rendered.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        builder.append_data(&mut header, SECRETS_MANIFEST_NAME, rendered.as_bytes())?;
        builder.finish().context("finishing tar archive")?;
    }

    let encryptor = age::Encryptor::with_user_passphrase(passphrase.clone());
    let mut encrypted = Vec::new();
    let mut writer = encryptor.wrap_output(&mut encrypted).context("initializing age encryption")?;
    writer.write_all(&tar_bytes).context("writing archive contents")?;
    writer.finish().context("finalizing age encryption")?;

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest_path, &encrypted).with_context(|| format!("writing {}", dest_path.display()))?;
    // The archive holds live credentials — same discipline as
    // single_core::account's snapshot files.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest_path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(warnings)
}

fn add_dir_recursive<W: Write>(builder: &mut tar::Builder<W>, base: &Path, dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
        if is_excluded(&relative) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            add_dir_recursive(builder, base, &path)?;
        } else if file_type.is_file() {
            builder.append_path_with_name(&path, &relative).with_context(|| format!("archiving {}", path.display()))?;
        }
        // Symlinks are skipped, same reasoning agent_home::ensure_bootstrapped
        // already documents for its own directory walk: not worth the
        // risk of an unexpected target on the machine being restored to.
    }
    Ok(())
}

/// Decrypts `src_path` with `passphrase`, unpacks the tar, and writes
/// every file back under `dirs.root()` (through the same
/// backup-before-overwrite discipline every other SingleCLI config writer
/// uses) and every secret back into the OS keychain. `dry_run` reports
/// what would happen without writing anything.
pub fn import(dirs: &SingleDirs, src_path: &Path, passphrase: &SecretString, dry_run: bool) -> Result<BackupReport> {
    let encrypted = std::fs::read(src_path).with_context(|| format!("reading {}", src_path.display()))?;
    let decryptor = age::Decryptor::new(&encrypted[..]).context("this file isn't a valid age-encrypted archive")?;
    let mut tar_bytes = Vec::new();
    let mut reader = decryptor
        .decrypt(std::iter::once(&age::scrypt::Identity::new(passphrase.clone()) as &dyn age::Identity))
        .context("decryption failed — wrong passphrase, or the archive is corrupted")?;
    reader.read_to_end(&mut tar_bytes).context("reading decrypted archive")?;

    let mut report = BackupReport { dry_run, ..Default::default() };
    let mut archive = tar::Archive::new(&tar_bytes[..]);
    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();

        if entry_path == Path::new(SECRETS_MANIFEST_NAME) {
            let mut contents = String::new();
            entry.read_to_string(&mut contents).context("reading secrets manifest")?;
            let manifest: SecretsManifest = toml::from_str(&contents).context("parsing secrets manifest")?;
            restore_secrets(manifest, dry_run, &mut report);
            continue;
        }

        let target = dirs.root().join(&entry_path);
        let label = entry_path.display().to_string();
        if dry_run {
            report.files.push(BackupItemResult { path: label, success: true, detail: "would restore".into() });
            continue;
        }
        match restore_one_file(&mut entry, &target) {
            Ok(()) => report.files.push(BackupItemResult { path: label, success: true, detail: "restored".into() }),
            Err(e) => report.files.push(BackupItemResult { path: label, success: false, detail: format!("{e:#}") }),
        }
    }
    Ok(report)
}

fn restore_one_file<R: Read>(entry: &mut tar::Entry<R>, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    backup_file_before_write(target)?;
    let mut contents = Vec::new();
    entry.read_to_end(&mut contents)?;
    std::fs::write(target, contents).with_context(|| format!("writing {}", target.display()))
}

/// Same one-file-backup-before-overwrite discipline
/// `single-agent-sdk::backup::backup_before_write` already applies to
/// every agent config write — duplicated here rather than imported since
/// `single-agent-sdk` depends on `single-core`, not the other way around.
fn backup_file_before_write(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let backup_path = PathBuf::from(format!("{}.bak-{timestamp}", path.display()));
    std::fs::copy(path, &backup_path).with_context(|| format!("backing up {} to {}", path.display(), backup_path.display()))?;
    Ok(())
}

fn restore_secrets(manifest: SecretsManifest, dry_run: bool, report: &mut BackupReport) {
    let store = SecretTool;
    for (name, value) in manifest.secrets {
        if dry_run {
            report.secrets.push(BackupItemResult { path: name, success: true, detail: "would restore".into() });
            continue;
        }
        match SecretStore::set(&store, &name, &value) {
            Ok(()) => report.secrets.push(BackupItemResult { path: name, success: true, detail: "restored".into() }),
            Err(e) => report.secrets.push(BackupItemResult { path: name, success: false, detail: format!("{e:#}") }),
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct SecretsManifest {
    #[serde(default)]
    secrets: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_import_round_trips_files_and_secrets_byte_identical() {
        let src_dir = tempfile::tempdir().unwrap();
        let dirs = SingleDirs::from_root(src_dir.path().to_path_buf());
        std::fs::create_dir_all(dirs.root().join("profiles")).unwrap();
        std::fs::write(dirs.config_file(), "answer = 42\n").unwrap();
        std::fs::write(dirs.root().join("profiles").join("default.toml"), "name = \"default\"\n").unwrap();
        // A live socket must never survive a round trip.
        std::fs::create_dir_all(dirs.state_dir()).unwrap();
        std::fs::write(dirs.socket_path(), "not a real socket").unwrap();

        let archive_path = tempfile::tempdir().unwrap().path().join("backup.age");
        let passphrase = SecretString::from("correct horse battery staple".to_string());
        export(&dirs, &archive_path, &passphrase).unwrap();
        assert!(archive_path.exists());

        let dest_dir = tempfile::tempdir().unwrap();
        let dest_dirs = SingleDirs::from_root(dest_dir.path().to_path_buf());
        let report = import(&dest_dirs, &archive_path, &passphrase, false).unwrap();
        assert!(!report.dry_run);
        assert!(report.files.iter().all(|f| f.success), "every file should restore cleanly: {:?}", report.files);

        assert_eq!(std::fs::read_to_string(dest_dirs.config_file()).unwrap(), "answer = 42\n");
        assert_eq!(std::fs::read_to_string(dest_dirs.root().join("profiles").join("default.toml")).unwrap(), "name = \"default\"\n");
        assert!(!dest_dirs.socket_path().exists(), "runtime.sock must be excluded from the archive");
    }

    #[test]
    fn wrong_passphrase_fails_cleanly_not_partially() {
        let src_dir = tempfile::tempdir().unwrap();
        let dirs = SingleDirs::from_root(src_dir.path().to_path_buf());
        std::fs::write(dirs.config_file(), "answer = 42\n").unwrap();

        let archive_path = tempfile::tempdir().unwrap().path().join("backup.age");
        export(&dirs, &archive_path, &SecretString::from("right-passphrase".to_string())).unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let dest_dirs = SingleDirs::from_root(dest_dir.path().to_path_buf());
        let result = import(&dest_dirs, &archive_path, &SecretString::from("wrong-passphrase".to_string()), false);
        assert!(result.is_err());
        assert!(!dest_dirs.config_file().exists(), "nothing should be written on decryption failure");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let src_dir = tempfile::tempdir().unwrap();
        let dirs = SingleDirs::from_root(src_dir.path().to_path_buf());
        std::fs::write(dirs.config_file(), "answer = 42\n").unwrap();

        let archive_path = tempfile::tempdir().unwrap().path().join("backup.age");
        let passphrase = SecretString::from("dry-run-pass".to_string());
        export(&dirs, &archive_path, &passphrase).unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let dest_dirs = SingleDirs::from_root(dest_dir.path().to_path_buf());
        let report = import(&dest_dirs, &archive_path, &passphrase, true).unwrap();
        assert!(report.dry_run);
        assert!(!report.files.is_empty());
        assert!(!dest_dirs.config_file().exists(), "dry run must not write any file");
    }

    #[test]
    fn importing_onto_an_existing_file_backs_it_up_first() {
        let src_dir = tempfile::tempdir().unwrap();
        let dirs = SingleDirs::from_root(src_dir.path().to_path_buf());
        std::fs::write(dirs.config_file(), "from-backup = true\n").unwrap();

        let archive_path = tempfile::tempdir().unwrap().path().join("backup.age");
        let passphrase = SecretString::from("existing-file-pass".to_string());
        export(&dirs, &archive_path, &passphrase).unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let dest_dirs = SingleDirs::from_root(dest_dir.path().to_path_buf());
        std::fs::create_dir_all(dest_dirs.root()).unwrap();
        std::fs::write(dest_dirs.config_file(), "already-here = true\n").unwrap();

        import(&dest_dirs, &archive_path, &passphrase, false).unwrap();
        assert_eq!(std::fs::read_to_string(dest_dirs.config_file()).unwrap(), "from-backup = true\n");
        let backups: Vec<_> = std::fs::read_dir(dest_dirs.root())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups.len(), 1, "the pre-existing config.toml should have been backed up before being overwritten");
    }
}
