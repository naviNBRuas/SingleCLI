//! Self-update: check GitHub Releases for a newer build and replace the
//! running `single`/`single-runtimed` binaries in place — the same idea
//! as `claude update`/`codex update` (verified real commands on both of
//! those CLIs earlier in this project's own investigation), applied here.
//!
//! Two channels:
//! - **stable** — real semver tags (`vX.Y.Z`), via GitHub's own
//!   `/releases/latest` endpoint (which already skips pre-releases).
//! - **nightly** — a single rolling pre-release tagged `nightly` that
//!   `.github/workflows/nightly.yml` overwrites on every push to `main`,
//!   so `single update --channel nightly` tracks the repo's HEAD rather
//!   than a tagged version. There's no meaningful semver to compare for
//!   this channel, so it's always offered as "available" — re-running it
//!   when already current just re-downloads the same build, which is
//!   harmless, not silently wrong.
//!
//! Platform detection and the `singlecli-<target>.tar.gz` asset naming
//! exactly mirror `install.sh`, so both stay consistent with the same
//! release artifacts.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

const REPO: &str = "naviNBRuas/SingleCLI";

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub asset_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

pub fn detect_target() -> Result<String> {
    let os_name = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        other => bail!("unsupported OS for self-update: {other} (build from source instead)"),
    };
    let arch_name = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        other => bail!("unsupported architecture for self-update: {other} (build from source instead)"),
    };
    Ok(format!("{os_name}-{arch_name}"))
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn check_latest(channel: &str) -> Result<ReleaseInfo> {
    let url = match channel {
        "stable" => format!("https://api.github.com/repos/{REPO}/releases/latest"),
        "nightly" => format!("https://api.github.com/repos/{REPO}/releases/tags/nightly"),
        other => bail!("unknown channel '{other}' (expected 'stable' or 'nightly')"),
    };

    let client = reqwest::blocking::Client::new();
    let release: GhRelease = client
        .get(&url)
        .header("User-Agent", "SingleCLI-update")
        .send()
        .context("querying GitHub releases")?
        .error_for_status()
        .with_context(|| format!("no {channel} release found (has one been published yet?)"))?
        .json()
        .context("parsing GitHub releases response")?;

    let target = detect_target()?;
    let asset_name = format!("singlecli-{target}.tar.gz");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| format!("no asset named {asset_name} in release {}", release.tag_name))?;

    Ok(ReleaseInfo { tag: release.tag_name, asset_url: asset.browser_download_url.clone() })
}

/// `None` return means "can't tell" (e.g. a non-`vX.Y.Z` tag) — callers
/// should treat that as "an update might be available" rather than
/// silently assuming there isn't one.
pub fn is_newer(current: &str, latest_tag: &str) -> Option<bool> {
    let parse = |s: &str| -> Option<(u64, u64, u64)> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some((major, minor, patch))
    };
    let current = parse(current)?;
    let latest = parse(latest_tag)?;
    Some(latest > current)
}

/// Downloads the release asset and atomically replaces `single`/
/// `single-runtimed` next to the currently running executable.
pub fn apply(release: &ReleaseInfo) -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("resolving current executable")?;
    let install_dir = current_exe.parent().context("executable has no parent directory")?.to_path_buf();

    let client = reqwest::blocking::Client::new();
    let bytes = client
        .get(&release.asset_url)
        .header("User-Agent", "SingleCLI-update")
        .send()
        .context("downloading update")?
        .error_for_status()
        .context("update download failed")?
        .bytes()
        .context("reading update download")?;

    let tmp_dir = tempfile::tempdir().context("creating temp dir for update")?;
    let tarball_path = tmp_dir.path().join("update.tar.gz");
    std::fs::write(&tarball_path, &bytes).context("writing downloaded archive")?;

    let status = std::process::Command::new("tar")
        .args(["-xzf"])
        .arg(&tarball_path)
        .args(["-C"])
        .arg(tmp_dir.path())
        .status()
        .context("running tar to extract the update")?;
    if !status.success() {
        bail!("failed to extract update archive (tar exited with {status})");
    }

    let target = detect_target()?;
    let extracted_dir = tmp_dir.path().join(format!("singlecli-{target}"));
    let mut replaced = Vec::new();
    for binary in ["single", "single-runtimed", "single-mcp"] {
        let src = extracted_dir.join(binary);
        if !src.exists() {
            continue;
        }
        let dest = install_dir.join(binary);
        // rename() is atomic but fails across filesystems (tmp dir vs
        // install dir can be on different mounts); fall back to copying.
        // Copying straight onto `dest` would fail with ETXTBSY when it's
        // the binary currently executing this very update (Linux refuses
        // to open-for-write a running executable in place) — copy into a
        // temp file in the *same* directory as `dest` instead, then
        // rename that onto `dest`. That rename is same-filesystem (so
        // atomic) and safe even while `dest` is running, since the
        // running process keeps its already-open inode after the
        // directory entry is replaced.
        if std::fs::rename(&src, &dest).is_err() {
            let tmp_dest = install_dir.join(format!("{binary}.update-tmp"));
            std::fs::copy(&src, &tmp_dest).with_context(|| format!("downloading updated {binary} into place"))?;
            std::fs::rename(&tmp_dest, &dest).with_context(|| format!("installing updated {binary}"))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
        }
        replaced.push(binary);
    }
    if replaced.is_empty() {
        bail!("update archive for {target} didn't contain single or single-runtimed");
    }

    Ok(install_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_detects_a_real_version_bump() {
        assert_eq!(is_newer("0.1.0", "v0.1.1"), Some(true));
        assert_eq!(is_newer("0.1.1", "v0.1.1"), Some(false));
        assert_eq!(is_newer("0.2.0", "v0.1.9"), Some(false));
        assert_eq!(is_newer("0.1.0", "v1.0.0"), Some(true));
    }

    #[test]
    fn is_newer_returns_none_for_unparseable_tags() {
        assert_eq!(is_newer("0.1.0", "nightly"), None);
        assert_eq!(is_newer("0.1.0", "not-a-version"), None);
    }

    #[test]
    fn detect_target_matches_this_machine_and_the_release_asset_naming_scheme() {
        // Not asserting a specific value (that would just hardcode the test
        // runner's platform) — only that it succeeds and produces the
        // `<os>-<arch>` shape install.sh/release.yml both expect.
        let target = detect_target().unwrap();
        assert!(target.contains('-'));
    }
}
