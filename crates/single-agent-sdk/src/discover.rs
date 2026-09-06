//! Real detection of an agent CLI on `$PATH` — shells out to the same
//! commands used during manual investigation (`which`, `<cmd> --version`).
//! No capability is inferred here beyond "is it installed and what version
//! does it report."
//!
//! Both spawns are bounded by a short timeout: some agent CLIs never exit
//! on `--version` (they wait on stdin, or `--version` isn't wired and they
//! drop into a REPL). Without a bound, one hung probe blocks the parallel
//! `AgentList` / `Status` / `doctor` fan-out forever and, run twice,
//! leaks enough stuck child processes to OOM the daemon.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Upper bound for `<cmd> --version`. Real CLIs answer in well under a
/// second; anything past this is a hang, not a slow start.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// `which` is a cheap builtin-like lookup; give it very little rope.
const WHICH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub struct Discovery {
    pub detected: bool,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
}

/// Run `cmd` to completion, or kill it and return `None` after `timeout`.
/// stdin is closed so a well-behaved child sees EOF immediately; a
/// misbehaving one is force-killed. Output is small (`--version`), so
/// reading after exit rather than concurrently cannot deadlock here.
fn output_within(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_end(&mut stdout);
                }
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_end(&mut stderr);
                }
                return Some(Output { status, stdout, stderr });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => return None,
        }
    }
}

pub fn discover(command: &str) -> Discovery {
    let which = output_within(Command::new("which").arg(command), WHICH_TIMEOUT);
    let resolved_path = match which {
        Some(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() { None } else { Some(path) }
        }
        _ => None,
    };

    if resolved_path.is_none() {
        return Discovery { detected: false, resolved_path: None, version: None };
    }

    let version = output_within(Command::new(command).arg("--version"), VERSION_TIMEOUT)
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty());

    Discovery { detected: true, resolved_path, version }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_command_is_not_detected() {
        let d = discover("single-cli-definitely-does-not-exist-xyz");
        assert!(!d.detected);
        assert!(d.version.is_none());
    }

    #[test]
    fn a_hanging_version_probe_is_killed_and_returns_no_version() {
        // `sleep 60` stands in for a CLI that never exits on `--version`.
        // `which sleep` succeeds, so `detected` is true, but the version
        // probe must time out fast rather than hang the test.
        let start = Instant::now();
        let out = output_within(Command::new("sleep").arg("60"), Duration::from_millis(300));
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
