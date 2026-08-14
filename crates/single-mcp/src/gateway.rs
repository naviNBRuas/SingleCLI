//! The gateway `ServerHandler`: exposes exactly two tools
//! (`list_available_mcp_tools`, `invoke_mcp`) over stdio to whichever agent
//! CLI has this binary registered as its one MCP server, and lazily
//! proxies to the real MCP servers listed in SingleCLI's `mcp.toml`
//! registry — spawned as child processes only on first use, then reused
//! for the rest of this gateway process's lifetime (which is exactly the
//! agent session's lifetime, since the agent's own MCP client keeps this
//! stdio child alive for as long as it's talking to it). This deliberately
//! holds no state in `single-runtimed` itself — the statefulness lives
//! here, in a process the agent already owns.
//!
//! Discovery is two-step by design, not "list every tool from every
//! server upfront": `list_available_mcp_tools` returns only the *servers*
//! in the registry (name/command, no process spawned) — actually listing
//! one server's own tools means spawning it, which `invoke_mcp` does when
//! called with `tool: null`. This is a smaller, honest scope: probing
//! every registered server just to build an upfront catalog would spawn
//! exactly the N processes this design exists to avoid.

use anyhow::{Context, Result};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData as McpError, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

type ChildSession = rmcp::service::RunningService<RoleClient, ()>;

pub struct Gateway {
    sessions: Mutex<HashMap<String, Arc<ChildSession>>>,
}

impl Gateway {
    pub fn new() -> Self {
        Gateway { sessions: Mutex::new(HashMap::new()) }
    }

    /// Registry servers, honoring `enabled` the same way the rest of
    /// SingleCLI does (`single mcp list` shows both, but only enabled
    /// servers are meant to be synced/used).
    fn registry() -> Result<Vec<single_protocol::McpServerSpec>> {
        let dirs = single_core::SingleDirs::discover().context("resolving SingleCLI config directory")?;
        let servers = single_core::mcp::load(&dirs.mcp_registry_file())?;
        Ok(servers.into_iter().filter(|s| s.enabled).collect())
    }

    async fn ensure_session(&self, name: &str) -> Result<Arc<ChildSession>> {
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(name) {
            return Ok(existing.clone());
        }
        let servers = Self::registry()?;
        let spec = servers.into_iter().find(|s| s.name == name).with_context(|| {
            format!("no enabled mcp server named '{name}' (see `single mcp list`; disabled servers must be enabled first)")
        })?;

        let mut command = tokio::process::Command::new(&spec.command);
        command.args(&spec.args);
        for (k, v) in &spec.env {
            command.env(k, v);
        }
        // Secret-backed env vars are resolved here, at spawn time, and set
        // directly on the child's environment — never written to mcp.toml
        // or any other file. This is the one path in SingleCLI that
        // actually honors McpServerSpec::secret_env (see its doc comment).
        let secret_store = single_core::secrets::SecretTool;
        for (env_var, secret_key) in &spec.secret_env {
            let value = single_core::secrets::SecretStore::get(&secret_store, secret_key)?
                .with_context(|| format!("mcp server '{name}' needs secret '{secret_key}' (set with `single secret set {secret_key} <value>`)"))?;
            command.env(env_var, value);
        }
        let transport = TokioChildProcess::new(command.configure(|_| {})).with_context(|| format!("spawning mcp server '{name}'"))?;
        let session = ().serve(transport).await.with_context(|| format!("initializing mcp server '{name}'"))?;
        let session = Arc::new(session);
        sessions.insert(name.to_string(), session.clone());
        Ok(session)
    }

    async fn list_available_mcp_tools(&self) -> Result<Value> {
        let servers = Self::registry()?;
        let catalog: Vec<Value> = servers
            .into_iter()
            .map(|s| json!({ "server": s.name, "command": format!("{} {}", s.command, s.args.join(" ")).trim() }))
            .collect();
        Ok(json!({
            "servers": catalog,
            "note": "each entry is a registered MCP server, not yet its individual tools \
                     (that would mean spawning every one up front) — call invoke_mcp with \
                     this server name and tool: null to lazily spawn it and list its real tools."
        }))
    }

