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
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Upper bound for `<cmd> --version`. Real CLIs answer in well under a
/// second; anything past this is a hang, not a slow start.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// `which` is a cheap builtin-like lookup; give it very little rope.
const WHICH_TIMEOUT: Duration = Duration::from_secs(3);
/// Per-stream cap on captured probe output. A `--version` string is a
/// handful of bytes; a misbehaving CLI that dumps its help text or a REPL
/// banner instead must not be buffered without bound (30 of those at once
/// is real memory on the daemon). Confirmed cause of the RSS spike, along
/// with `MAX_CONCURRENT_PROBES` below.
const MAX_PROBE_OUTPUT: u64 = 64 * 1024;
/// Global ceiling on concurrent probes. Every probe spawns up to two
/// children (`which`, then `<cmd> --version`); most agent CLIs are
/// node/bun bundles that fault in a ~100 MB runtime just to print a
/// version. The daemon fans `discover()` out across ~30 agents at once for
/// `Status` / `AgentList` / `doctor` — uncapped that is ~30 runtimes
/// resident together, which is what drove `single-runtimed` to ~2.6 GB.
/// 4 keeps the fan-out moving without the pile-up.
const MAX_CONCURRENT_PROBES: usize = 4;

/// (in-flight count, wakeup). Shared by every `discover()` caller in the
/// process, so the cap holds whether probes come from `doctor`'s
/// sequential loop or the handlers' per-agent thread fan-out.
fn probe_gate() -> &'static (Mutex<usize>, Condvar) {
    static GATE: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();
    GATE.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

/// RAII permit: blocks until a probe slot is free, frees it on drop
/// (including panic-unwind), so a killed or panicking probe never leaks a
/// slot.
struct ProbePermit;

impl ProbePermit {
    fn acquire() -> Self {
        let (lock, cvar) = probe_gate();
        let mut n = lock.lock().unwrap();
        while *n >= MAX_CONCURRENT_PROBES {
            n = cvar.wait(n).unwrap();
        }
        *n += 1;
        ProbePermit
    }
}

impl Drop for ProbePermit {
    fn drop(&mut self) {
        let (lock, cvar) = probe_gate();
        *lock.lock().unwrap() -= 1;
        cvar.notify_one();
    }
}

/// Read at most `MAX_PROBE_OUTPUT` bytes from a finished child's pipe.
/// Reading after exit can't deadlock (see `output_within`), and the cap
/// only matters for a CLI that ignored `--version` and printed a wall of
/// text.
fn read_capped(mut pipe: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = pipe.by_ref().take(MAX_PROBE_OUTPUT).read_to_end(&mut buf);
    buf
}

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
    let _permit = ProbePermit::acquire();
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
                let stdout = child.stdout.take().map(read_capped).unwrap_or_default();
                let stderr = child.stderr.take().map(read_capped).unwrap_or_default();
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
            // `try_wait` itself failed (unusual). Still reap the child we
            // spawned — dropping the handle would leave a zombie until the
            // daemon exits.
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
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

    #[test]
    fn a_killed_probe_reaps_its_child_and_returns_promptly() {
        // A timed-out probe must `wait()` the child it killed, not just
        // drop the handle — a leaked zombie per probe is the accumulation
        // that OOM'd the daemon. We can't see the pid from here, but the
        // reap is synchronous inside `output_within`, so a fast return
        // means the `kill` + `wait` both completed.
        let start = Instant::now();
        let out = output_within(Command::new("sleep").arg("30"), Duration::from_millis(200));
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn probe_output_is_captured_but_bounded() {
        // Normal `--version`-sized output passes through intact...
        let out = output_within(Command::new("sh").args(["-c", "printf hello"]), Duration::from_secs(5))
            .expect("sh exits promptly");
        assert_eq!(out.stdout, b"hello");
        // ...and whatever is captured is never more than the cap. (A CLI
        // that streams *more* than a pipe buffer without exiting can't be
        // read pre-exit anyway — it blocks on the full pipe and is killed
        // at the timeout; the cap just bounds the exited-but-chatty case.)
        assert!((out.stdout.len() as u64) <= MAX_PROBE_OUTPUT);
    }
}
