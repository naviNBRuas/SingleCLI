//! `single serve --openai` — an OpenAI-compatible HTTP proxy over the
//! SingleCLI pool (E27.01 backlog #2 / Zed backlog #2). Lets Zed's inline
//! assistant / edit-prediction / commit-message model run on the pool via
//! `language_models.openai_compatible`.
//!
//! Deliberately tiny: a hand-rolled HTTP/1.1 server on `std::net` (no new
//! dependency), one thread per connection. Each `/v1/chat/completions`
//! flattens the messages into a prompt, picks an agent (the `model` name
//! if it's a real agent, else routed like the coordinator's `code/quick`
//! kind), and runs one `Request::TaskRun { allow_fallback: true }` against
//! the daemon — so a 429 already hops the fallback chain inside the
//! runtime. `task::run` is one-shot, so `stream: true` is honoured as a
//! single content chunk followed by `[DONE]`, not real per-token SSE.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use single_protocol::{Request, Response, ResponseData, TaskRecord, TaskStatus};
use single_runtime::coordinator::graph::{Effort, NodeKind};
use single_runtime::coordinator::routing::{self, PoolHealth, RoutingTable};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Config {
    pub socket_path: PathBuf,
    pub addr: String,
    /// Forced agent for every request (overrides routing). `None` → route
    /// per request when the model name isn't itself an agent.
    pub agent: Option<String>,
    pub timeout_secs: u64,
}

pub fn run(cfg: Config) -> Result<()> {
    let ctx = single_runtime::Context::load().context("loading SingleCLI context")?;
    let listener = TcpListener::bind(&cfg.addr).with_context(|| format!("binding {}", cfg.addr))?;
    eprintln!("single serve --openai listening on http://{}/v1  (Ctrl-C to stop)", cfg.addr);

    let cfg = std::sync::Arc::new(cfg);
    let ctx = std::sync::Arc::new(ctx);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let cfg = std::sync::Arc::clone(&cfg);
        let ctx = std::sync::Arc::clone(&ctx);
        std::thread::spawn(move || {
            if let Err(e) = handle_conn(stream, &cfg, &ctx) {
                eprintln!("[serve] connection error: {e:#}");
            }
        });
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

fn read_request(stream: &TcpStream) -> Result<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(HttpRequest { method, path, body: String::from_utf8_lossy(&body).into_owned() })
}

fn handle_conn(mut stream: TcpStream, cfg: &Config, ctx: &single_runtime::Context) -> Result<()> {
    let req = read_request(&stream)?;
    let path = req.path.split('?').next().unwrap_or(&req.path);

    match (req.method.as_str(), path) {
        ("GET", "/health") => write_json(&mut stream, 200, &json!({ "ok": true })),
        ("GET", "/v1/models") => write_json(&mut stream, 200, &models_body(ctx)),
        ("POST", "/v1/chat/completions") => chat_completions(&mut stream, &req.body, cfg, ctx),
        _ => write_json(
            &mut stream,
            404,
            &json!({ "error": { "message": format!("no route for {} {}", req.method, path), "type": "invalid_request_error" } }),
        ),
    }
}

fn models_body(ctx: &single_runtime::Context) -> Value {
    let now = unix_now();
    let mut data = vec![json!({ "id": "pool", "object": "model", "created": now, "owned_by": "singlecli" })];
    for a in &ctx.registry {
        data.push(json!({ "id": a.name, "object": "model", "created": now, "owned_by": "singlecli" }));
    }
    json!({ "object": "list", "data": data })
}

fn chat_completions(stream: &mut TcpStream, body: &str, cfg: &Config, ctx: &single_runtime::Context) -> Result<()> {
    let parsed: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return write_json(
                stream,
                400,
                &json!({ "error": { "message": format!("invalid JSON body: {e}"), "type": "invalid_request_error" } }),
            )
        }
    };
    let model = parsed.get("model").and_then(|m| m.as_str()).unwrap_or("pool").to_string();
    let stream_reply = parsed.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let messages = parsed.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    if messages.is_empty() {
        return write_json(
            stream,
            400,
            &json!({ "error": { "message": "`messages` is required and must be non-empty", "type": "invalid_request_error" } }),
        );
    }
    let prompt = flatten_messages(&messages);
    let agent = pick_agent(&model, cfg, ctx);

    let response = crate::client::send(
        &cfg.socket_path,
        Request::TaskRun {
            description: prompt,
            agent: agent.clone(),
            cwd: ".".to_string(),
            use_worktree: false,
            account: None,
            real_home: false,
            no_memory_context: true, // the messages ARE the prompt
            timeout_secs: cfg.timeout_secs,
            background: false,
            allow_fallback: true,
            usage_json: false,
        },
    );

    let rec = match response {
        Ok(Response::Ok { data: ResponseData::Task(rec) }) => rec,
        Ok(Response::Ok { data }) => {
            return write_json(
                stream,
                502,
                &json!({ "error": { "message": format!("unexpected daemon response: {data:?}"), "type": "api_error" } }),
            )
        }
        Ok(Response::Error { message }) => {
            return write_json(stream, 502, &json!({ "error": { "message": message, "type": "api_error" } }))
        }
        Err(e) => {
            return write_json(
                stream,
                502,
                &json!({ "error": { "message": format!("daemon unreachable: {e:#}"), "type": "api_error" } }),
            )
        }
    };

    let content = task_content(&rec);
    let finish = if rec.timed_out {
        "length"
    } else if rec.status == TaskStatus::Completed {
        "stop"
    } else {
        "stop" // a failed agent still returns text; surface it rather than erroring
    };
    let (pt, ct) = (rec.prompt_tokens.unwrap_or(0).max(0), rec.completion_tokens.unwrap_or(0).max(0));
    let id = format!("chatcmpl-{}", rec.id);

    if stream_reply {
        write_sse(stream, &id, &model, &content, finish)
    } else {
        let payload = json!({
            "id": id,
            "object": "chat.completion",
            "created": unix_now(),
            "model": model,
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content },
                "finish_reason": finish,
            }],
            "usage": { "prompt_tokens": pt, "completion_tokens": ct, "total_tokens": pt + ct },
        });
        write_json(stream, 200, &payload)
    }
}

