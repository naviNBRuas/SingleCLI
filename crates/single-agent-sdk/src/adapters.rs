use crate::adapter::{run_with_prompt_flag, AgentAdapter};
use crate::backup::backup_before_write;
use crate::formats;
use crate::run::run_command;
use anyhow::Result;
use single_protocol::{IntegrationWrite, McpServerSpec, RunOutcome};
use std::path::Path;
use std::time::Duration;

pub struct ClaudeAdapter;
pub struct CodexAdapter;
pub struct OpenCodeAdapter;
pub struct AgyAdapter;
pub struct PerplexityAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn command(&self) -> &str {
        "claude"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".claude.json");
        let updated = formats::claude::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("claude", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".claude.json");
        match formats::claude::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("claude", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("claude", home, "no config file present; nothing to remove")),
        }
    }

    /// `claude -p "<prompt>"` — confirmed non-interactive print mode via
    /// `claude --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, timeout: Duration) -> Result<RunOutcome> {
        run_with_prompt_flag("claude", cwd, prompt, timeout)
    }
}

impl AgentAdapter for CodexAdapter {
    fn command(&self) -> &str {
        "codex"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".codex").join("config.toml");
        let updated = formats::codex::apply(&path, servers)?;
        let rendered = toml::to_string_pretty(&updated)?;
        write_with_backup("codex", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".codex").join("config.toml");
        match formats::codex::remove(&path, names)? {
            Some(updated) => {
                let rendered = toml::to_string_pretty(&updated)?;
                write_with_backup("codex", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("codex", home, "no config file present; nothing to remove")),
        }
    }

    /// `codex exec "<prompt>"` — confirmed non-interactive mode via
    /// `codex exec --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, timeout: Duration) -> Result<RunOutcome> {
        run_command("codex", &["exec".to_string(), prompt.to_string()], cwd, timeout)
    }
}

impl AgentAdapter for OpenCodeAdapter {
    fn command(&self) -> &str {
        "opencode"
    }

    fn configure_mcp(&self, home: &Path, servers: &[McpServerSpec], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        let updated = formats::opencode::apply(&path, servers)?;
        let rendered = serde_json::to_string_pretty(&updated)?;
        write_with_backup("opencode", &path, &rendered, dry_run)
    }

    fn remove_mcp(&self, home: &Path, names: &[String], dry_run: bool) -> Result<IntegrationWrite> {
        let path = home.join(".config").join("opencode").join("opencode.jsonc");
        match formats::opencode::remove(&path, names)? {
            Some(updated) => {
                let rendered = serde_json::to_string_pretty(&updated)?;
                write_with_backup("opencode", &path, &rendered, dry_run)
            }
            None => Ok(unsupported_write("opencode", home, "no config file present; nothing to remove")),
        }
    }

    /// `opencode run "<prompt>" --dir <cwd>` — confirmed non-interactive
    /// mode and `--dir` flag via `opencode run --help` on the reference
    /// machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, timeout: Duration) -> Result<RunOutcome> {
        run_command(
            "opencode",
            &["run".to_string(), prompt.to_string(), "--dir".to_string(), cwd.display().to_string()],
            cwd,
            timeout,
        )
    }
}

impl AgentAdapter for AgyAdapter {
    fn command(&self) -> &str {
        "agy"
    }

    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("agy", home, "no on-disk MCP config location has been identified for agy"))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write("agy", home, "no on-disk MCP config location has been identified for agy"))
    }

    /// `agy -p "<prompt>"` — confirmed non-interactive print mode via
    /// `agy --help` on the reference machine.
    fn run_prompt(&self, cwd: &Path, prompt: &str, timeout: Duration) -> Result<RunOutcome> {
        run_with_prompt_flag("agy", cwd, prompt, timeout)
    }
}

impl AgentAdapter for PerplexityAdapter {
    fn command(&self) -> &str {
        "pplx"
    }

