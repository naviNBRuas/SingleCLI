//! Talk to a running `single-runtimed` over its Unix socket; if none is
//! running, fall back to calling straight into `single-runtime` in this
//! process. Deliberately duplicated from `single-cli/src/client.rs`'s
//! socket-or-fallback logic rather than extracted into a shared crate —
//! that file also owns CLI-only concerns (a `--background` warning
//! `eprintln!`) that don't belong in an MCP server's stdout/stderr, which
//! is reserved for the MCP protocol itself; ~30 lines duplicated once is a
//! smaller risk than restructuring `single-cli`'s module boundaries to
//! carve out a piece neither binary's test suite currently exercises
//! independently.

use anyhow::Result;
use single_protocol::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn send(socket_path: &Path, request: Request) -> Result<Response> {
    match UnixStream::connect(socket_path) {
        Ok(stream) => send_over(stream, &request),
        Err(_) => {
            let ctx = single_runtime::Context::load()?;
            Ok(single_runtime::handle(&ctx, request))
        }
    }
}

fn send_over(stream: UnixStream, request: &Request) -> Result<Response> {
    let mut writer = stream.try_clone()?;
    let mut payload = serde_json::to_string(request)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim_end())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_in_process_when_no_daemon_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        // No daemon socket exists at this path — connect() must fail, and
        // send() must fall back to single_runtime::handle rather than
        // erroring out.
        let socket_path = dir.path().join("nonexistent.sock");
        let response = send(&socket_path, Request::Status).unwrap();
        assert!(matches!(response, Response::Ok { .. }));
    }
}
