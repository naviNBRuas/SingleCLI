//! Blocking subprocess execution with a wall-clock timeout. `std::process`
//! has no built-in timeout, so this polls `try_wait` and kills the child
//! if it runs past the deadline — the standard approach absent an async
//! runtime here (this runs inside `single-runtime`'s synchronous request
//! handlers, matching how `discover()` already blocks synchronously).

use crate::backend::ExecBackend;
use anyhow::{Context, Result};
use single_protocol::RunOutcome;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Runs `command` attached to the real terminal (inherited stdin/stdout/
/// stderr) with `$HOME` overridden to `home`, and blocks until it exits.
/// No timeout — this is for interactive flows (login prompts, browser
/// OAuth round-trips) that a human is actively driving, unlike
/// `run_command`'s captured-output, timeout-bounded, non-interactive
/// shape. Used only by `AgentAdapter::login`.
pub fn run_interactive_with_home(command: &str, args: &[String], home: &Path) -> Result<()> {
    let mut cmd = Command::new(command);
    cmd.args(args).current_dir(home).env("HOME", home);
    pin_real_config_dir(&mut cmd);
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawning {command}"))?;
    if !status.success() {
        anyhow::bail!("{command} exited with {status}");
    }
    Ok(())
}

pub fn run_command(command: &str, args: &[String], cwd: &Path, timeout: Duration) -> Result<RunOutcome> {
    run_command_with_home(command, args, cwd, None, timeout)
}

/// Same as `run_command`, but when `home` is set, overrides `$HOME` for the
/// child process. This is what lets multiple isolated accounts of the same
/// agent CLI run concurrently without stepping on each other's live
/// credentials/session state — each account gets its own materialized HOME
/// directory (see `single-core::account::ensure_isolated_home`), and the
/// CLI reads/writes its config relative to whatever `$HOME` it sees, same
/// as any other well-behaved Unix program. Always runs on the host — used
/// by `install_plugin`/`login`, which aren't part of the opt-in Docker
/// backend (see `backend::ExecBackend`'s doc comment for why that's
/// scoped to `run_prompt` only).
pub fn run_command_with_home(command: &str, args: &[String], cwd: &Path, home: Option<&Path>, timeout: Duration) -> Result<RunOutcome> {
    run_command_live(command, args, cwd, &ExecBackend::host(home), None, timeout)
}

/// Same as `run_command_with_home`, but when `live_output_path` is set,
/// stdout/stderr are tee'd to that file **as the process produces them**
/// (both streams interleaved, line-buffered), not just captured and
/// returned after the process exits. This is what lets a concurrent
/// caller — the TUI's task-detail view, polling on its own connection —
/// watch a long-running agent invocation while it's still in flight,
/// instead of only seeing output once the whole thing finishes. Reading
/// happens on two background threads (one per stream) so the child's
/// pipes are drained continuously; the previous read-after-wait shape
/// risked the child blocking on a full pipe buffer for chatty output,
/// which this also incidentally fixes.
pub fn run_command_live(
    command: &str,
    args: &[String],
    cwd: &Path,
    backend: &ExecBackend,
    live_output_path: Option<&Path>,
    timeout: Duration,
) -> Result<RunOutcome> {
    let start = Instant::now();
    let mut cmd = match backend {
        ExecBackend::Host { home } => {
            let mut cmd = Command::new(command);
            cmd.args(args).current_dir(cwd);
            if let Some(home) = home {
                cmd.env("HOME", home);
                pin_real_config_dir(&mut cmd);
            }
            cmd
        }
        ExecBackend::Docker { container, workdir } => {
            // No $HOME override, no current_dir: the container's home was
            // fixed when it was created (the isolated home bind-mounted
            // in — see single-runtime::docker::ensure_started), and
            // `docker exec -w` sets the working directory inside the
            // container, not this host process's cwd.
            let mut cmd = Command::new("docker");
            cmd.arg("exec").arg("-w").arg(workdir).arg(container).arg(command).args(args);
            cmd
        }
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("spawning {command}"))?;

    let tee_file: Option<Arc<Mutex<std::fs::File>>> = live_output_path
        .and_then(|p| std::fs::File::create(p).ok())
        .map(|f| Arc::new(Mutex::new(f)));

    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));

    let stdout_handle = child.stdout.take().map(|pipe| {
        let buf = Arc::clone(&stdout_buf);
        let tee = tee_file.clone();
        std::thread::spawn(move || drain_into(pipe, buf, tee))
    });
    let stderr_handle = child.stderr.take().map(|pipe| {
        let buf = Arc::clone(&stderr_buf);
        let tee = tee_file.clone();
        std::thread::spawn(move || drain_into(pipe, buf, tee))
    });

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

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    let exit_code = child.wait().ok().and_then(|s| s.code());
    let success = !timed_out && exit_code == Some(0);
    let stdout = Arc::try_unwrap(stdout_buf).map(|m| m.into_inner().unwrap_or_default()).unwrap_or_default();
    let stderr = Arc::try_unwrap(stderr_buf).map(|m| m.into_inner().unwrap_or_default()).unwrap_or_default();

    Ok(RunOutcome {
        success,
        stdout,
        stderr,
        exit_code,
        timed_out,
        duration_ms: start.elapsed().as_millis(),
    })
}

