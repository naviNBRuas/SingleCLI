use crate::client::call;
use single_core::SingleDirs;
use single_protocol::{
    AccountProfileInfo, AgentInfo, LspServerSpec, McpServerInfo, PluginSpec, ProviderPresetInfo,
    ProviderSpec, Request, Response, ResponseData, RuntimeStatus, SetupAction, TaskRecord, TaskStatus, ToolSpec,
};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Agents,
    Tasks,
    Mcp,
    Lsp,
    Plugins,
    Tools,
    Providers,
    Accounts,
    Memory,
    Help,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Agents, Tab::Tasks, Tab::Mcp, Tab::Lsp, Tab::Plugins, Tab::Tools, Tab::Providers, Tab::Accounts, Tab::Memory, Tab::Help,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Agents => "Agents",
            Tab::Tasks => "Tasks",
            Tab::Mcp => "MCP",
            Tab::Lsp => "LSP",
            Tab::Plugins => "Plugins",
            Tab::Tools => "Tools",
            Tab::Providers => "Providers",
            Tab::Accounts => "Accounts",
            Tab::Memory => "Memory",
            Tab::Help => "Help",
        }
    }

    fn next(self) -> Tab {
        let idx = Tab::ALL.iter().position(|t| *t == self).unwrap();
        Tab::ALL[(idx + 1) % Tab::ALL.len()]
    }

    fn prev(self) -> Tab {
        let idx = Tab::ALL.iter().position(|t| *t == self).unwrap();
        Tab::ALL[(idx + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }
}

/// State of the in-TUI agent install flow (spec: "for agents install lets
/// make them inside the tui"). A confirm step (showing the real bootstrap
/// command before running it — never install silently) then a background
/// thread runs the real install so the UI keeps redrawing/spinning instead
/// of freezing for however long the download takes.
pub enum InstallFlow {
    Idle,
    Confirming { agent: String, command: String, source: String },
    Running { agent: String, started_at: Instant, rx: mpsc::Receiver<anyhow::Result<SetupAction>> },
    Done { agent: String, action: SetupAction },
    Failed { agent: String, error: String },
}

/// State of the in-TUI "add a provider" flow: pick a preset (OpenAI,
/// Anthropic, OpenCode Zen, NVIDIA — spec: "id like to add providers,
/// configuring them using the TUI"), type the API key (masked), submit.
/// Registration and key storage happen on a background thread — the key
/// itself never lands in `App` once submitted, only the masked input
/// buffer while typing.
pub enum ProviderAddFlow {
    Idle,
    PickingPreset { presets: Vec<ProviderPresetInfo>, selected: usize },
    EnteringKey { preset: ProviderPresetInfo, input: String },
    Submitting { preset_name: String, rx: mpsc::Receiver<anyhow::Result<()>> },
    Done { preset_name: String },
    Failed { preset_name: String, error: String },
}

/// State of the in-TUI "create a task" flow: description, workspace path,
/// then one or more agents (toggle-select — picking more than one runs
/// `Orchestrate` instead of a plain `TaskRun`). Mirrors `ProviderAddFlow`'s
/// step-then-background-thread shape.
pub enum TaskAddFlow {
    Idle,
    EnteringDescription { input: String },
    EnteringCwd { description: String, input: String },
    PickingAgents { description: String, cwd: String, agent_names: Vec<String>, chosen: Vec<bool>, cursor: usize, real_home: bool },
    Submitting { rx: mpsc::Receiver<anyhow::Result<usize>> },
    Done { count: usize },
    Failed { error: String },
}

/// Which registry a `QuickAddFlow` is adding into, and how its single-line
/// input is parsed. Deliberately one compact form (`field|field|...`)
/// instead of a per-type multi-step wizard — this is a power-user quick
/// add for the TUI; anything needing finer control (env vars on an MCP
/// server, etc.) still has the full `single ... add` CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddKind {
    Mcp,
    Lsp,
    Plugin,
    Tool,
}

impl QuickAddKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mcp => "MCP server",
            Self::Lsp => "LSP server",
            Self::Plugin => "plugin",
            Self::Tool => "tool",
        }
    }

    pub fn format_hint(&self) -> &'static str {
        match self {
            Self::Mcp => "name|command|arg1,arg2 (args optional)",
            Self::Lsp => "name|command|arg1,arg2|.ext1,.ext2 (args/extensions optional)",
            Self::Plugin => "name|target|opencode_module (opencode_module optional)",
            Self::Tool => "name|description|risk_level (low/medium/high)",
        }
    }
}

