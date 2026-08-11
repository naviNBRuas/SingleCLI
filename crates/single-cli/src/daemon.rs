//! Spawns `single-runtimed` as a detached background process if the socket
//! isn't already live, so the TUI gets a real daemon to talk to over IPC
//! (per spec section 3: the runtime is headless, the TUI is one client).
//! One-shot CLI commands don't need this — see `client.rs`'s in-process
//! fallback — this is only exercised by the no-subcommand TUI path.

use anyhow::{Context, Result};
use single_core::SingleDirs;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub fn ensure_running(dirs: &SingleDirs) -> Result<()> {
    let socket_path = dirs.socket_path();
    if UnixStream::connect(&socket_path).is_ok() {
        return Ok(());
    }

    let daemon_path = daemon_binary_path()?;
    std::process::Command::new(&daemon_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", daemon_path.display()))?;

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if UnixStream::connect(&socket_path).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!(
        "single-runtimed did not come up at {} within 3s; run it manually with `single-runtimed`",
        socket_path.display()
    )
}

fn daemon_binary_path() -> Result<std::path::PathBuf> {
    let current_exe = std::env::current_exe().context("resolving current executable path")?;
    let dir = current_exe.parent().context("executable has no parent directory")?;
    let candidate = dir.join("single-runtimed");
    if candidate.exists() {
        return Ok(candidate);
    }
    // Fall back to $PATH lookup if not installed alongside `single` (e.g. `cargo run`).
    Ok(std::path::PathBuf::from("single-runtimed"))
}