/// Flattens chat messages into one prompt. System messages become a
/// preamble; the rest are `role: content` lines in order.
pub fn flatten_messages(messages: &[Value]) -> String {
    let text_of = |m: &Value| -> String {
        match m.get("content") {
            Some(Value::String(s)) => s.clone(),
            // OpenAI vision-style content parts: keep the text parts.
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    };
    let mut preamble = Vec::new();
    let mut turns = Vec::new();
    for m in messages {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let text = text_of(m);
        if text.is_empty() {
            continue;
        }
        if role == "system" {
            preamble.push(text);
        } else {
            turns.push(format!("{role}: {text}"));
        }
    }
    let mut out = String::new();
    if !preamble.is_empty() {
        out.push_str(&preamble.join("\n\n"));
        out.push_str("\n\n");
    }
    out.push_str(&turns.join("\n"));
    out
}

/// `model` is used verbatim if it names a real registered agent; `"pool"`
/// or anything unrecognized routes (the `--agent` override wins over
/// routing).
pub fn pick_agent(model: &str, cfg: &Config, ctx: &single_runtime::Context) -> String {
    if ctx.registry.iter().any(|a| a.name == model) {
        return model.to_string();
    }
    if let Some(a) = &cfg.agent {
        return a.clone();
    }
    let table = RoutingTable::load(&ctx.dirs);
    let health = single_runtime::state::open(&ctx.dirs.db_path())
        .map(|conn| PoolHealth::probe(&ctx.registry, &conn))
        .unwrap_or_default();
    routing::select_agent(&table, NodeKind::Code, Effort::Quick, &health)
        .or_else(|| table.fallback_default.first().cloned())
        .unwrap_or_else(|| "opencode".to_string())
}

/// The agent's answer text — the captured artifact if present, else the
/// stored summary.
fn task_content(rec: &TaskRecord) -> String {
    if let Some(p) = &rec.artifact_path {
        if let Ok(s) = std::fs::read_to_string(p) {
            // artifacts are `"<stdout>\n--- stderr ---\n<stderr>"`; keep
            // the stdout half for a clean assistant message.
            return s.split("\n--- stderr ---\n").next().unwrap_or(&s).trim_end().to_string();
        }
    }
    rec.summary.clone().unwrap_or_default()
}

// ---- HTTP writers -------------------------------------------------------

fn write_json(stream: &mut TcpStream, status: u16, body: &Value) -> Result<()> {
    let text = body.to_string();
    let reason = if (200..300).contains(&status) { "OK" } else { "ERROR" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )?;
    stream.flush()?;
    Ok(())
}

/// Single-chunk SSE: one delta with the whole content, then `[DONE]`.
/// Real per-token streaming isn't available (the underlying `task::run`
/// is one-shot).
fn write_sse(stream: &mut TcpStream, id: &str, model: &str, content: &str, finish: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
    )?;
    let created = unix_now();
    let role_chunk = json!({
        "id": id, "object": "chat.completion.chunk", "created": created, "model": model,
        "choices": [{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": null }],
    });
    let text_chunk = json!({
        "id": id, "object": "chat.completion.chunk", "created": created, "model": model,
        "choices": [{ "index": 0, "delta": { "content": content }, "finish_reason": null }],
    });
    let stop_chunk = json!({
        "id": id, "object": "chat.completion.chunk", "created": created, "model": model,
        "choices": [{ "index": 0, "delta": {}, "finish_reason": finish }],
    });
    write!(stream, "data: {role_chunk}\n\n")?;
    write!(stream, "data: {text_chunk}\n\n")?;
    write!(stream, "data: {stop_chunk}\n\n")?;
    write!(stream, "data: [DONE]\n\n")?;
    stream.flush()?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_messages_puts_system_first_then_role_lines() {
        let msgs = vec![
            json!({ "role": "system", "content": "you are terse" }),
            json!({ "role": "user", "content": "hi" }),
            json!({ "role": "assistant", "content": "hello" }),
            json!({ "role": "user", "content": "bye" }),
        ];
        let p = flatten_messages(&msgs);
        assert_eq!(p, "you are terse\n\nuser: hi\nassistant: hello\nuser: bye");
    }

    #[test]
    fn flatten_messages_handles_content_parts_and_skips_empty() {
        let msgs = vec![
            json!({ "role": "user", "content": [ { "type": "text", "text": "part one" }, { "type": "image_url" }, { "type": "text", "text": "part two" } ] }),
            json!({ "role": "assistant", "content": "" }),
        ];
        assert_eq!(flatten_messages(&msgs), "user: part one\npart two");
    }

    #[test]
    fn openai_response_shape_has_the_required_fields() {
        // guards the non-stream payload keys Zed's client checks.
        let payload = json!({
            "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": "pool",
            "choices": [{ "index": 0, "message": { "role": "assistant", "content": "x" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3 },
        });
        assert_eq!(payload["choices"][0]["message"]["role"], "assistant");
        assert_eq!(payload["usage"]["total_tokens"], 3);
        let s: single_protocol::TokenUsage = serde_json::from_value(json!({ "prompt_tokens": 1, "completion_tokens": 2 })).unwrap();
        assert_eq!(s.completion_tokens, 2);
    }
}