    fn configure_mcp(&self, home: &Path, _servers: &[McpServerSpec], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write(
            "perplexity",
            home,
            "pplx is a Search API client, not an MCP-capable coding agent; nothing to configure",
        ))
    }

    fn remove_mcp(&self, home: &Path, _names: &[String], _dry_run: bool) -> Result<IntegrationWrite> {
        Ok(unsupported_write(
            "perplexity",
            home,
            "pplx is a Search API client, not an MCP-capable coding agent; nothing to remove",
        ))
    }
}

fn unsupported_write(agent: &str, home: &Path, detail: &str) -> IntegrationWrite {
    IntegrationWrite {
        agent: agent.to_string(),
        config_path: home.display().to_string(),
        backup_path: None,
        applied: false,
        detail: detail.to_string(),
    }
}

fn write_with_backup(agent: &str, path: &Path, rendered: &str, dry_run: bool) -> Result<IntegrationWrite> {
    if dry_run {
        return Ok(IntegrationWrite {
            agent: agent.to_string(),
            config_path: path.display().to_string(),
            backup_path: None,
            applied: false,
            detail: format!("dry run: would write {} bytes to {}", rendered.len(), path.display()),
        });
    }

    let backup_path = backup_before_write(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rendered)?;

    Ok(IntegrationWrite {
        agent: agent.to_string(),
        config_path: path.display().to_string(),
        backup_path: backup_path.map(|p| p.display().to_string()),
        applied: true,
        detail: format!("wrote {} bytes", rendered.len()),
    })
}

pub fn for_agent(name: &str) -> Option<Box<dyn AgentAdapter>> {
    match name {
        "claude" => Some(Box::new(ClaudeAdapter)),
        "codex" => Some(Box::new(CodexAdapter)),
        "opencode" => Some(Box::new(OpenCodeAdapter)),
        "agy" => Some(Box::new(AgyAdapter)),
        "perplexity" => Some(Box::new(PerplexityAdapter)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_servers() -> Vec<McpServerSpec> {
        vec![McpServerSpec {
            name: "git".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: BTreeMap::new(),
            enabled: true,
        }]
    }

    #[test]
    fn claude_adapter_writes_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::write(home.join(".claude.json"), r#"{"numStartups": 1}"#).unwrap();

        let adapter = ClaudeAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), false).unwrap();
        assert!(result.applied);
        assert!(result.backup_path.is_some());

        let written = std::fs::read_to_string(home.join(".claude.json")).unwrap();
        assert!(written.contains("mcpServers"));
        assert!(written.contains("numStartups"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let adapter = ClaudeAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), true).unwrap();
        assert!(!result.applied);
        assert!(!home.join(".claude.json").exists());
    }

    #[test]
    fn codex_adapter_writes_toml() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let adapter = CodexAdapter;
        let result = adapter.configure_mcp(home, &sample_servers(), false).unwrap();
        assert!(result.applied);
        let written = std::fs::read_to_string(home.join(".codex").join("config.toml")).unwrap();
        assert!(written.contains("[mcp_servers.git]"));
    }

    #[test]
    fn agy_configure_is_a_documented_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = AgyAdapter;
        let result = adapter.configure_mcp(dir.path(), &sample_servers(), false).unwrap();
        assert!(!result.applied);
    }

    #[test]
    fn for_agent_returns_none_for_unknown_name() {
        assert!(for_agent("nonexistent").is_none());
        assert!(for_agent("claude").is_some());
    }

    #[test]
    fn perplexity_run_prompt_is_unsupported_by_default() {
        // pplx is a Search API client, not a coding agent — it should fall
        // through to AgentAdapter's default "unsupported" implementation
        // rather than silently claiming to run a prompt against it.
        let dir = tempfile::tempdir().unwrap();
        let adapter = PerplexityAdapter;
        assert!(adapter.run_prompt(dir.path(), "hello", Duration::from_secs(1)).is_err());
    }
}