/// State of the in-TUI "quick add" flow shared by the MCP/LSP/Plugins/Tools
/// tabs: one line of `field|field|...` input, parsed per `QuickAddKind`,
/// submitted on a background thread (same shape as `ProviderAddFlow`).
pub enum QuickAddFlow {
    Idle,
    EnteringLine { kind: QuickAddKind, input: String },
    Submitting { kind: QuickAddKind, rx: mpsc::Receiver<anyhow::Result<()>> },
    Done { kind: QuickAddKind },
    Failed { kind: QuickAddKind, error: String },
}

/// State of the task-detail viewer (Tasks tab, `Enter` on a row): shows a
/// single task's full output, live-tailed while it's still running (see
/// `single_core::SingleDirs::task_live_output_path` /
/// `single-agent-sdk::run::run_command_live`) and switching to the final
/// artifact once it finishes. Since an `orchestrate` run creates one task
/// row per agent per step, this is also how each agent in a multi-agent
/// run gets inspected individually — select its row, press `Enter`.
pub enum TaskDetailFlow {
    Idle,
    Viewing { task: TaskRecord, output: String, last_polled: Instant },
}

pub struct App {
    pub socket_path: PathBuf,
    pub dirs: SingleDirs,
    pub tab: Tab,
    pub selected: usize,
    pub status: Option<RuntimeStatus>,
    pub agents: Vec<AgentInfo>,
    pub tasks: Vec<TaskRecord>,
    pub mcp_servers: Vec<McpServerInfo>,
    pub lsp_servers: Vec<LspServerSpec>,
    pub plugins: Vec<PluginSpec>,
    pub tools: Vec<ToolSpec>,
    pub providers: Vec<ProviderSpec>,
    pub accounts: Vec<AccountProfileInfo>,
    pub kg_entity_count: Option<usize>,
    pub cache_configured: bool,
    pub cache_reachable: bool,
    pub vector_configured: bool,
    pub vector_reachable: bool,
    pub error: Option<String>,
    pub install: InstallFlow,
    pub provider_add: ProviderAddFlow,
    pub task_add: TaskAddFlow,
    pub quick_add: QuickAddFlow,
    pub task_detail: TaskDetailFlow,
    pub last_refresh: Instant,
}

impl App {
    pub fn new(dirs: SingleDirs) -> Self {
        let socket_path = dirs.socket_path();
        let mut app = Self {
            socket_path,
            dirs,
            tab: Tab::Agents,
            selected: 0,
            status: None,
            agents: Vec::new(),
            tasks: Vec::new(),
            mcp_servers: Vec::new(),
            lsp_servers: Vec::new(),
            plugins: Vec::new(),
            tools: Vec::new(),
            providers: Vec::new(),
            accounts: Vec::new(),
            kg_entity_count: None,
            cache_configured: false,
            cache_reachable: false,
            vector_configured: false,
            vector_reachable: false,
            error: None,
            install: InstallFlow::Idle,
            provider_add: ProviderAddFlow::Idle,
            task_add: TaskAddFlow::Idle,
            quick_add: QuickAddFlow::Idle,
            task_detail: TaskDetailFlow::Idle,
            last_refresh: Instant::now(),
        };
        app.refresh();
        app
    }