    async fn invoke_mcp(&self, arguments: &Map<String, Value>) -> Result<Value> {
        let server = arguments.get("server").and_then(Value::as_str).context("invoke_mcp needs a \"server\" argument")?;
        let tool = arguments.get("tool").and_then(Value::as_str);
        let call_args = arguments.get("arguments").and_then(Value::as_object).cloned();

        match tool {
            None => {
                // Listing a server's own tools is read-only discovery, not
                // an action with consequences — not gated by permissions.
                let session = self.ensure_session(server).await?;
                let tools = session.list_all_tools().await.with_context(|| format!("listing tools on mcp server '{server}'"))?;
                Ok(json!({ "server": server, "tools": tools }))
            }
            Some(tool_name) => {
                // Checked before spawning anything: a denied/pending call
                // shouldn't pay for (or trigger) starting the underlying
                // server process at all.
                let resource = format!("mcp:{server}:{tool_name}");
                match check_permission(&resource)? {
                    single_core::preferences::Verdict::Deny => {
                        return Ok(json!({
                            "server": server, "tool": tool_name, "denied": true,
                            "reason": "blocked by permission policy (permissions.toml or a learned preference)"
                        }));
                    }
                    single_core::preferences::Verdict::PendingApproval(id) => {
                        return Ok(json!({
                            "server": server, "tool": tool_name, "pending_approval": id,
                            "message": format!(
                                "this action needs your approval first — run `single approval resolve {id} --allow` (or --deny), then retry"
                            )
                        }));
                    }
                    single_core::preferences::Verdict::Allow => {}
                }

                let session = self.ensure_session(server).await?;
                let mut params = CallToolRequestParams::new(tool_name.to_string());
                if let Some(args) = call_args {
                    params = params.with_arguments(args);
                }
                let result = session.call_tool(params).await.with_context(|| format!("calling '{tool_name}' on mcp server '{server}'"))?;
                Ok(json!({ "server": server, "tool": tool_name, "result": result }))
            }
        }
    }
}

/// The real first caller of `permissions::evaluate` (via
/// `preferences::evaluate_and_learn`) — every `invoke_mcp` tool call is
/// checked against `permissions.toml`'s `tools` rules, then learned
/// preferences, before being proxied. Opens the same SQLite db the daemon
/// uses directly (see this file's module doc for why) rather than
/// round-tripping through the socket.
fn check_permission(resource: &str) -> Result<single_core::preferences::Verdict> {
    let dirs = single_core::SingleDirs::discover().context("resolving SingleCLI config directory")?;
    let rules = single_core::permissions::load(&dirs.permissions_file())?;
    let db_path = dirs.db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let conn = rusqlite::Connection::open(&db_path).with_context(|| format!("opening {}", db_path.display()))?;
    single_core::preferences::ensure_schema(&conn)?;
    single_core::preferences::evaluate_and_learn(&rules.tools, &conn, resource, Some("single-mcp invoke_mcp"))
}

fn schema(fields: Value) -> Arc<Map<String, Value>> {
    let Value::Object(map) = fields else { unreachable!("schema() is always called with a json!({{...}}) object literal") };
    Arc::new(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_only_returns_enabled_servers() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let dirs = single_core::SingleDirs::discover().unwrap();
        // default_servers() ships a mix of enabled (git, memory, fetch,
        // sequential-thinking) and disabled (filesystem, github, ...) —
        // registry() must only surface the enabled ones.
        single_core::mcp::save(&dirs.mcp_registry_file(), &single_core::mcp::default_servers()).unwrap();

        let servers = Gateway::registry().unwrap();
        assert!(servers.iter().all(|s| s.enabled));
        assert!(servers.iter().any(|s| s.name == "git"));
        assert!(!servers.iter().any(|s| s.name == "filesystem"), "disabled servers must not appear");
    }

    #[test]
    fn schema_extracts_the_object_from_a_json_literal() {
        let obj = schema(json!({ "type": "object", "properties": {} }));
        assert_eq!(obj.get("type").and_then(Value::as_str), Some("object"));
    }
}

impl ServerHandler for Gateway {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("single-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Dynamic MCP gateway for SingleCLI. Call list_available_mcp_tools first to see \
                 registered servers, then invoke_mcp with {server, tool: null} to discover that \
                 server's real tools, then invoke_mcp with {server, tool, arguments} to call one.",
            )
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(vec![
            Tool::new(
                "list_available_mcp_tools",
                "Lists MCP servers SingleCLI has registered and enabled, without spawning any of them.",
                schema(json!({ "type": "object", "properties": {}, "additionalProperties": false })),
            ),
            Tool::new(
                "invoke_mcp",
                "Lazily spawns (or reuses) a registered MCP server and either lists its tools \
                 (tool: null) or calls one of them (tool: <name>, arguments: {...}).",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "server": { "type": "string", "description": "Registered server name, from list_available_mcp_tools." },
                        "tool": { "type": "string", "description": "Tool name to call; omit to list this server's tools instead." },
                        "arguments": { "type": "object", "description": "Arguments for the tool call, if any." }
                    },
                    "required": ["server"],
                    "additionalProperties": false
                })),
            ),
        ]))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let empty = Map::new();
        let arguments = request.arguments.as_ref().unwrap_or(&empty);
        let result = match request.name.as_ref() {
            "list_available_mcp_tools" => self.list_available_mcp_tools().await,
            "invoke_mcp" => self.invoke_mcp(arguments).await,
            other => Err(anyhow::anyhow!("unknown tool: {other}")),
        };
        match result {
            Ok(value) => Ok(CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
            )])
            .into()),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))]).into()),
        }
    }
}
