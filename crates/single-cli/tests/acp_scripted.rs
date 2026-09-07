//! Scripted ACP client for `single acp` (spec E27.02 §7 / §10 "messenger"
//! row). `#[ignore]` by default: it drives the real binary and, for the
//! goal path, needs a running `single-runtimed` with at least one usable
//! agent. The `initialize` / `session/new` / `/status` legs work against
//! the in-process fallback with no daemon.
//!
//! Run: `cargo test -p single-cli --test acp_scripted -- --ignored --nocapture`

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

struct AcpProc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl AcpProc {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_single"))
            .arg("acp")
            .env("SINGLE_ACP_LOG", "/dev/null")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn single acp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdin, stdout }
    }

    fn send(&mut self, v: serde_json::Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// reads lines until one parses as JSON with `id == want_id`.
    fn wait_response(&mut self, want_id: i64) -> serde_json::Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            assert!(std::time::Instant::now() < deadline, "timed out waiting for id {want_id}");
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).unwrap();
            assert!(n > 0, "acp closed stdout before id {want_id}");
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else { continue };
            if v.get("id").and_then(|x| x.as_i64()) == Some(want_id) && v.get("method").is_none() {
                return v;
            }
        }
    }
}

impl Drop for AcpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "drives the real binary; goal leg needs a running daemon + agent"]
fn initialize_new_session_and_status_slash() {
    let mut acp = AcpProc::spawn();

    acp.send(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    let init = acp.wait_response(1);
    assert_eq!(init["result"]["protocolVersion"], 1);
    assert_eq!(init["result"]["agentCapabilities"]["loadSession"], true);

    acp.send(serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new",
        "params": { "cwd": std::env::temp_dir().to_str().unwrap() }
    }));
    let new = acp.wait_response(2);
    let sid = new["result"]["sessionId"].as_str().expect("sessionId").to_string();
    let modes: Vec<_> = new["result"]["modes"]["availableModes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(modes, ["auto", "plan", "careful", "dry"]);

    // a slash prompt answers from the socket, burns no agent, ends the turn.
    acp.send(serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": { "sessionId": sid, "prompt": [ { "type": "text", "text": "/status" } ] }
    }));
    let turn = acp.wait_response(3);
    assert_eq!(turn["result"]["stopReason"], "end_turn");
}

#[test]
#[ignore = "needs a running daemon with a usable planning agent"]
fn goal_prompt_streams_plan_and_terminal_status() {
    let mut acp = AcpProc::spawn();
    acp.send(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    acp.wait_response(1);
    acp.send(serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/new",
        "params": { "cwd": std::env::temp_dir().to_str().unwrap() }
    }));
    let sid = acp.wait_response(2)["result"]["sessionId"].as_str().unwrap().to_string();

    acp.send(serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": { "sessionId": sid, "prompt": [
            { "type": "text", "text": "print the single word OK to a file called ok.txt" }
        ] }
    }));
    // the turn eventually ends (done / failed / blocked all resolve to a
    // stopReason); wait_response tolerates the interleaved session/update
    // notifications.
    let turn = acp.wait_response(3);
    assert!(turn["result"]["stopReason"].is_string());
}
