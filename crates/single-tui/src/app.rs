use crate::client::call;
use single_protocol::{
    AccountProfileInfo, AgentInfo, McpServerInfo, ProviderPresetInfo, ProviderSpec, Request,
    Response, ResponseData, RuntimeStatus, SetupAction, TaskRecord,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Agents,
    Tasks,
    Mcp,
    Providers,
    Accounts,
    Memory,
    Help,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Agents, Tab::Tasks, Tab::Mcp, Tab::Providers, Tab::Accounts, Tab::Memory, Tab::Help];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Agents => "Agents",
            Tab::Tasks => "Tasks",
            Tab::Mcp => "MCP",
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

pub struct App {
    pub socket_path: PathBuf,
    pub tab: Tab,
    pub selected: usize,
    pub status: Option<RuntimeStatus>,
    pub agents: Vec<AgentInfo>,
    pub tasks: Vec<TaskRecord>,
    pub mcp_servers: Vec<McpServerInfo>,
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
    pub last_refresh: Instant,
}

impl App {
    pub fn new(socket_path: &Path) -> Self {
        let mut app = Self {
            socket_path: socket_path.to_path_buf(),
            tab: Tab::Agents,
            selected: 0,
            status: None,
            agents: Vec::new(),
            tasks: Vec::new(),
            mcp_servers: Vec::new(),
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
            let result = call(&socket_path, &Request::AgentInstall { name: agent.clone(), dry_run: false }).map_err(anyhow::Error::from).and_then(|resp| match resp {
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
}
