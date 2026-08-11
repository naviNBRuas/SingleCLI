//! Talks to the runtime over its Unix socket when one is already running;
//! otherwise falls back to calling straight into `single-runtime` in the
//! current process. This keeps one-shot CLI commands (`single doctor`,
//! `single agent list`, ...) working on a fresh install where no daemon has
//! ever been started, without requiring the CLI to manage a background
//! process lifecycle in Phase 1. The TUI, which genuinely wants a live
//! socket connection, spawns the daemon explicitly instead (see
//! `main.rs::launch_tui`).

use anyhow::Result;
use single_protocol::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub fn send(socket_path: &Path, request: Request) -> Result<Response> {
    match try_socket(socket_path, &request) {
        Ok(response) => Ok(response),
        Err(_) => {
            let ctx = single_runtime::Context::load()?;
            Ok(single_runtime::handle(&ctx, request))
        }
    }
}

fn try_socket(socket_path: &Path, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut payload = serde_json::to_string(request)?;
    payload.push('\n');
    stream.write_all(payload.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim_end())?)
}