/// When a child's `$HOME` is overridden for isolation, its own view of
/// `directories::BaseDirs` (and therefore `SingleDirs::discover()`) would
/// otherwise resolve `~/.config/single` under the *isolated* home instead
/// of the real one. That breaks any nested `single` invocation the child
/// makes on its own — notably Claude Code's `PreToolUse` hook, which shells
/// back out to `single internal claude-pretooluse-hook` — silently
/// splitting its permission rules/preferences/pending-approvals into a
/// second, invisible store nobody's `single approval list` or the TUI ever
/// looks at. Pinning `SINGLE_CONFIG_DIR` to the resolving process's own
/// (real) config root keeps every nested `single` call pointed at the same
/// central state regardless of what `$HOME` the child sees.
fn pin_real_config_dir(cmd: &mut Command) {
    if let Ok(dirs) = single_core::paths::SingleDirs::discover() {
        cmd.env("SINGLE_CONFIG_DIR", dirs.root());
    }
}

/// Reads `pipe` line by line, accumulating into `buf` and — when `tee` is
/// set — appending each line to the shared file immediately (flushed, so
/// a concurrent reader of that file sees it right away).
fn drain_into(pipe: impl Read, buf: Arc<Mutex<String>>, tee: Option<Arc<Mutex<std::fs::File>>>) {
    let reader = BufReader::new(pipe);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if let Some(tee) = &tee {
            if let Ok(mut f) = tee.lock() {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
        }
        if let Ok(mut b) = buf.lock() {
            b.push_str(&line);
            b.push('\n');
        }
    }
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
    fn overriding_home_also_pins_single_config_dir_for_the_child() {
        // Regression test: a child spawned with $HOME overridden (agent
        // isolation) must still see the *real* SINGLE_CONFIG_DIR, or any
        // nested `single` invocation it makes on its own (e.g. Claude
        // Code's PreToolUse hook shelling back into `single internal
        // claude-pretooluse-hook`) would resolve config/state under the
        // isolated home instead of the real central one — splitting
        // pending approvals/preferences into a store nobody ever looks at.
        let dir = tempfile::tempdir().unwrap();
        let fake_home = tempfile::tempdir().unwrap();
        let outcome = run_command_with_home(
            "sh",
            &["-c".into(), "echo \"$SINGLE_CONFIG_DIR\"".into()],
            dir.path(),
            Some(fake_home.path()),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(outcome.success);
        let expected = single_core::paths::SingleDirs::discover().unwrap().root().to_string_lossy().into_owned();
        assert_eq!(outcome.stdout.trim(), expected);
    }

    #[test]
    fn nonzero_exit_is_not_success() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_command("sh", &["-c".into(), "exit 3".into()], dir.path(), Duration::from_secs(5)).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[test]
    fn live_output_path_tees_the_same_content_as_the_captured_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let live_path = dir.path().join("live.txt");
        let outcome = run_command_live(
            "sh",
            &["-c".into(), "echo first; sleep 0.3; echo second".into()],
            dir.path(),
            &ExecBackend::host(None),
            Some(&live_path),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.stdout, "first\nsecond\n");
        assert_eq!(std::fs::read_to_string(&live_path).unwrap(), "first\nsecond\n");
    }

    #[test]
    fn kills_process_that_exceeds_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_command("sh", &["-c".into(), "sleep 5".into()], dir.path(), Duration::from_millis(200)).unwrap();
        assert!(outcome.timed_out);
        assert!(!outcome.success);
    }
}
