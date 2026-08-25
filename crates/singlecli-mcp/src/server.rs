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

    fn parse_parallel_tasks(args: &Map<String, Value>) -> anyhow::Result<Vec<single_protocol::ParallelTaskSpec>> {
        args.get("tasks")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing required array argument \"tasks\""))?
            .iter()
            .map(|t| {
                let obj = t.as_object().ok_or_else(|| anyhow::anyhow!("each task must be an object"))?;
                Ok(single_protocol::ParallelTaskSpec {
                    agent: Self::str_arg(obj, "agent")?.to_string(),
                    description: Self::str_arg(obj, "description")?.to_string(),
                })
            })
            .collect()
    }

    fn parse_graph_nodes(args: &Map<String, Value>) -> anyhow::Result<Vec<single_protocol::TaskGraphNode>> {
        args.get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing required array argument \"nodes\""))?
            .iter()
            .map(|n| {
                let obj = n.as_object().ok_or_else(|| anyhow::anyhow!("each node must be an object"))?;
                let depends_on: Vec<String> = obj
                    .get("depends_on")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                let run_if = match obj.get("run_if").and_then(Value::as_str) {
                    Some("on_success") => single_protocol::RunCondition::OnSuccess,
                    Some("on_failure") => single_protocol::RunCondition::OnFailure,
                    _ => single_protocol::RunCondition::Always,
                };
                Ok(single_protocol::TaskGraphNode {
                    id: Self::str_arg(obj, "id")?.to_string(),
                    agent: Self::str_arg(obj, "agent")?.to_string(),
                    description: Self::str_arg(obj, "description")?.to_string(),
                    depends_on,
                    run_if,
                })
            })
            .collect()
    }

    fn orchestrate_parallel_run(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let tasks = Self::parse_parallel_tasks(args)?;
        let cwd = args.get("cwd").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| ".".to_string());
        self.send(Request::OrchestrateParallel {
            tasks,
            cwd,
            real_home: Self::bool_arg(args, "real_home", false),
            timeout_secs: Self::u64_arg(args, "timeout_secs", 300),
            background: false,
            orchestrator: single_protocol::OrchestratorMode::Fixed,
            goal: args.get("goal").and_then(Value::as_str).map(str::to_string),
            candidate_agents: Vec::new(),
        })
    }

    fn orchestrate_graph_run(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let nodes = Self::parse_graph_nodes(args)?;
        let cwd = args.get("cwd").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| ".".to_string());
        self.send(Request::OrchestrateGraph {
            nodes,
            cwd,
            real_home: Self::bool_arg(args, "real_home", false),
            timeout_secs: Self::u64_arg(args, "timeout_secs", 300),
            background: false,
            orchestrator: single_protocol::OrchestratorMode::Fixed,
            goal: args.get("goal").and_then(Value::as_str).map(str::to_string),
            candidate_agents: Vec::new(),
        })
    }

    fn parse_agents(args: &Map<String, Value>) -> anyhow::Result<Vec<String>> {
        let agents = args
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing required array argument \"agents\""))?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Ok(agents)
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
        let agents = Self::parse_agents(args)?;
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

    fn agent_list(&self) -> anyhow::Result<Value> {
        self.send(Request::AgentList)
    }

    fn agent_inspect(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let name = Self::str_arg(args, "name")?.to_string();
        self.send(Request::AgentInspect { name })
    }

    fn memory_store(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let title = Self::str_arg(args, "title")?.to_string();
        let content = Self::str_arg(args, "content")?.to_string();
        self.send(Request::MemoryStore {
            scope: None,
            source: None,
            project: args.get("project").and_then(Value::as_str).map(str::to_string),
            agent: args.get("agent").and_then(Value::as_str).map(str::to_string),
            task: args.get("task").and_then(Value::as_str).map(str::to_string),
            title,
            content,
            confidence: args.get("confidence").and_then(Value::as_f64),
            expires_in_seconds: args.get("expires_in_seconds").and_then(Value::as_i64),
        })
    }

    fn memory_search(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let query = Self::str_arg(args, "query")?.to_string();
        self.send(Request::MemorySearch {
            query,
            scope: None,
            project: args.get("project").and_then(Value::as_str).map(str::to_string),
        })
    }

    fn provider_configured_list(&self) -> anyhow::Result<Value> {
        self.send(Request::ConfiguredProviderList)
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
            Tool::new(
                "orchestrate_parallel_run",
                "Runs several agents concurrently, each on its own explicit sub-task, each in its own git worktree. No automatic goal splitting — you supply each agent's task.",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "items": { "type": "object", "properties": { "agent": { "type": "string" }, "description": { "type": "string" } }, "required": ["agent", "description"] }
                        },
                        "goal": { "type": "string" },
                        "cwd": { "type": "string" },
                        "real_home": { "type": "boolean" },
                        "timeout_secs": { "type": "integer" }
                    },
                    "required": ["tasks"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "orchestrate_graph_run",
                "Runs an explicit dependency graph of agent tasks: each node runs once its dependencies have finished, with real cycle validation.",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "nodes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "agent": { "type": "string" },
                                    "description": { "type": "string" },
                                    "depends_on": { "type": "array", "items": { "type": "string" } },
                                    "run_if": { "type": "string", "enum": ["always", "on_success", "on_failure"] }
                                },
                                "required": ["id", "agent", "description"]
                            }
                        },
                        "goal": { "type": "string" },
                        "cwd": { "type": "string" },
                        "real_home": { "type": "boolean" },
                        "timeout_secs": { "type": "integer" }
                    },
                    "required": ["nodes"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "agent_list",
                "Lists every agent CLI SingleCLI knows about, with detection status — what's actually available to delegate to.",
                schema(json!({ "type": "object", "properties": {}, "additionalProperties": false })),
            ),
            Tool::new(
                "agent_inspect",
                "Details on one agent: detection, install method, capabilities.",
                schema(json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"], "additionalProperties": false })),
            ),
            Tool::new(
                "memory_store",
                "Stores an entry in SingleCLI's shared memory store, visible to every agent's task preamble.",
                schema(json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "content": { "type": "string" },
                        "project": { "type": "string" },
                        "agent": { "type": "string" },
                        "task": { "type": "string" },
                        "confidence": { "type": "number" },
                        "expires_in_seconds": { "type": "integer" }
                    },
                    "required": ["title", "content"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "memory_search",
                "Substring-searches SingleCLI's shared memory store.",
                schema(json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" }, "project": { "type": "string" } },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "provider_configured_list",
                "Lists which LLM providers actually have a key configured right now, for deciding what's available to delegate against.",
                schema(json!({ "type": "object", "properties": {}, "additionalProperties": false })),
            ),
        ]))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let empty = Map::new();
        let arguments = request.arguments.as_ref().unwrap_or(&empty);
        let result = match request.name.as_ref() {
            "task_run" => self.task_run(arguments),
            "orchestrate_run" => self.orchestrate_run(arguments),
            "orchestrate_parallel_run" => self.orchestrate_parallel_run(arguments),
            "orchestrate_graph_run" => self.orchestrate_graph_run(arguments),
            "agent_list" => self.agent_list(),
            "agent_inspect" => self.agent_inspect(arguments),
            "memory_store" => self.memory_store(arguments),
            "memory_search" => self.memory_search(arguments),
            "provider_configured_list" => self.provider_configured_list(),
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
        assert!(SingleCliServer::parse_agents(&args).is_err());
    }

    #[test]
    fn parse_agents_collects_string_items_in_order() {
        let args: Map<String, Value> = json!({ "agents": ["codex", "opencode"] }).as_object().unwrap().clone();
        assert_eq!(SingleCliServer::parse_agents(&args).unwrap(), vec!["codex".to_string(), "opencode".to_string()]);
    }

    #[test]
    fn parse_parallel_tasks_rejects_malformed_entries() {
        let args: Map<String, Value> = json!({ "tasks": [{ "agent": "codex" }] }).as_object().unwrap().clone(); // missing "description"
        assert!(SingleCliServer::parse_parallel_tasks(&args).is_err());
    }

    #[test]
    fn parse_parallel_tasks_accepts_well_formed_entries() {
        let args: Map<String, Value> = json!({ "tasks": [{ "agent": "codex", "description": "backend" }, { "agent": "claude", "description": "frontend" }] }).as_object().unwrap().clone();
        let tasks = SingleCliServer::parse_parallel_tasks(&args).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].agent, "codex");
        assert_eq!(tasks[1].description, "frontend");
    }

    #[test]
    fn parse_graph_nodes_accepts_dependencies() {
        let args: Map<String, Value> = json!({ "nodes": [
            { "id": "build", "agent": "codex", "description": "build it" },
            { "id": "test", "agent": "claude", "description": "test it", "depends_on": ["build"] }
        ] }).as_object().unwrap().clone();
        let nodes = SingleCliServer::parse_graph_nodes(&args).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[1].depends_on, vec!["build".to_string()]);
    }

    #[test]
    fn memory_store_rejects_missing_title_or_content() {
        let args: Map<String, Value> = json!({ "title": "note" }).as_object().unwrap().clone(); // missing content
        assert!(SingleCliServer::str_arg(&args, "content").is_err());
    }

    #[test]
    fn agent_inspect_requires_name() {
        let args: Map<String, Value> = json!({}).as_object().unwrap().clone();
        assert!(SingleCliServer::str_arg(&args, "name").is_err());
    }
}
