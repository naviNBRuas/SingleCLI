//! Unix domain socket server: newline-delimited JSON `Request` in,
//! `Response` out. One request per connection is fine for Phase 1's usage
//! (a CLI invocation, or the TUI polling on an interval) — a persistent
//! multiplexed connection with a real event stream is Phase 4 scope.

use crate::context::Context;
use crate::handlers::handle;
use anyhow::{Context as _, Result};
use single_protocol::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

pub async fn serve(socket_path: &std::path::Path) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("removing stale socket {}", socket_path.display()))?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding socket {}", socket_path.display()))?;
    tracing::info!(path = %socket_path.display(), "single-runtime listening");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                tracing::warn!(error = %e, "connection error");
            }
        });
    }
}

async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<Request>(line.trim_end()) {
            Ok(request) => {
                // Context is cheap to load (local file reads) and Phase 1
                // has no long-lived mutable daemon state, so a fresh
                // Context per request keeps config changes picked up live.
                match Context::load() {
                    Ok(ctx) => handle(&ctx, request),
                    Err(e) => Response::Error { message: format!("loading context: {e:#}") },
                }
            }
            Err(e) => Response::Error { message: format!("invalid request: {e}") },
        };
        let mut payload = serde_json::to_string(&response)?;
        payload.push('\n');
        write_half.write_all(payload.as_bytes()).await?;
        line.clear();
    }
    Ok(())
}
