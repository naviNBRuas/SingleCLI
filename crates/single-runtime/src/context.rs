use single_core::{registry::AgentDefinition, ResolvedConfig, SingleDirs};

/// Shared state for handling a request: resolved config, the agent
/// registry, and where SingleCLI's directories live. Built once per
/// request in Phase 1 (cheap: local file reads) rather than kept as a long
/// lived mutable daemon state — there is no in-memory state that outlives a
/// single request yet, since orchestration/task state is Phase 4.
pub struct Context {
    pub dirs: SingleDirs,
    pub resolved: ResolvedConfig,
    pub registry: Vec<AgentDefinition>,
}

impl Context {
    pub fn load() -> anyhow::Result<Self> {
        let dirs = SingleDirs::discover()?;
        dirs.ensure_created()?;
        let resolved = single_core::resolve(&dirs, None)?;
        let mut registry = single_core::builtin_registry();

        // User-defined agents (~/.config/single/agents/*.toml) join the
        // registry alongside the five built-in ones. A malformed custom
        // agent file is skipped here (surfaced properly via `doctor`,
        // which calls `custom_agents::load_all` directly to see the
        // errors) rather than failing every request.
        if let Ok((custom, _errors)) = single_core::custom_agents::load_all(&dirs.agents_dir()) {
            registry.extend(custom.iter().map(single_core::custom_agents::to_agent_definition));
        }

        Ok(Self { dirs, resolved, registry })
    }

    pub fn find_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.registry.iter().find(|a| a.name == name)
    }
}