    pub fn refresh(&mut self) {
        self.error = None;
        self.status = self.fetch_status();
        self.agents = self.fetch_agents();
        self.tasks = self.fetch(Request::TaskList, |d| match d { ResponseData::Tasks(t) => Some(t), _ => None }).unwrap_or_default();
        self.mcp_servers = self.fetch(Request::McpList, |d| match d { ResponseData::McpServers(s) => Some(s), _ => None }).unwrap_or_default();
        self.lsp_servers = self.fetch(Request::LspList, |d| match d { ResponseData::LspServers(s) => Some(s), _ => None }).unwrap_or_default();
        self.plugins = self.fetch(Request::PluginList, |d| match d { ResponseData::Plugins(p) => Some(p), _ => None }).unwrap_or_default();
        self.tools = self.fetch(Request::ToolList, |d| match d { ResponseData::Tools(t) => Some(t), _ => None }).unwrap_or_default();
        self.providers = self.fetch(Request::ProviderList, |d| match d { ResponseData::Providers(p) => Some(p), _ => None }).unwrap_or_default();
        self.accounts = self.fetch(Request::AccountList { agent: None }, |d| match d { ResponseData::AccountProfiles(p) => Some(p), _ => None }).unwrap_or_default();
        self.kg_entity_count = self
            .fetch(Request::KgReadGraph, |d| match d { ResponseData::KgGraph(g) => Some(g), _ => None })
            .map(|g| g.entities.len());
        if let Some(ResponseData::CacheStatus { configured, reachable, .. }) = self.raw(Request::CacheStatus) {
            self.cache_configured = configured;
            self.cache_reachable = reachable;
        }
        if let Some(ResponseData::VectorStatus { configured, reachable, .. }) = self.raw(Request::VectorStatus) {
            self.vector_configured = configured;
            self.vector_reachable = reachable;
        }
        self.last_refresh = Instant::now();
        self.clamp_selection();
    }

    fn fetch_status(&mut self) -> Option<RuntimeStatus> {
        self.fetch(Request::Status, |d| match d { ResponseData::Status(s) => Some(s), _ => None })
    }

    fn fetch_agents(&mut self) -> Vec<AgentInfo> {
        self.fetch(Request::AgentList, |d| match d { ResponseData::Agents(a) => Some(a), _ => None }).unwrap_or_default()
    }

    fn fetch<T>(&mut self, req: Request, extract: impl FnOnce(ResponseData) -> Option<T>) -> Option<T> {
        match self.raw(req) {
            Some(data) => extract(data),
            None => None,
        }
    }

