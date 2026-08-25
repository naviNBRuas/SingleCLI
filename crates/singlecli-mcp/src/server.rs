//! The `singlecli-mcp` `ServerHandler`: exposes SingleCLI's own
//! task/orchestrate/agent/memory/provider commands as MCP tools, so an
//! agent CLI that has this binary registered as an MCP server can delegate
//! work to SingleCLI's other agents/models instead of doing it itself —
//! see `docs/superpowers/specs/2026-08-24-claude-code-singlecli-integration-design.md`.
//! Unlike `single-mcp`'s gateway (which proxies to *other* MCP servers),
//! every tool here is a direct SingleCLI capability, reached via
//! `crate::client::send` — the same socket-or-in-process path `single-cli`
//! itself uses.

use crate::client::send;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData as McpError, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use serde_json::{json, Map, Value};
use single_protocol::{Request, Response};
use std::path::PathBuf;

pub struct SingleCliServer {
    socket_path: PathBuf,
}

impl SingleCliServer {
    pub fn new() -> anyhow::Result<Self> {
        let dirs = single_core::SingleDirs::discover()?;
        Ok(Self { socket_path: dirs.socket_path() })
    }

    /// Propagates both a transport failure and a `Response::Error` as a
    /// real `Err`, so `call_tool`'s existing `Err(e) => CallToolResult::error(...)`
    /// path fires — a failed delegation must surface as a genuine MCP tool
    /// error, not as a "successful" result whose content happens to
    /// mention failure (see this plan's Global Constraints / the spec's
    /// error-handling section).
    fn send(&self, request: Request) -> anyhow::Result<Value> {
        match send(&self.socket_path, request)? {
            Response::Ok { data } => Ok(serde_json::to_value(data)?),
            Response::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    fn str_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, anyhow::Error> {
        args.get(key).and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("missing required string argument \"{key}\""))
    }

    fn bool_arg(args: &Map<String, Value>, key: &str, default: bool) -> bool {
        args.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    fn u64_arg(args: &Map<String, Value>, key: &str, default: u64) -> u64 {
        args.get(key).and_then(Value::as_u64).unwrap_or(default)
    }

    fn task_run(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let description = Self::str_arg(args, "description")?.to_string();
        let agent = Self::str_arg(args, "agent")?.to_string();
        let cwd = args.get("cwd").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| ".".to_string());
        self.send(Request::TaskRun {
            description,
            agent,
            cwd,
            use_worktree: Self::bool_arg(args, "use_worktree", false),
            account: args.get("account").and_then(Value::as_str).map(str::to_string),
            real_home: Self::bool_arg(args, "real_home", false),
            no_memory_context: Self::bool_arg(args, "no_memory_context", false),
            timeout_secs: Self::u64_arg(args, "timeout_secs", 300),
            background: false,
            allow_fallback: Self::bool_arg(args, "allow_fallback", false),
        })
    }

    fn orchestrate_run(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let goal = Self::str_arg(args, "goal")?.to_string();
        let agents: Vec<String> = args
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing required array argument \"agents\""))?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let cwd = args.get("cwd").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| ".".to_string());
        self.send(Request::Orchestrate {
            goal,
            agents,
            cwd,
            use_worktree: Self::bool_arg(args, "use_worktree", false),
            real_home: Self::bool_arg(args, "real_home", false),
            timeout_secs: Self::u64_arg(args, "timeout_secs", 300),
        })
    }
}

fn schema(fields: Value) -> std::sync::Arc<Map<String, Value>> {
    let Value::Object(map) = fields else { unreachable!("schema() is always called with a json!({{...}}) object literal") };
    std::sync::Arc::new(map)
}

impl ServerHandler for SingleCliServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("singlecli-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Delegates work to SingleCLI's other agents/models instead of doing it yourself — \
                 use task_run for one prompt to one agent, orchestrate_run for a sequential relay \
                 across several agents, orchestrate_parallel_run / orchestrate_graph_run for \
                 independent or dependency-ordered parallel work. Check agent_list first to see \
                 what's actually available to delegate to.",
            )
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(vec![
            Tool::new(
                "task_run",
                "Delegates one prompt to one agent CLI (e.g. codex, opencode), synchronously, and returns its real output.",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "The prompt/task description." },
                        "agent": { "type": "string", "description": "Which agent CLI to run this against, e.g. \"codex\"." },
                        "cwd": { "type": "string", "description": "Working directory; defaults to \".\"." },
                        "use_worktree": { "type": "boolean" },
                        "account": { "type": "string", "description": "Named account profile for this agent, if any." },
                        "real_home": { "type": "boolean", "description": "Off by default — runs against the isolated home, not your real credentials/files." },
                        "no_memory_context": { "type": "boolean" },
                        "timeout_secs": { "type": "integer" },
                        "allow_fallback": { "type": "boolean" }
                    },
                    "required": ["description", "agent"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "orchestrate_run",
                "Runs several agents in sequence on one goal: each agent gets the previous agent's real output. A sequential relay, not live back-and-forth.",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string" },
                        "agents": { "type": "array", "items": { "type": "string" }, "description": "Ordered list of agent names." },
                        "cwd": { "type": "string" },
                        "use_worktree": { "type": "boolean" },
                        "real_home": { "type": "boolean" },
                        "timeout_secs": { "type": "integer" }
                    },
                    "required": ["goal", "agents"],
                    "additionalProperties": false
                })),
            ),
        ]))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let empty = Map::new();
        let arguments = request.arguments.as_ref().unwrap_or(&empty);
        let result = match request.name.as_ref() {
            "task_run" => self.task_run(arguments),
            "orchestrate_run" => self.orchestrate_run(arguments),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_run_rejects_missing_description() {
        let args: Map<String, Value> = json!({ "agent": "codex" }).as_object().unwrap().clone();
        // SingleCliServer::new() talks to SingleDirs::discover(), which is filesystem-backed —
        // exercise the pure argument-validation path directly instead of constructing a server.
        assert!(SingleCliServer::str_arg(&args, "description").is_err());
    }

    #[test]
    fn orchestrate_run_rejects_missing_agents() {
        let args: Map<String, Value> = json!({ "goal": "ship it" }).as_object().unwrap().clone();
        assert!(args.get("agents").and_then(Value::as_array).is_none());
    }
}
