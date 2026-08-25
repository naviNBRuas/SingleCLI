//! `singlecli-mcp`: exposes SingleCLI's own agent/task/orchestrate/memory/
//! provider commands as MCP tools — see `server.rs`'s module doc.

mod client;
mod server;

use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = server::SingleCliServer::new()?.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