    fn raw(&mut self, req: Request) -> Option<ResponseData> {
        match call(&self.socket_path, &req) {
            Ok(Response::Ok { data }) => Some(data),
            Ok(Response::Error { message }) => {
                self.error = Some(message);
                None
            }
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }

    pub fn current_len(&self) -> usize {
        match self.tab {
            Tab::Agents => self.agents.len(),
            Tab::Tasks => self.tasks.len(),
            Tab::Mcp => self.mcp_servers.len(),
            Tab::Lsp => self.lsp_servers.len(),
            Tab::Plugins => self.plugins.len(),
            Tab::Tools => self.tools.len(),
            Tab::Providers => self.providers.len(),
            Tab::Accounts => self.accounts.len(),
            Tab::Memory | Tab::Help => 0,
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.current_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    pub fn next_tab(&mut self) {
        self.tab = self.tab.next();
        self.selected = 0;
    }

    pub fn prev_tab(&mut self) {
        self.tab = self.tab.prev();
        self.selected = 0;
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let current = self.selected as i32;
        let new = (current + delta).rem_euclid(len as i32);
        self.selected = new as usize;
    }

    /// Begins the interactive install flow for the currently selected
    /// agent, if it's a real agent with a bootstrap command and isn't
    /// already installed.
    pub fn begin_install(&mut self) {
        if self.tab != Tab::Agents {
            return;
        }
        let Some(agent) = self.agents.get(self.selected) else { return };
        if agent.detected {
            self.error = Some(format!("{} is already installed", agent.name));
            return;
        }
        let Some(install) = &agent.bootstrap_install else {
            self.error = Some(format!("{} has no verified install command", agent.name));
            return;
        };
        self.install = InstallFlow::Confirming { agent: agent.name.clone(), command: install.command.clone(), source: install.source.clone() };
    }

    pub fn confirm_install(&mut self) {
        let InstallFlow::Confirming { agent, .. } = &self.install else { return };
        let agent = agent.clone();
        let socket_path = self.socket_path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = call(&socket_path, &Request::AgentInstall { name: agent.clone(), dry_run: false }).and_then(|resp| match resp {
                Response::Ok { data: ResponseData::AgentInstallResult(action) } => Ok(action),
                Response::Ok { .. } => Err(anyhow::anyhow!("unexpected response")),
                Response::Error { message } => Err(anyhow::anyhow!(message)),
            });
            let _ = tx.send(result);
        });
        let agent_name = if let InstallFlow::Confirming { agent, .. } = &self.install { agent.clone() } else { unreachable!() };
        self.install = InstallFlow::Running { agent: agent_name, started_at: Instant::now(), rx };
    }

    pub fn cancel_install(&mut self) {
        self.install = InstallFlow::Idle;
    }

    /// Polls the background install thread; call every tick. Returns true
    /// if a redraw-worthy state change happened (install just finished).
    pub fn poll_install(&mut self) -> bool {
        if let InstallFlow::Running { agent, rx, .. } = &self.install {
            if let Ok(result) = rx.try_recv() {
                self.install = match result {
                    Ok(action) => InstallFlow::Done { agent: agent.clone(), action },
                    Err(e) => InstallFlow::Failed { agent: agent.clone(), error: e.to_string() },
                };
                self.refresh();
                return true;
            }
        }
        false
    }

    pub fn install_elapsed(&self) -> Option<Duration> {
        match &self.install {
            InstallFlow::Running { started_at, .. } => Some(started_at.elapsed()),
            _ => None,
        }
    }

    /// Opens the preset picker (spec: "add providers, configuring them
    /// using the TUI, for providers like OpenAI, Anthropic, OpenCode Zen,
    /// NVIDIA AI API").
    pub fn begin_add_provider(&mut self) {
        if self.tab != Tab::Providers {
            return;
        }
        let presets = match call(&self.socket_path, &Request::ProviderPresetList) {
            Ok(Response::Ok { data: ResponseData::ProviderPresets(p) }) => p,
            Ok(Response::Error { message }) => {
                self.error = Some(message);
                return;
            }
            _ => {
                self.error = Some("failed to load provider presets".into());
                return;
            }
        };
        self.provider_add = ProviderAddFlow::PickingPreset { presets, selected: 0 };
    }

    pub fn provider_picker_move(&mut self, delta: i32) {
        if let ProviderAddFlow::PickingPreset { presets, selected } = &mut self.provider_add {
            if !presets.is_empty() {
                let len = presets.len() as i32;
                *selected = ((*selected as i32 + delta).rem_euclid(len)) as usize;
            }
        }
    }

    pub fn provider_picker_confirm(&mut self) {
        if let ProviderAddFlow::PickingPreset { presets, selected } = &self.provider_add {
            if let Some(preset) = presets.get(*selected).cloned() {
                self.provider_add = ProviderAddFlow::EnteringKey { preset, input: String::new() };
            }
        }
    }

    pub fn provider_key_input(&mut self, c: char) {
        if let ProviderAddFlow::EnteringKey { input, .. } = &mut self.provider_add {
            input.push(c);
        }
    }

    pub fn provider_key_backspace(&mut self) {
        if let ProviderAddFlow::EnteringKey { input, .. } = &mut self.provider_add {
            input.pop();
        }
    }

    pub fn provider_key_submit(&mut self) {
        let ProviderAddFlow::EnteringKey { preset, input } = &self.provider_add else { return };
        if input.is_empty() {
            self.error = Some("API key cannot be empty".into());
            return;
        }
        let preset_name = preset.name.clone();
        let value = input.clone();
        let socket_path = self.socket_path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                match call(&socket_path, &Request::ProviderAddPreset { name: preset_name.clone() })? {
                    Response::Ok { .. } => {}
                    Response::Error { message } => anyhow::bail!(message),
                }
                match call(&socket_path, &Request::ProviderSetKey { name: preset_name.clone(), value })? {
                    Response::Ok { .. } => Ok(()),
                    Response::Error { message } => anyhow::bail!(message),
                }
            })();
            let _ = tx.send(result);
        });
        self.provider_add = ProviderAddFlow::Submitting { preset_name: preset.name.clone(), rx };
    }

    pub fn cancel_provider_add(&mut self) {
        self.provider_add = ProviderAddFlow::Idle;
    }

    pub fn poll_provider_add(&mut self) -> bool {
        if let ProviderAddFlow::Submitting { preset_name, rx } = &self.provider_add {
            if let Ok(result) = rx.try_recv() {
                self.provider_add = match result {
                    Ok(()) => ProviderAddFlow::Done { preset_name: preset_name.clone() },
                    Err(e) => ProviderAddFlow::Failed { preset_name: preset_name.clone(), error: e.to_string() },
                };
                self.refresh();
                return true;
            }
        }
        false
    }

    /// Opens the task-creation flow (spec: "in the TUI, i want to be able
    /// to create tasks, specify workspace paths, agent(s), and so on").
    pub fn begin_add_task(&mut self) {
        if self.tab != Tab::Tasks {
            return;
        }
        self.task_add = TaskAddFlow::EnteringDescription { input: String::new() };
    }

    pub fn task_desc_input(&mut self, c: char) {
        if let TaskAddFlow::EnteringDescription { input } = &mut self.task_add {
            input.push(c);
        }
    }

    pub fn task_desc_backspace(&mut self) {
        if let TaskAddFlow::EnteringDescription { input } = &mut self.task_add {
            input.pop();
        }
    }

    pub fn task_desc_submit(&mut self) {
        let TaskAddFlow::EnteringDescription { input } = &self.task_add else { return };
        if input.is_empty() {
            self.error = Some("description cannot be empty".into());
            return;
        }
        self.task_add = TaskAddFlow::EnteringCwd { description: input.clone(), input: ".".to_string() };
    }

    pub fn task_cwd_input(&mut self, c: char) {
        if let TaskAddFlow::EnteringCwd { input, .. } = &mut self.task_add {
            input.push(c);
        }
    }

    pub fn task_cwd_backspace(&mut self) {
        if let TaskAddFlow::EnteringCwd { input, .. } = &mut self.task_add {
            input.pop();
        }
    }

    pub fn task_cwd_submit(&mut self) {
        let TaskAddFlow::EnteringCwd { description, input } = &self.task_add else { return };
        let cwd = if input.is_empty() { ".".to_string() } else { input.clone() };
        let cwd = std::fs::canonicalize(&cwd).map(|p| p.display().to_string()).unwrap_or(cwd);
        let agent_names: Vec<String> = self.agents.iter().filter(|a| a.detected).map(|a| a.name.clone()).collect();
        let chosen = vec![false; agent_names.len()];
        self.task_add = TaskAddFlow::PickingAgents { description: description.clone(), cwd, agent_names, chosen, cursor: 0, real_home: false };
    }

    pub fn task_agents_move(&mut self, delta: i32) {
        if let TaskAddFlow::PickingAgents { agent_names, cursor, .. } = &mut self.task_add {
            if !agent_names.is_empty() {
                let len = agent_names.len() as i32;
                *cursor = ((*cursor as i32 + delta).rem_euclid(len)) as usize;
            }
        }
    }

    pub fn task_agents_toggle(&mut self) {
        if let TaskAddFlow::PickingAgents { chosen, cursor, .. } = &mut self.task_add {
            if let Some(c) = chosen.get_mut(*cursor) {
                *c = !*c;
            }
        }
    }

    /// Toggles whether this task runs against the real, ambient $HOME
    /// instead of SingleCLI's isolated one — for tasks that need to
    /// actually modify the real system (see `Request::TaskRun::real_home`
    /// docs). Off by default.
    pub fn task_toggle_real_home(&mut self) {
        if let TaskAddFlow::PickingAgents { real_home, .. } = &mut self.task_add {
            *real_home = !*real_home;
        }
    }

    pub fn task_agents_submit(&mut self) {
        let TaskAddFlow::PickingAgents { description, cwd, agent_names, chosen, real_home, .. } = &self.task_add else { return };
        let selected: Vec<String> = agent_names.iter().zip(chosen).filter(|(_, on)| **on).map(|(n, _)| n.clone()).collect();
        if selected.is_empty() {
            self.error = Some("pick at least one agent (space to toggle)".into());
            return;
        }
        let description = description.clone();
        let cwd = cwd.clone();
        let real_home = *real_home;
        let socket_path = self.socket_path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<usize> {
                if selected.len() == 1 {
                    match call(&socket_path, &Request::TaskRun { description, agent: selected[0].clone(), cwd, use_worktree: false, account: None, real_home, no_memory_context: false, timeout_secs: 300 })? {
                        Response::Ok { .. } => Ok(1),
                        Response::Error { message } => anyhow::bail!(message),
                    }
                } else {
                    match call(&socket_path, &Request::Orchestrate { goal: description, agents: selected.clone(), cwd, use_worktree: true, real_home, timeout_secs: 300 })? {
                        Response::Ok { .. } => Ok(selected.len()),
                        Response::Error { message } => anyhow::bail!(message),
                    }
                }
            })();
            let _ = tx.send(result);
        });
        self.task_add = TaskAddFlow::Submitting { rx };
    }

    pub fn cancel_task_add(&mut self) {
        self.task_add = TaskAddFlow::Idle;
    }

    pub fn poll_task_add(&mut self) -> bool {
        if let TaskAddFlow::Submitting { rx } = &self.task_add {
            if let Ok(result) = rx.try_recv() {
                self.task_add = match result {
                    Ok(count) => TaskAddFlow::Done { count },
                    Err(e) => TaskAddFlow::Failed { error: e.to_string() },
                };
                self.refresh();
                return true;
            }
        }
        false
    }

    /// Opens the quick-add flow for whichever of MCP/LSP/Plugins/Tools tab
    /// is active.
    pub fn begin_quick_add(&mut self) {
        let kind = match self.tab {
            Tab::Mcp => QuickAddKind::Mcp,
            Tab::Lsp => QuickAddKind::Lsp,
            Tab::Plugins => QuickAddKind::Plugin,
            Tab::Tools => QuickAddKind::Tool,
            _ => return,
        };
        self.quick_add = QuickAddFlow::EnteringLine { kind, input: String::new() };
    }

    pub fn quick_add_input(&mut self, c: char) {
        if let QuickAddFlow::EnteringLine { input, .. } = &mut self.quick_add {
            input.push(c);
        }
    }

    pub fn quick_add_backspace(&mut self) {
        if let QuickAddFlow::EnteringLine { input, .. } = &mut self.quick_add {
            input.pop();
        }
    }

    pub fn quick_add_submit(&mut self) {
        let QuickAddFlow::EnteringLine { kind, input } = &self.quick_add else { return };
        let kind = *kind;
        let parts: Vec<&str> = input.split('|').map(|s| s.trim()).collect();
        let request = match build_quick_add_request(kind, &parts) {
            Ok(req) => req,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let socket_path = self.socket_path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = match call(&socket_path, &request) {
                Ok(Response::Ok { .. }) => Ok(()),
                Ok(Response::Error { message }) => Err(anyhow::anyhow!(message)),
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
        });
        self.quick_add = QuickAddFlow::Submitting { kind, rx };
    }

    pub fn cancel_quick_add(&mut self) {
        self.quick_add = QuickAddFlow::Idle;
    }

    pub fn poll_quick_add(&mut self) -> bool {
        if let QuickAddFlow::Submitting { kind, rx } = &self.quick_add {
            if let Ok(result) = rx.try_recv() {
                let kind = *kind;
                self.quick_add = match result {
                    Ok(()) => QuickAddFlow::Done { kind },
                    Err(e) => QuickAddFlow::Failed { kind, error: e.to_string() },
                };
                self.refresh();
                return true;
            }
        }
        false
    }

    /// Removes the currently selected entry (MCP/LSP/Plugins/Tools/
    /// Accounts/Providers tab — whichever has a real Remove request).
    pub fn delete_selected(&mut self) {
        let request = match self.tab {
            Tab::Mcp => self.mcp_servers.get(self.selected).map(|s| Request::McpRemove { name: s.name.clone() }),
            Tab::Lsp => self.lsp_servers.get(self.selected).map(|s| Request::LspRemove { name: s.name.clone() }),
            Tab::Plugins => self.plugins.get(self.selected).map(|p| Request::PluginRemove { name: p.name.clone() }),
            Tab::Tools => None, // tools are built-in metadata; disable instead of remove (see toggle_selected)
            Tab::Providers => self.providers.get(self.selected).map(|p| Request::ProviderRemove { name: p.name.clone() }),
            Tab::Accounts => self.accounts.get(self.selected).map(|a| Request::AccountRemove { agent: a.agent.clone(), name: a.name.clone() }),
            _ => None,
        };
        let Some(request) = request else { return };
        self.raw(request);
        self.refresh();
    }

    /// Toggles enabled/disabled for the selected MCP server or tool (the
    /// two registries with a real Enable/Disable request).
    pub fn toggle_selected(&mut self) {
        let request = match self.tab {
            Tab::Mcp => self.mcp_servers.get(self.selected).map(|s| {
                if s.enabled { Request::McpDisable { name: s.name.clone() } } else { Request::McpEnable { name: s.name.clone() } }
            }),
            Tab::Tools => self.tools.get(self.selected).map(|t| {
                if t.enabled { Request::ToolDisable { name: t.name.clone() } } else { Request::ToolEnable { name: t.name.clone() } }
            }),
            _ => None,
        };
        let Some(request) = request else { return };
        self.raw(request);
        self.refresh();
    }

    /// Syncs the selected plugin into every registered agent (Plugins tab).
    pub fn sync_selected_plugin(&mut self) {
        if self.tab != Tab::Plugins {
            return;
        }
        let Some(plugin) = self.plugins.get(self.selected) else { return };
        self.raw(Request::PluginSync { name: plugin.name.clone(), agents: Vec::new(), dry_run: false });
        self.refresh();
    }

    /// Opens the task-detail viewer for the selected row (Tasks tab).
    pub fn begin_view_task(&mut self) {
        if self.tab != Tab::Tasks {
            return;
        }
        let Some(task) = self.tasks.get(self.selected).cloned() else { return };
        let output = self.read_task_output(&task);
        self.task_detail = TaskDetailFlow::Viewing { task, output, last_polled: Instant::now() };
    }

    pub fn cancel_task_detail(&mut self) {
        self.task_detail = TaskDetailFlow::Idle;
    }

    fn read_task_output(&self, task: &TaskRecord) -> String {
        let path = if task.status == TaskStatus::Running {
            self.dirs.task_live_output_path(task.id)
        } else {
            self.dirs.task_artifact_path(task.id)
        };
        std::fs::read_to_string(&path).unwrap_or_else(|_| "(no output yet)".to_string())
    }

    /// Re-fetches the viewed task and its output while it's still
    /// running, throttled so this doesn't hit the socket/disk every
    /// single event-loop tick. Stops polling once the task finishes —
    /// there's nothing left to change.
    pub fn poll_task_detail(&mut self) -> bool {
        let TaskDetailFlow::Viewing { task, last_polled, .. } = &self.task_detail else { return false };
        if task.status != TaskStatus::Running || last_polled.elapsed() < Duration::from_millis(500) {
            return false;
        }
        let id = task.id;
        let Some(updated) = self.fetch(Request::TaskInspect { id }, |d| match d { ResponseData::Task(t) => Some(t), _ => None }) else {
            return false;
        };
        let output = self.read_task_output(&updated);
        self.task_detail = TaskDetailFlow::Viewing { task: updated, output, last_polled: Instant::now() };
        true
    }
}

