//! Blocking subprocess execution with a wall-clock timeout. `std::process`
//! has no built-in timeout, so this polls `try_wait` and kills the child
//! if it runs past the deadline — the standard approach absent an async
//! runtime here (this runs inside `single-runtime`'s synchronous request
//! handlers, matching how `discover()` already blocks synchronously).

use anyhow::{Context, Result};
use single_protocol::RunOutcome;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn run_command(command: &str, args: &[String], cwd: &Path, timeout: Duration) -> Result<RunOutcome> {
    let start = Instant::now();
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {command}"))?;

    let deadline = start + timeout;
    let timed_out = loop {
        match child.try_wait().context("polling child process")? {
            Some(_) => break false,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

    let exit_code = child.wait().ok().and_then(|s| s.code());
    let success = !timed_out && exit_code == Some(0);

    Ok(RunOutcome {
        success,
        stdout,
        stderr,
        exit_code,
        timed_out,
        duration_ms: start.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_exit_code_of_a_real_process() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_command("sh", &["-c".into(), "echo hello".into()], dir.path(), Duration::from_secs(5)).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.stdout.trim(), "hello");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(!outcome.timed_out);
    }

    #[test]
    fn nonzero_exit_is_not_success() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_command("sh", &["-c".into(), "exit 3".into()], dir.path(), Duration::from_secs(5)).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[test]
    fn kills_process_that_exceeds_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_command("sh", &["-c".into(), "sleep 5".into()], dir.path(), Duration::from_millis(200)).unwrap();
        assert!(outcome.timed_out);
        assert!(!outcome.success);
    }
}