fn build_quick_add_request(kind: QuickAddKind, parts: &[&str]) -> Result<Request, String> {
    let bad_format = || format!("expected: {}", kind.format_hint());
    match kind {
        QuickAddKind::Mcp => {
            let name = parts.first().filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let command = parts.get(1).filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let args = parts.get(2).map(|s| s.split(',').filter(|a| !a.is_empty()).map(|a| a.to_string()).collect()).unwrap_or_default();
            Ok(Request::McpAdd { server: single_protocol::McpServerSpec { name: name.to_string(), command: command.to_string(), args, env: Default::default(), enabled: true } })
        }
        QuickAddKind::Lsp => {
            let name = parts.first().filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let command = parts.get(1).filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let args = parts.get(2).map(|s| s.split(',').filter(|a| !a.is_empty()).map(|a| a.to_string()).collect()).unwrap_or_default();
            let extensions = parts.get(3).map(|s| s.split(',').filter(|a| !a.is_empty()).map(|a| a.to_string()).collect()).unwrap_or_default();
            Ok(Request::LspAdd { server: single_protocol::LspServerSpec { name: name.to_string(), command: command.to_string(), args, extensions, enabled: true } })
        }
        QuickAddKind::Plugin => {
            let name = parts.first().filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let target = parts.get(1).filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let opencode_module = parts.get(2).filter(|s| !s.is_empty()).map(|s| s.to_string());
            Ok(Request::PluginAdd { plugin: single_protocol::PluginSpec { name: name.to_string(), target: target.to_string(), opencode_module } })
        }
        QuickAddKind::Tool => {
            let name = parts.first().filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let description = parts.get(1).filter(|s| !s.is_empty()).ok_or_else(bad_format)?;
            let risk_level = match parts.get(2).copied().unwrap_or("low") {
                "low" => single_protocol::RiskLevel::Low,
                "medium" => single_protocol::RiskLevel::Medium,
                "high" => single_protocol::RiskLevel::High,
                other => return Err(format!("unknown risk level '{other}' (expected low/medium/high)")),
            };
            Ok(Request::ToolAdd { tool: single_protocol::ToolSpec { name: name.to_string(), description: description.to_string(), risk_level, enabled: true } })
        }
    }
}
