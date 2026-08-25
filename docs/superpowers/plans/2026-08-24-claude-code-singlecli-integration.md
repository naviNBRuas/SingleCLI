# Claude Code ⟷ SingleCLI Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make SingleCLI the single place Navin configures MCP/LSP servers and provider credentials, with his real, day-to-day Claude Code install automatically gaining access to all of it — two MCP entries (`single-mcp`, `singlecli-mcp`) and one LSP entry (`single-lsp`) — plus a real mechanism for Claude Code to delegate work to other agents/models through SingleCLI.

**Architecture:** Extend three existing sync commands with a `--real-home` opt-in so they write Navin's actual `~/.claude.json`/`~/.claude/settings.json` instead of the isolated home. Build a new `singlecli-mcp` binary exposing SingleCLI's own task/orchestrate/agent/memory/provider commands as MCP tools over the existing daemon protocol. Build a new `single-lsp` binary that speaks LSP to Claude Code and dynamically proxies to whichever real language server a given open file needs, packaged as a Claude Code plugin whose manifest is generated from SingleCLI's LSP registry. Finish by rewiring this machine's actual `~/.claude` config to the new two-MCP/one-LSP shape.

**Tech Stack:** Rust (2021 edition), `rmcp` 3.1 (MCP server framework, already a workspace dependency via `single-mcp`), `tokio`, `serde_json`, existing `single-core`/`single-protocol`/`single-runtime`/`single-agent-sdk` crates. No new external LSP framework — the LSP proxy hand-rolls Content-Length JSON-RPC framing, matching this codebase's existing preference for simple hand-rolled protocol handling over pulling in a framework (see `single-protocol`'s own module doc: "not a real RPC framework — acceptable for Phase 1").

**Spec:** `docs/superpowers/specs/2026-08-24-claude-code-singlecli-integration-design.md`

## Global Constraints

- Every write into an agent's config file goes through `backup_before_write` (`crates/single-agent-sdk/src/backup.rs`) — this is already automatic for any code path that reuses the existing `formats::*::apply`/`provider_sync::sync`/`adapters::install_plugin` functions with a `home: &Path` argument, so no new task needs to re-implement backup logic; it only needs to make sure it's still passing through those same functions.
- Isolation stays the *default* everywhere. `--real-home` is opt-in, off by default, on every command it's added to — mirrors `task run --real-home`'s existing default-off posture (`crates/single-runtime/src/task.rs:345`).
- No placeholder LSP method list: the LSP proxy forwards generically (by inspecting `params.textDocument.uri` when present), not via a hand-maintained enum of supported methods — see Task 7.
- Sole commit author is `Navin B. Ruas <founder@nbr.company>`, subjects are `type: description` (`feat`/`fix`/`refactor`/`docs`/`test`/`chore`/`perf`/`build`/`ci` only), no `Co-Authored-By` — see the `git-commit-standards` skill.
- Workspace version is `0.5.0` (pre-1.0) — new crates inherit `version.workspace = true`; this whole feature is a `feat`, bumping the workspace minor version to `0.6.0` in the final task once everything is verified working end-to-end.

---

## Task 1: `--real-home` for `install-integrations` / `uninstall-integrations`

**Files:**
- Modify: `crates/single-protocol/src/lib.rs:511-514` (`Request::InstallIntegrations`, `Request::UninstallIntegrations`)
- Modify: `crates/single-runtime/src/integrations.rs:12-68` (`install_all`, `uninstall_all`)
- Modify: `crates/single-runtime/src/handlers.rs:1367-1372`
- Modify: `crates/single-cli/src/main.rs:264-275` (`Command::InstallIntegrations`, `Command::UninstallIntegrations`), `:2367-2381` (dispatch)
- Test: `crates/single-runtime/src/integrations.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `integrations::install_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult>` and `integrations::uninstall_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult>` — the `real_home` parameter later tasks don't touch, but Task 6 (registering `singlecli-mcp`) builds directly on top of `install_all`'s existing structure, so its signature must land here first.

- [ ] **Step 1: Write the failing test — `--real-home` writes the real path, not the isolated one**

Add to `crates/single-runtime/src/integrations.rs`'s existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn real_home_writes_the_actual_home_not_the_isolated_copy() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let real_home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", real_home.path());

        install_all(&ctx, false, true).unwrap();

        let real_claude_json = real_home.path().join(".claude.json");
        assert!(real_claude_json.exists(), "--real-home must write into the real $HOME, not the isolated homes/ dir");
        let isolated_claude_json = dir.path().join("homes").join("claude").join(".claude.json");
        assert!(!isolated_claude_json.exists(), "--real-home must not also bootstrap/write the isolated home");

        std::env::remove_var("HOME");
    }

    #[test]
    fn without_real_home_still_writes_the_isolated_copy_only() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        install_all(&ctx, false, false).unwrap();
        assert!(dir.path().join("homes").join("claude").join(".claude.json").exists());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p single-runtime --lib integrations:: -- --nocapture`
Expected: compile error — `install_all` takes 2 arguments, not 3 (same for the existing calls at lines 98, 104, 109, 120, 124 in the test module, and the real (non-test) call site expectations below).

- [ ] **Step 3: Add `real_home` to `install_all`/`uninstall_all` and fix existing call sites**

In `crates/single-runtime/src/integrations.rs`, replace the signatures and the per-agent home resolution:

```rust
pub fn install_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult> {
    let home_root = home_dir()?;
    let registry_servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    let gateway_spec = single_core::mcp::gateway_server_spec();
    let (mcp_servers, stale_names): (Vec<_>, Vec<String>) = if single_core::mcp::gateway_mode(&ctx.dirs.mcp_gateway_file())? {
        (vec![gateway_spec], registry_servers.iter().map(|s| s.name.clone()).collect())
    } else {
        (registry_servers, vec![gateway_spec.name])
    };
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = if real_home {
            home_root.clone()
        } else {
            single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &home_root, &agent.name)?
        };
        if !stale_names.is_empty() {
            writes.push(adapter.remove_mcp(&home, &stale_names, dry_run)?);
        }
        writes.push(adapter.configure_mcp(&home, &mcp_servers, dry_run)?);
        writes.push(adapter.configure_lsp(&home, &lsp_servers, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}

pub fn uninstall_all(ctx: &Context, dry_run: bool, real_home: bool) -> Result<IntegrationResult> {
    let home_root = home_dir()?;
    let mcp_servers = single_core::mcp::load(&ctx.dirs.mcp_registry_file())?;
    let mcp_names: Vec<String> =
        mcp_servers.iter().map(|s| s.name.clone()).chain(std::iter::once(single_core::mcp::gateway_server_spec().name)).collect();
    let lsp_servers = single_core::lsp::load(&ctx.dirs.lsp_registry_file())?;
    let lsp_names: Vec<String> = lsp_servers.iter().map(|s| s.name.clone()).collect();

    let mut writes = Vec::new();
    for agent in &ctx.registry {
        let Some(adapter) = for_agent_with_custom(&agent.name, &ctx.dirs.agents_dir(), &ctx.registry) else { continue };
        let home = if real_home {
            home_root.clone()
        } else {
            single_core::agent_home::ensure_bootstrapped(&ctx.dirs.homes_dir(), &home_root, &agent.name)?
        };
        writes.push(adapter.remove_mcp(&home, &mcp_names, dry_run)?);
        writes.push(adapter.remove_lsp(&home, &lsp_names, dry_run)?);
    }
    Ok(IntegrationResult { dry_run, writes })
}
```

(`home_dir()` is unchanged — still `single_core::paths::real_home_dir()`.)

Update the two existing tests in the same file (`switching_gateway_mode_replaces_rather_than_accumulates_mcp_entries`, `uninstall_removes_the_gateway_entry_even_when_gateway_mode_is_off`) to pass `false` as the new third argument at every `install_all(&ctx, false)` / `uninstall_all(&ctx, false)` call site, becoming `install_all(&ctx, false, false)` / `uninstall_all(&ctx, false, false)`.

In `crates/single-protocol/src/lib.rs`, change:

```rust
    InstallIntegrations {
        dry_run: bool,
    },
    UninstallIntegrations,
```

to:

```rust
    InstallIntegrations {
        dry_run: bool,
        real_home: bool,
    },
    UninstallIntegrations {
        real_home: bool,
    },
```

In `crates/single-runtime/src/handlers.rs`, change:

```rust
        Request::InstallIntegrations { dry_run } => Ok(ResponseData::IntegrationResult(
            integrations::install_all(ctx, dry_run)?,
        )),
        Request::UninstallIntegrations => Ok(ResponseData::IntegrationResult(
            integrations::uninstall_all(ctx, false)?,
        )),
```

to:

```rust
        Request::InstallIntegrations { dry_run, real_home } => Ok(ResponseData::IntegrationResult(
            integrations::install_all(ctx, dry_run, real_home)?,
        )),
        Request::UninstallIntegrations { real_home } => Ok(ResponseData::IntegrationResult(
            integrations::uninstall_all(ctx, false, real_home)?,
        )),
```

In `crates/single-cli/src/main.rs`, change the `Command` variants at lines 264-275:

```rust
    /// Sync SingleCLI's MCP registry into every agent's native config.
    InstallIntegrations {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
        /// Write into the real, ambient $HOME instead of the SingleCLI-managed
        /// isolated home — the only way this ever reaches an agent you run
        /// normally, outside SingleCLI. Off by default: same posture as
        /// `single task run --real-home`.
        #[arg(long)]
        real_home: bool,
    },
    /// Remove SingleCLI-managed entries from every agent's native config.
    UninstallIntegrations {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        real_home: bool,
    },
```

And the dispatch at lines 2367-2381:

```rust
        Command::InstallIntegrations { yes, json, real_home } => {
            if !yes {
                eprintln!("Dry run (pass --yes to actually write config files; backups are made either way).");
            }
            let response =
                client::send(&socket_path, Request::InstallIntegrations { dry_run: !yes, real_home })?;
            render::print(response, json);
        }
        Command::UninstallIntegrations { yes, real_home } => {
            if !yes {
                anyhow::bail!("this removes SingleCLI-managed MCP entries from every agent's config; pass --yes to confirm");
            }
            let response = client::send(&socket_path, Request::UninstallIntegrations { real_home })?;
            render::print(response, false);
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p single-runtime --lib integrations:: -- --nocapture`
Expected: PASS — all 4 tests in `integrations::tests` (2 existing, 2 new).

Run: `cargo build -p single-cli -p single-runtime -p single-protocol`
Expected: builds cleanly (confirms every call site was updated).

- [ ] **Step 5: Commit**

```bash
git add crates/single-protocol/src/lib.rs crates/single-runtime/src/integrations.rs crates/single-runtime/src/handlers.rs crates/single-cli/src/main.rs
git commit -m "feat: add --real-home to install-integrations and uninstall-integrations"
```

---

## Task 2: `--real-home` for `provider sync` and `plugin sync`

**Files:**
- Modify: `crates/single-protocol/src/lib.rs:409-413` (`Request::ProviderSync`), `:529-533` (`Request::PluginSync`)
- Modify: `crates/single-runtime/src/handlers.rs:965-999` (`Request::ProviderSync` arm), `:1134-1197` (`Request::PluginSync` arm)
- Modify: `crates/single-cli/src/main.rs:864-870` (`ProviderCommand::Sync`), `:971-977` (`PluginCommand::Sync`), `:2179-2192` (dispatch), `:2296-2311` (dispatch)
- Test: `crates/single-agent-sdk/src/provider_sync.rs`, new integration-style test in `crates/single-runtime/src/handlers.rs` is not needed — `provider_sync::sync` and `install_plugin` are already unit-tested against an arbitrary `home: &Path`; this task only needs to prove the *caller* picks the right path, which `Task 1`'s pattern already established. Cover that here with one focused test per command.

**Interfaces:**
- Consumes: nothing new from Task 1 (this task's `real_home` branching is independent, same pattern applied to two more call sites).
- Produces: nothing later tasks depend on directly — Tasks 3-6 only need Task 1's `install_all`/`uninstall_all`.

- [ ] **Step 1: Write the failing test — provider sync `--real-home` writes the real path**

`crates/single-runtime/src/handlers.rs` has no `#[cfg(test)] mod tests` yet (confirmed: `grep -n "mod tests" crates/single-runtime/src/handlers.rs` returns nothing). Add one at the bottom of the file, with its own `test_ctx` helper mirroring `integrations.rs`'s (`crates/single-runtime/src/integrations.rs:79-83`) exactly, since that one is private to its own module and not reusable across files:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use single_protocol::{ProviderSpec, Response};

    fn test_ctx(dir: &std::path::Path) -> Context {
        let dirs = single_core::SingleDirs::from_root(dir.to_path_buf());
        dirs.ensure_created().unwrap();
        Context { dirs, resolved: single_core::ResolvedConfig::default(), registry: single_core::builtin_registry() }
    }

    #[test]
    fn provider_sync_real_home_writes_the_actual_home() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let real_home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", real_home.path());

        let store = single_core::secrets::SecretTool;
        single_core::secrets::SecretStore::set(&store, "test-provider-key", "sk-test").unwrap();
        single_core::providers::add(&ctx.dirs.providers_registry_file(), ProviderSpec {
            name: "testprov".into(),
            env_var_name: "ANTHROPIC_API_KEY".into(),
            secret_name: "test-provider-key".into(),
            base_url: None,
        }).unwrap();

        let response = handle(&ctx, Request::ProviderSync { name: "testprov".into(), agents: vec!["claude".into()], dry_run: false, real_home: true });
        assert!(matches!(response, Response::Ok { .. }));
        assert!(real_home.path().join(".claude/settings.json").exists());
        assert!(!dir.path().join("homes").join("claude").join(".claude/settings.json").exists());

        std::env::remove_var("HOME");
    }
}
```

Field order/names confirmed directly: `ProviderSpec { name, env_var_name, secret_name, base_url }` (`crates/single-protocol/src/lib.rs:1325-1330`), `single_core::providers::add(path: &Path, provider: ProviderSpec) -> Result<()>` (`crates/single-core/src/providers.rs:35`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p single-runtime --lib provider_sync_real_home -- --nocapture`
Expected: compile error — `Request::ProviderSync` has no field `real_home` yet.

- [ ] **Step 3: Thread `real_home` through provider sync and plugin sync**

In `crates/single-protocol/src/lib.rs`, add `real_home: bool` to both variants:

```rust
    ProviderSync {
        name: String,
        agents: Vec<String>,
        dry_run: bool,
        real_home: bool,
    },
```

```rust
    PluginSync {
        name: String,
        agents: Vec<String>,
        dry_run: bool,
        real_home: bool,
    },
```

In `crates/single-runtime/src/handlers.rs`, in the `Request::ProviderSync { name, agents, dry_run }` arm (around line 965), add `real_home` to the destructure and branch the per-agent home resolution the same way Task 1 did:

```rust
        Request::ProviderSync {
            name,
            agents,
            dry_run,
            real_home,
        } => {
            let provider =
                single_core::providers::find(&ctx.dirs.providers_registry_file(), &name)?
                    .ok_or_else(|| anyhow::anyhow!("no such provider: {name}"))?;
            let store = single_core::secrets::SecretTool;
            let value = single_core::secrets::SecretStore::get(&store, &provider.secret_name)?
                .ok_or_else(|| anyhow::anyhow!("no key stored for provider '{name}'; run `single provider set-key {name} <value>` first"))?;
            let home_root = integrations::home_dir()?;
            let target_agents: Vec<String> = if agents.is_empty() {
                ctx.registry.iter().map(|a| a.name.clone()).collect()
            } else {
                agents
            };
            let mut results = Vec::new();
            for agent in target_agents {
                let home = if real_home {
                    home_root.clone()
                } else {
                    single_core::agent_home::ensure_bootstrapped(
                        &ctx.dirs.homes_dir(),
                        &home_root,
                        &agent,
                    )?
                };
                let mut result = single_agent_sdk::provider_sync::sync(
                    &agent,
                    &home,
                    &provider.env_var_name,
                    &value,
                    dry_run,
                )?;
                result.provider = name.clone();
                results.push(result);
            }
            Ok(ResponseData::ProviderSyncResults(results))
        }
```

Apply the same pattern to the `Request::PluginSync { name, agents, dry_run }` arm (around line 1134): add `real_home` to the destructure, rename its existing `let real_home = integrations::home_dir()?;` line to `let home_root = integrations::home_dir()?;`, and change the `let home = single_core::agent_home::ensure_bootstrapped(...)?;` inside the per-agent loop to the same `if real_home { home_root.clone() } else { ensure_bootstrapped(...)? }` branch.

In `crates/single-cli/src/main.rs`, add `#[arg(long)] real_home: bool,` to `ProviderCommand::Sync` (line 864-870) and `PluginCommand::Sync` (line 971-977), and pass `real_home` through at both dispatch sites (`ProviderCommand::Sync { name, agents, yes, real_home }` at line 2179, `PluginCommand::Sync { name, agents, yes, real_home }` at line 2296), adding `real_home` into each `Request::ProviderSync { .. }` / `Request::PluginSync { .. }` construction.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p single-runtime --lib -- --nocapture`
Expected: PASS, including the new `provider_sync_real_home_writes_the_actual_home` test and every pre-existing test in the crate (confirms no other call site was missed).

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/single-protocol/src/lib.rs crates/single-runtime/src/handlers.rs crates/single-cli/src/main.rs
git commit -m "feat: add --real-home to provider sync and plugin sync"
```

---

## Task 3: `singlecli-mcp` crate scaffold, daemon client, `task_run` and `orchestrate_run`

**Files:**
- Create: `crates/singlecli-mcp/Cargo.toml`
- Create: `crates/singlecli-mcp/src/main.rs`
- Create: `crates/singlecli-mcp/src/client.rs`
- Create: `crates/singlecli-mcp/src/server.rs`
- Modify: `Cargo.toml` (workspace `members`)
- Test: `crates/singlecli-mcp/src/server.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `singlecli_mcp::client::send(socket_path: &Path, request: single_protocol::Request) -> anyhow::Result<single_protocol::Response>` (Tasks 4-5 reuse this for every other tool), and `singlecli_mcp::server::SingleCliServer` (an `rmcp::ServerHandler` — Tasks 4-5 add match arms to its `call_tool`/entries to its `list_tools`).

- [ ] **Step 1: Add the new crate to the workspace**

In the root `Cargo.toml`, add `"crates/singlecli-mcp",` to the `members` list (alongside the existing `"crates/single-mcp",`).

- [ ] **Step 2: Scaffold `Cargo.toml`**

Create `crates/singlecli-mcp/Cargo.toml`:

```toml
[package]
name = "singlecli-mcp"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "singlecli-mcp"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
serde_json.workspace = true
tokio.workspace = true
single-core = { path = "../single-core" }
single-protocol = { path = "../single-protocol" }
single-runtime = { path = "../single-runtime" }
rmcp = { version = "3.1", features = ["server", "transport-io"] }

[dev-dependencies]
tempfile = "3"
```

(`transport-child-process`/`client` features are dropped versus `single-mcp`'s `Cargo.toml` — this binary is only ever an MCP *server*, never an MCP client spawning child processes; it talks to the SingleCLI daemon over a plain Unix socket, not via `rmcp`.)

- [ ] **Step 3: Write the failing test for the daemon client's in-process fallback**

Create `crates/singlecli-mcp/src/client.rs` test module first (TDD: write the test, then the module it tests):

```rust
//! Talk to a running `single-runtimed` over its Unix socket; if none is
//! running, fall back to calling straight into `single-runtime` in this
//! process. Deliberately duplicated from `single-cli/src/client.rs`'s
//! socket-or-fallback logic rather than extracted into a shared crate —
//! that file also owns CLI-only concerns (a `--background` warning
//! `eprintln!`) that don't belong in an MCP server's stdout/stderr, which
//! is reserved for the MCP protocol itself; ~30 lines duplicated once is a
//! smaller risk than restructuring `single-cli`'s module boundaries to
//! carve out a piece neither binary's test suite currently exercises
//! independently.

use anyhow::Result;
use single_protocol::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn send(socket_path: &Path, request: Request) -> Result<Response> {
    match UnixStream::connect(socket_path) {
        Ok(stream) => send_over(stream, &request),
        Err(_) => {
            let ctx = single_runtime::Context::load()?;
            Ok(single_runtime::handle(&ctx, request))
        }
    }
}

fn send_over(stream: UnixStream, request: &Request) -> Result<Response> {
    let mut writer = stream.try_clone()?;
    let mut payload = serde_json::to_string(request)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim_end())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_in_process_when_no_daemon_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        // No daemon socket exists at this path — connect() must fail, and
        // send() must fall back to single_runtime::handle rather than
        // erroring out.
        let socket_path = dir.path().join("nonexistent.sock");
        let response = send(&socket_path, Request::Status).unwrap();
        assert!(matches!(response, Response::Ok { .. }));
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p singlecli-mcp --lib client:: -- --nocapture`
Expected: fails to compile until the crate/module exist — this is expected on first creation; re-run after Step 3's file is saved and confirm it now actually runs and passes (this crate has no prior code, so "red" here is "doesn't build yet," which Step 3 already resolves — proceed straight to Step 5's full-pass check).

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p singlecli-mcp --lib client:: -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Write the `SingleCliServer` scaffold with `task_run` and `orchestrate_run`**

Create `crates/singlecli-mcp/src/server.rs`:

```rust
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
```

- [ ] **Step 7: Write `main.rs`**

Create `crates/singlecli-mcp/src/main.rs`:

```rust
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
```

- [ ] **Step 8: Run all tests to verify they pass**

Run: `cargo test -p singlecli-mcp --lib -- --nocapture`
Expected: PASS — `client::tests::falls_back_to_in_process_when_no_daemon_is_listening`, `server::tests::task_run_rejects_missing_description`, `server::tests::orchestrate_run_rejects_missing_agents`.

Run: `cargo build -p singlecli-mcp`
Expected: builds cleanly, produces a `singlecli-mcp` binary.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/singlecli-mcp
git commit -m "feat: scaffold singlecli-mcp with task_run and orchestrate_run tools"
```

---

## Task 4: `singlecli-mcp` — `orchestrate_parallel_run` and `orchestrate_graph_run`

**Files:**
- Modify: `crates/singlecli-mcp/src/server.rs`

**Interfaces:**
- Consumes: `Self::str_arg`/`bool_arg`/`u64_arg`, `self.send(Request) -> Value` (Task 3).
- Produces: nothing later tasks depend on.

- [ ] **Step 1: Write the failing tests**

Add to `crates/singlecli-mcp/src/server.rs`'s `#[cfg(test)] mod tests`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p singlecli-mcp --lib server:: -- --nocapture`
Expected: compile error — `parse_parallel_tasks`/`parse_graph_nodes` don't exist yet.

- [ ] **Step 3: Implement the two parsers and tools**

Add to `impl SingleCliServer` in `crates/singlecli-mcp/src/server.rs`:

```rust
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
```

Add the two tools to `list_tools`'s returned `Vec` (alongside `task_run`/`orchestrate_run`):

```rust
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
```

And two more match arms in `call_tool`:

```rust
            "orchestrate_parallel_run" => self.orchestrate_parallel_run(arguments),
            "orchestrate_graph_run" => self.orchestrate_graph_run(arguments),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p singlecli-mcp --lib server:: -- --nocapture`
Expected: PASS — all 5 tests in `server::tests`.

- [ ] **Step 5: Commit**

```bash
git add crates/singlecli-mcp/src/server.rs
git commit -m "feat: add orchestrate_parallel_run and orchestrate_graph_run to singlecli-mcp"
```

---

## Task 5: `singlecli-mcp` — `agent_list`, `agent_inspect`, `memory_store`, `memory_search`, `provider_configured_list`

**Files:**
- Modify: `crates/singlecli-mcp/src/server.rs`

**Interfaces:**
- Consumes: Task 3's `self.send`/arg helpers.
- Produces: nothing later tasks depend on — this is the last tool-adding task; Task 6 registers the *binary* into Claude Code's config, not a tool.

- [ ] **Step 1: Write the failing tests**

Add to `#[cfg(test)] mod tests`:

```rust
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
```

(`agent_list` and `provider_configured_list` take no arguments, so they have nothing to unit-test at the parsing level beyond what `call_tool`'s dispatch itself exercises — that's covered by this task's build succeeding and Task 9's live smoke test, not a unit test here.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p singlecli-mcp --lib server:: -- --nocapture`
Expected: both new tests actually pass immediately since they only exercise the already-existing `str_arg` helper — this task's *tools* are what's new, not argument parsing primitives. Confirm the crate still builds, then proceed straight to Step 3 (this is one of the rare cases where the "test" step doubles as a regression check on Task 3/4's helpers rather than a true red step for genuinely new logic — the real coverage for this task's tool-dispatch wiring is the build in Step 4 plus Task 9's live smoke test).

- [ ] **Step 3: Implement the five tools**

Add to `impl SingleCliServer`:

```rust
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
```

Add tools to `list_tools`:

```rust
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
```

Add match arms to `call_tool`:

```rust
            "agent_list" => self.agent_list(),
            "agent_inspect" => self.agent_inspect(arguments),
            "memory_store" => self.memory_store(arguments),
            "memory_search" => self.memory_search(arguments),
            "provider_configured_list" => self.provider_configured_list(),
```

(Every tool method — including the two that take no arguments — now returns `anyhow::Result<Value>` uniformly, matching Task 3's `send` helper; the existing `match result { Ok(value) => ..., Err(e) => CallToolResult::error(...) }` block in `call_tool` needs no changes.)

- [ ] **Step 4: Run tests and build**

Run: `cargo test -p singlecli-mcp --lib -- --nocapture`
Expected: PASS — all 7 tests in `server::tests`.

Run: `cargo build -p singlecli-mcp`
Expected: builds cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/singlecli-mcp/src/server.rs
git commit -m "feat: add agent, memory, and provider tools to singlecli-mcp"
```

---

## Task 6: Register `singlecli-mcp` as a fixed entry synced by `install-integrations`

**Files:**
- Modify: `crates/single-core/src/mcp.rs` (near `gateway_server_spec`, per the earlier finding at `crates/single-core/src/mcp.rs:384-386`)
- Modify: `crates/single-runtime/src/integrations.rs` (`install_all`, `uninstall_all` — built in Task 1)
- Test: `crates/single-runtime/src/integrations.rs`

**Interfaces:**
- Consumes: Task 1's `install_all(ctx, dry_run, real_home)` / `uninstall_all(ctx, dry_run, real_home)`.
- Produces: `single_core::mcp::singlecli_server_spec() -> single_protocol::McpServerSpec` — a fixed spec, unconditionally included every sync (not gated behind gateway mode, since `singlecli-mcp` isn't a registry-toggleable proxy target — it's SingleCLI's own always-on delegation surface).

- [ ] **Step 1: Write the failing test**

Add to `crates/single-runtime/src/integrations.rs`'s test module:

```rust
    #[test]
    fn singlecli_mcp_is_always_included_regardless_of_gateway_mode() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());

        install_all(&ctx, false, false).unwrap();
        assert!(claude_mcp_server_names(dir.path()).contains(&"singlecli-mcp".to_string()));

        single_core::mcp::set_gateway_mode(&ctx.dirs.mcp_gateway_file(), true).unwrap();
        install_all(&ctx, false, false).unwrap();
        let names = claude_mcp_server_names(dir.path());
        assert!(names.contains(&"single-mcp".to_string()));
        assert!(names.contains(&"singlecli-mcp".to_string()), "singlecli-mcp must survive a gateway-mode switch, unlike the registry servers it isn't one of");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p single-runtime --lib integrations::tests::singlecli_mcp_is_always_included -- --nocapture`
Expected: FAIL — `singlecli_server_spec` doesn't exist, `singlecli-mcp` never gets written.

- [ ] **Step 3: Add `singlecli_server_spec` and always-include it**

Find `gateway_server_spec` in `crates/single-core/src/mcp.rs` (established at `crates/single-core/src/mcp.rs:384-386`) and add a sibling function directly after it:

```rust
/// The fixed spec for `singlecli-mcp` — SingleCLI's own self-exposing MCP
/// server (task/orchestrate/agent/memory/provider tools). Unlike
/// `gateway_server_spec`, this isn't conditional on gateway mode: it's
/// always synced, since it isn't one of the registry servers gateway mode
/// toggles between proxying individually vs. through `single-mcp`.
pub fn singlecli_server_spec() -> single_protocol::McpServerSpec {
    single_protocol::McpServerSpec {
        name: "singlecli-mcp".into(),
        command: "singlecli-mcp".into(),
        args: vec![],
        env: std::collections::BTreeMap::new(),
        secret_env: std::collections::BTreeMap::new(),
        enabled: true,
    }
}
```

(Field set confirmed directly against `gateway_server_spec`'s own body at `crates/single-core/src/mcp.rs:385` and `McpServerSpec`'s definition at `crates/single-protocol/src/lib.rs:833-843` — six fields: `name`, `command`, `args`, `env: BTreeMap<String, String>`, `secret_env: BTreeMap<String, String>`, `enabled`.)

In `crates/single-runtime/src/integrations.rs`, update `install_all` to always append the singlecli-mcp spec to `mcp_servers` regardless of which branch gateway mode took:

```rust
    let (mut mcp_servers, stale_names): (Vec<_>, Vec<String>) = if single_core::mcp::gateway_mode(&ctx.dirs.mcp_gateway_file())? {
        (vec![gateway_spec], registry_servers.iter().map(|s| s.name.clone()).collect())
    } else {
        (registry_servers, vec![gateway_spec.name])
    };
    mcp_servers.push(single_core::mcp::singlecli_server_spec());
```

(Note the `let (mut mcp_servers, ...)` — the binding needs `mut` now that it's pushed into after the branch.)

And in `uninstall_all`, add `singlecli-mcp`'s name to the removal list:

```rust
    let mcp_names: Vec<String> = mcp_servers
        .iter()
        .map(|s| s.name.clone())
        .chain(std::iter::once(single_core::mcp::gateway_server_spec().name))
        .chain(std::iter::once(single_core::mcp::singlecli_server_spec().name))
        .collect();
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p single-runtime --lib integrations:: -- --nocapture`
Expected: PASS — all tests in `integrations::tests`, including the two from Task 1 and the new one here.

Run: `cargo build --workspace`
Expected: builds cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/single-core/src/mcp.rs crates/single-runtime/src/integrations.rs
git commit -m "feat: always sync singlecli-mcp alongside the mcp registry/gateway"
```

---

## Task 7: `single-lsp` — dynamic LSP proxy binary

**Files:**
- Create: `crates/single-lsp/Cargo.toml`
- Create: `crates/single-lsp/src/main.rs`
- Create: `crates/single-lsp/src/framing.rs`
- Create: `crates/single-lsp/src/proxy.rs`
- Modify: `Cargo.toml` (workspace `members`)
- Test: `crates/single-lsp/src/framing.rs`, `crates/single-lsp/src/proxy.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `single_lsp::framing::{read_message, write_message}` (Content-Length JSON-RPC framing over any `Read`/`Write`), `single_lsp::proxy::Router::route(uri: &str) -> Option<single_protocol::LspServerSpec>` (extension → backend spec lookup, used by Task 8's manifest generator as the same source of truth for which extensions are covered).

**Design note (read before starting):** Claude Code spawns exactly one process per `lspServers` entry in a plugin's marketplace manifest — see Task 8's `extensionToLanguage` map, which will list 150+ extensions under this one `single-lsp` command. So this one process must handle files from many different real languages within a single session, proxying each open document to the correct real language server based on its file extension, while multiple such backends may be alive concurrently (e.g. a Rust file and a Python file open at once). Forwarding is generic — driven by inspecting `params.textDocument.uri` when a message has one — not a hand-maintained list of supported LSP methods, per this plan's Global Constraints.

- [ ] **Step 1: Add the new crate to the workspace**

In the root `Cargo.toml`, add `"crates/single-lsp",` to `members`.

- [ ] **Step 2: Scaffold `Cargo.toml`**

Create `crates/single-lsp/Cargo.toml`:

```toml
[package]
name = "single-lsp"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "single-lsp"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
serde_json.workspace = true
single-core = { path = "../single-core" }
single-protocol = { path = "../single-protocol" }

[dev-dependencies]
tempfile = "3"
```

(No `tokio`/`rmcp` — this proxy is synchronous, one OS thread per backend connection, matching the simplest correct design for a handful of concurrently-open languages in one editor session; no need for an async runtime here.)

- [ ] **Step 3: Write the failing tests for Content-Length framing**

Create `crates/single-lsp/src/framing.rs`:

```rust
//! Hand-rolled LSP wire framing: `Content-Length: <n>\r\n\r\n<n bytes of JSON>`,
//! read/written on both sides of this proxy (Claude Code ↔ proxy, and
//! proxy ↔ each real backend language server) — the same framing every
//! LSP implementation uses. Kept as raw `serde_json::Value` rather than
//! typed `lsp-types` structs: this proxy only needs to read `id` and
//! `params.textDocument.uri` to route, never the full message shape, so a
//! typed dependency would buy nothing here.

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, Read, Write};

pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).context("reading LSP header line")?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let line = line.trim_end();
        if line.is_empty() {
            break; // blank line ends the header block
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            content_length = Some(value.trim().parse().context("parsing Content-Length")?);
        }
        // Any other header (e.g. Content-Type) is read and discarded — this proxy never sends one.
    }
    let content_length = content_length.context("LSP message had no Content-Length header")?;
    let mut buf = vec![0u8; content_length];
    reader.read_exact(&mut buf).context("reading LSP message body")?;
    let value: Value = serde_json::from_slice(&buf).context("parsing LSP message body as JSON")?;
    Ok(Some(value))
}

pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message).context("serializing LSP message")?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).context("writing LSP header")?;
    writer.write_all(&body).context("writing LSP message body")?;
    writer.flush().context("flushing LSP message")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::BufReader;

    #[test]
    fn round_trips_a_message() {
        let mut buf: Vec<u8> = Vec::new();
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        write_message(&mut buf, &message).unwrap();

        let mut reader = BufReader::new(buf.as_slice());
        let read_back = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(read_back, message);
    }

    #[test]
    fn returns_none_at_eof() {
        let mut reader = BufReader::new([].as_slice());
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn ignores_unrelated_headers() {
        let mut buf: Vec<u8> = Vec::new();
        let body = serde_json::to_vec(&json!({ "ok": true })).unwrap();
        write!(&mut buf, "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: {}\r\n\r\n", body.len()).unwrap();
        buf.extend_from_slice(&body);

        let mut reader = BufReader::new(buf.as_slice());
        let read_back = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(read_back, json!({ "ok": true }));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p single-lsp --lib framing:: -- --nocapture`
Expected: PASS — all 3 tests (this module's logic is simple enough that red/green collapse into "write it, then confirm it works," same as Task 5's argument-parsing tests — there is no pre-existing framing code to be red against).

- [ ] **Step 5: Write the failing test for extension routing**

Create `crates/single-lsp/src/proxy.rs`:

```rust
//! Routes an open document's URI to the SingleCLI LSP registry entry that
//! should handle it, and manages the spawned backend processes.

use anyhow::{Context, Result};
use single_protocol::LspServerSpec;
use std::collections::HashMap;

pub struct Router {
    by_extension: HashMap<String, LspServerSpec>,
}

impl Router {
    pub fn from_registry(specs: Vec<LspServerSpec>) -> Self {
        let mut by_extension = HashMap::new();
        for spec in specs.into_iter().filter(|s| s.enabled) {
            for ext in &spec.extensions {
                // First registered preset for a given extension wins; SingleCLI's
                // own registry is the source of truth for which preset is
                // "the" handler for an extension, same as `single lsp list`
                // shows only one row per extension in practice.
                by_extension.entry(ext.clone()).or_insert_with(|| spec.clone());
            }
        }
        Self { by_extension }
    }

    pub fn route(&self, uri: &str) -> Option<&LspServerSpec> {
        let ext = extension_of(uri)?;
        self.by_extension.get(&ext)
    }
}

fn extension_of(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let dot = path.rfind('.')?;
    Some(path[dot..].to_string())
}

pub fn load_registry() -> Result<Vec<LspServerSpec>> {
    let dirs = single_core::SingleDirs::discover().context("resolving SingleCLI config directory")?;
    single_core::lsp::load(&dirs.lsp_registry_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, exts: &[&str]) -> LspServerSpec {
        LspServerSpec {
            name: name.to_string(),
            command: name.to_string(),
            args: Vec::new(),
            extensions: exts.iter().map(|e| e.to_string()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn routes_by_extension() {
        let router = Router::from_registry(vec![spec("rust-analyzer", &[".rs"]), spec("pyright", &[".py", ".pyi"])]);
        assert_eq!(router.route("file:///project/src/main.rs").unwrap().name, "rust-analyzer");
        assert_eq!(router.route("file:///project/lib.pyi").unwrap().name, "pyright");
        assert!(router.route("file:///project/README.md").is_none());
    }

    #[test]
    fn ignores_disabled_presets() {
        let mut disabled = spec("gopls", &[".go"]);
        disabled.enabled = false;
        let router = Router::from_registry(vec![disabled]);
        assert!(router.route("file:///main.go").is_none());
    }

    #[test]
    fn extension_of_handles_file_uri_and_multi_dot_names() {
        assert_eq!(extension_of("file:///a/b.test.tsx").as_deref(), Some(".tsx"));
        assert_eq!(extension_of("file:///a/noext"), None);
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p single-lsp --lib proxy:: -- --nocapture`
Expected: PASS — all 3 tests.

- [ ] **Step 7: Implement the backend-spawning multiplexer**

Extend `crates/single-lsp/src/proxy.rs` with the stateful multiplexer that `main.rs` will drive. This is the core routing loop: read from stdin, dispatch, write to stdout, spawning/reusing backend child processes keyed by preset name (not by URI — two `.rs` files share one `rust-analyzer`).

```rust
use crate::framing::{read_message, write_message};
use std::io::{BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use serde_json::Value;

struct Backend {
    _child: Child,
    stdin: ChildStdin,
}

pub struct Multiplexer {
    router: Router,
    backends: Mutex<HashMap<String, Backend>>,
    uri_to_backend: Mutex<HashMap<String, String>>,
    next_id: Mutex<i64>,
    pending: Mutex<HashMap<i64, (Value, String)>>, // proxy-assigned id -> (original client id, backend name)
    client_out: Sender<Value>,
}

impl Multiplexer {
    pub fn new(router: Router, client_out: Sender<Value>) -> Arc<Self> {
        Arc::new(Self {
            router,
            backends: Mutex::new(HashMap::new()),
            uri_to_backend: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            pending: Mutex::new(HashMap::new()),
            client_out,
        })
    }

    fn extract_uri(message: &Value) -> Option<String> {
        message.pointer("/params/textDocument/uri").and_then(Value::as_str).map(str::to_string)
    }

    /// Spawns (or reuses) the backend for `spec`, starting its reader thread on first spawn.
    fn ensure_backend(self: &Arc<Self>, spec: &single_protocol::LspServerSpec) -> anyhow::Result<()> {
        let mut backends = self.backends.lock().unwrap();
        if backends.contains_key(&spec.name) {
            return Ok(());
        }
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        backends.insert(spec.name.clone(), Backend { _child: child, stdin });
        drop(backends);

        let this = self.clone();
        let backend_name = spec.name.clone();
        std::thread::spawn(move || this.read_from_backend(backend_name, stdout));
        Ok(())
    }

    fn read_from_backend(self: Arc<Self>, backend_name: String, stdout: ChildStdout) {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader) {
                Ok(Some(mut message)) => {
                    if let Some(id) = message.get("id").and_then(Value::as_i64) {
                        let mut pending = self.pending.lock().unwrap();
                        if let Some((client_id, owner)) = pending.remove(&id) {
                            if owner == backend_name {
                                message["id"] = client_id;
                                let _ = self.client_out.send(message);
                            }
                        }
                    } else {
                        // Notification from the backend (e.g. publishDiagnostics) — forward as-is.
                        let _ = self.client_out.send(message);
                    }
                }
                Ok(None) | Err(_) => break, // backend exited or its pipe broke
            }
        }
    }

    /// Handles one message received from the client (Claude Code).
    pub fn handle_client_message(self: &Arc<Self>, message: Value) -> anyhow::Result<()> {
        let method = message.get("method").and_then(Value::as_str);
        if method == Some("shutdown") || method == Some("exit") {
            let backends = self.backends.lock().unwrap();
            for backend in backends.values() {
                let mut stdin = &backend.stdin;
                let _ = write_message(&mut stdin, &message);
            }
            return Ok(());
        }

        let Some(uri) = Self::extract_uri(&message) else {
            // No routable document (e.g. `initialize` itself) — nothing to forward yet.
            return Ok(());
        };

        let backend_name = {
            let mut uri_map = self.uri_to_backend.lock().unwrap();
            if let Some(name) = uri_map.get(&uri) {
                name.clone()
            } else {
                let spec = self.router.route(&uri).context("no LSP preset registered for this file's extension")?.clone();
                self.ensure_backend(&spec)?;
                uri_map.insert(uri.clone(), spec.name.clone());
                spec.name.clone()
            }
        };

        if method == Some("textDocument/didClose") {
            self.uri_to_backend.lock().unwrap().remove(&uri);
        }

        let mut outgoing = message.clone();
        if let Some(client_id) = message.get("id").cloned().filter(|v| !v.is_null()) {
            let mut next_id = self.next_id.lock().unwrap();
            let proxy_id = *next_id;
            *next_id += 1;
            drop(next_id);
            self.pending.lock().unwrap().insert(proxy_id, (client_id, backend_name.clone()));
            outgoing["id"] = Value::from(proxy_id);
        }

        let backends = self.backends.lock().unwrap();
        let backend = backends.get(&backend_name).context("backend disappeared after ensure_backend")?;
        let mut stdin = &backend.stdin;
        write_message(&mut stdin, &outgoing)
    }
}
```

Add `use anyhow::Context;` and `use std::collections::HashMap;` to the top of the file alongside the existing imports.

- [ ] **Step 8: Write the failing test for message routing**

Add to `crates/single-lsp/src/proxy.rs`'s test module:

```rust
    #[test]
    fn extract_uri_reads_textdocument_uri() {
        let message = serde_json::json!({ "method": "textDocument/hover", "params": { "textDocument": { "uri": "file:///a.rs" } } });
        assert_eq!(Multiplexer::extract_uri(&message).as_deref(), Some("file:///a.rs"));
    }

    #[test]
    fn extract_uri_returns_none_when_absent() {
        let message = serde_json::json!({ "method": "initialize", "params": {} });
        assert!(Multiplexer::extract_uri(&message).is_none());
    }
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cargo test -p single-lsp --lib proxy:: -- --nocapture`
Expected: PASS — all 5 tests in `proxy::tests`.

Run: `cargo build -p single-lsp`
Expected: builds cleanly.

(The spawn/routing/backend-lifecycle logic in `Multiplexer` is exercised by the unit tests above only at the pure-function level (`extract_uri`) plus `Router`'s own tests — actually spawning a real child process and round-tripping a full `initialize` handshake against it is covered live in Task 9's manual verification pass, not here, since it needs a real installed language server binary to be meaningful rather than a fake one.)

- [ ] **Step 10: Write `main.rs`**

Create `crates/single-lsp/src/main.rs`:

```rust
//! `single-lsp`: a dynamic LSP proxy. Claude Code spawns this one process
//! for every extension registered in its plugin manifest (see
//! `docs/superpowers/specs/2026-08-24-claude-code-singlecli-integration-design.md`);
//! it routes each open document to the real language server SingleCLI's
//! LSP registry maps that extension to, spawning backends lazily and
//! reusing them for documents of the same language — see `proxy.rs`.

mod framing;
mod proxy;

use framing::{read_message, write_message};
use proxy::{load_registry, Multiplexer, Router};
use std::io::{BufReader, BufWriter};
use std::sync::mpsc::channel;

fn main() -> anyhow::Result<()> {
    let registry = load_registry()?;
    let router = Router::from_registry(registry);

    let (client_out_tx, client_out_rx) = channel();
    let multiplexer = Multiplexer::new(router, client_out_tx);

    // Writer thread: drains backend responses/notifications and the
    // proxy's own replies onto stdout, serialized through one channel so
    // concurrent backend reader threads never interleave partial writes.
    std::thread::spawn(move || {
        let mut writer = BufWriter::new(std::io::stdout());
        while let Ok(message) = client_out_rx.recv() {
            let _ = write_message(&mut writer, &message);
        }
    });

    let mut reader = BufReader::new(std::io::stdin());
    while let Some(message) = read_message(&mut reader)? {
        multiplexer.handle_client_message(message)?;
    }
    Ok(())
}
```

- [ ] **Step 11: Run all tests one more time and build**

Run: `cargo test -p single-lsp --lib -- --nocapture`
Expected: PASS — all tests across `framing::tests` and `proxy::tests`.

Run: `cargo build -p single-lsp`
Expected: builds cleanly, produces a `single-lsp` binary.

- [ ] **Step 12: Commit**

```bash
git add Cargo.toml crates/single-lsp
git commit -m "feat: add single-lsp dynamic LSP proxy"
```

---

## Task 8: `single-lsp` Claude Code plugin packaging

**Files:**
- Create: `crates/single-cli/src/internal_lsp_manifest.rs`
- Modify: `crates/single-cli/src/main.rs` (`enum InternalCommand`, around line 282, and its dispatch)
- Create: (generated at runtime, not committed) a small marketplace repo layout — this task produces the *generator*; running it is part of Task 9.

**Interfaces:**
- Consumes: `single_core::lsp::load` (same registry `single-lsp`'s own `proxy::load_registry` reads — both must agree on what's covered, which is why the generator lives in the same repo rather than being hand-maintained separately).
- Produces: `single generate-lsp-plugin-manifest <output-dir>` internal command, writing a `.claude-plugin/marketplace.json` (and a `plugins/single-lsp/README.md` stub, since every other marketplace entry in `claude-plugins-official` ships one) into `<output-dir>`.

- [ ] **Step 1: Write the failing test**

Create `crates/single-cli/src/internal_lsp_manifest.rs`:

```rust
//! Generates the `.claude-plugin/marketplace.json` entry for `single-lsp`
//! from SingleCLI's own LSP registry — the same registry `single-lsp`'s
//! `proxy::Router` reads at runtime, so the manifest's `extensionToLanguage`
//! map and the proxy's actual routing table can never drift apart as long
//! as this generator is re-run after registry changes (`single lsp add`,
//! `single lsp enable`, ...) — see `single lsp --help`'s note on
//! `install-integrations` needing a re-run after registry edits, same idea.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use single_protocol::LspServerSpec;
use std::path::Path;

pub fn generate(specs: &[LspServerSpec]) -> Value {
    let mut extension_to_language: Map<String, Value> = Map::new();
    for spec in specs.iter().filter(|s| s.enabled) {
        for ext in &spec.extensions {
            extension_to_language.entry(ext.clone()).or_insert_with(|| Value::String(spec.name.clone()));
        }
    }
    json!({
        "$schema": "https://json.schemastore.org/claude-code-marketplace.json",
        "name": "single-lsp-marketplace",
        "description": "Dynamic LSP proxy for SingleCLI's unified language server registry.",
        "owner": { "name": "Navin B. Ruas", "email": "founder@nbr.company" },
        "plugins": [
            {
                "name": "single-lsp",
                "description": "Dynamically proxies to whichever real language server SingleCLI's registry maps your open file's extension to.",
                "version": "0.1.0",
                "author": { "name": "Navin B. Ruas", "email": "founder@nbr.company" },
                "source": "./plugins/single-lsp",
                "category": "development",
                "strict": false,
                "lspServers": {
                    "single-lsp": {
                        "command": "single-lsp",
                        "args": [],
                        "extensionToLanguage": extension_to_language
                    }
                }
            }
        ]
    })
}

pub fn write_to(output_dir: &Path, specs: &[LspServerSpec]) -> Result<()> {
    let manifest_dir = output_dir.join(".claude-plugin");
    std::fs::create_dir_all(&manifest_dir).with_context(|| format!("creating {}", manifest_dir.display()))?;
    let manifest_path = manifest_dir.join("marketplace.json");
    let manifest = generate(specs);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?).with_context(|| format!("writing {}", manifest_path.display()))?;

    let plugin_dir = output_dir.join("plugins").join("single-lsp");
    std::fs::create_dir_all(&plugin_dir).with_context(|| format!("creating {}", plugin_dir.display()))?;
    std::fs::write(
        plugin_dir.join("README.md"),
        "# single-lsp\n\nDynamic LSP proxy — see SingleCLI's docs/superpowers/specs/2026-08-24-claude-code-singlecli-integration-design.md.\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, exts: &[&str]) -> LspServerSpec {
        LspServerSpec { name: name.to_string(), command: name.to_string(), args: Vec::new(), extensions: exts.iter().map(|e| e.to_string()).collect(), enabled: true }
    }

    #[test]
    fn generates_extension_to_language_from_enabled_presets_only() {
        let mut disabled = spec("elm", &[".elm"]);
        disabled.enabled = false;
        let manifest = generate(&[spec("rust-analyzer", &[".rs"]), disabled]);
        let ext_map = &manifest["plugins"][0]["lspServers"]["single-lsp"]["extensionToLanguage"];
        assert_eq!(ext_map[".rs"], "rust-analyzer");
        assert!(ext_map.get(".elm").is_none());
    }

    #[test]
    fn write_to_creates_manifest_and_plugin_stub() {
        let dir = tempfile::tempdir().unwrap();
        write_to(dir.path(), &[spec("gopls", &[".go"])]).unwrap();
        assert!(dir.path().join(".claude-plugin/marketplace.json").exists());
        assert!(dir.path().join("plugins/single-lsp/README.md").exists());
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join(".claude-plugin/marketplace.json")).unwrap()).unwrap();
        assert_eq!(manifest["plugins"][0]["lspServers"]["single-lsp"]["extensionToLanguage"][".go"], "gopls");
    }
}
```

Add `tempfile = "3"` to `crates/single-cli/Cargo.toml`'s `[dev-dependencies]` if it isn't already present (check first: `grep -n "tempfile" crates/single-cli/Cargo.toml`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p single-cli --lib internal_lsp_manifest:: -- --nocapture`
Expected: fails to compile — module not yet wired into `crates/single-cli/src/main.rs`'s module tree.

- [ ] **Step 3: Wire the module and add the internal command**

In `crates/single-cli/src/main.rs`, add `mod internal_lsp_manifest;` near the other `mod` declarations at the top of the file (check the existing pattern with `grep -n "^mod "` first and place it alongside).

In the `InternalCommand` enum (starting at line 282), add a new variant:

```rust
    /// Regenerates the single-lsp Claude Code plugin's marketplace manifest
    /// from the current LSP registry — re-run after `single lsp add`/
    /// `enable`/`disable` so the plugin's extensionToLanguage map stays in
    /// sync with what single-lsp itself actually routes.
    GenerateLspPluginManifest {
        output_dir: String,
    },
```

Find where other `InternalCommand` variants are dispatched (search for the existing dispatch `match` arm pattern with `grep -n "InternalCommand::" crates/single-cli/src/main.rs`) and add:

```rust
        InternalCommand::GenerateLspPluginManifest { output_dir } => {
            let dirs = single_core::SingleDirs::discover()?;
            let specs = single_core::lsp::load(&dirs.lsp_registry_file())?;
            internal_lsp_manifest::write_to(std::path::Path::new(&output_dir), &specs)?;
            println!("wrote single-lsp plugin manifest to {output_dir}");
        }
```

(Match this dispatch arm's placement to wherever the existing `InternalCommand` handling runs — before or after the socket-based dispatch, following whatever the existing internal commands do, since `Command::Internal(_)` is unreachable in the main socket-dispatch `match` per line 2383's existing `unreachable!("handled before the socket-based dispatch above")`.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p single-cli --lib internal_lsp_manifest:: -- --nocapture`
Expected: PASS — both tests.

Run: `cargo build -p single-cli`
Expected: builds cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/single-cli/src/internal_lsp_manifest.rs crates/single-cli/src/main.rs crates/single-cli/Cargo.toml
git commit -m "feat: generate single-lsp claude code plugin manifest from the lsp registry"
```

---

## Task 9: Rewire `~/.claude` and verify end to end

This task operates on the live machine, not just the repo — it's the sequencing step the spec's section 5 requires: nothing is removed from Navin's real Claude Code config until the replacement is proven working. It is verification-and-configuration, not new source code, so it does not follow the red/green/commit shape of the tasks above; each step is still concrete and checkable.

**Files touched (outside this repo):**
- `~/.claude.json` (MCP servers)
- `~/.claude/settings.json` (`enabledPlugins`)
- `~/.claude/CLAUDE.md` (guidance update)

- [ ] **Step 1: Build and install the new binaries**

```bash
cd /home/navinbruas/Projects/The-Company/nbr-vault/Development/Repositories/naviNBRuas/SingleCLI
cargo build --release -p single-mcp -p singlecli-mcp -p single-lsp -p single-cli
cp target/release/single-mcp target/release/singlecli-mcp target/release/single-lsp target/release/single ~/.local/bin/
```

Verify: `which single-mcp singlecli-mcp single-lsp` all resolve under `~/.local/bin`.

- [ ] **Step 2: Enable gateway mode and set up the LSP marketplace**

```bash
single mcp gateway enable
single lsp enable-all
mkdir -p ~/.config/single/single-lsp-marketplace
single internal generate-lsp-plugin-manifest ~/.config/single/single-lsp-marketplace
```

Verify: `cat ~/.config/single/single-lsp-marketplace/.claude-plugin/marketplace.json | python3 -m json.tool | head -20` shows a real `extensionToLanguage` map with well over 100 entries.

- [ ] **Step 3: Ensure registry parity for what's being removed**

Before removing anything from `~/.claude`, confirm every capability being replaced is actually enabled in Single's registry with any needed secret set:

```bash
single mcp list | grep -E "^(github|playwright|notion|gdrive|google-calendar)\b"
```

For any shown `[disabled]`, run `single mcp enable <name>`. For `notion`/`gdrive`/`google-calendar`, set the secrets their preset needs first (`single mcp inspect notion` etc. shows which `secret_env` keys are required) with `single secret set <key> <value>`, then enable.

- [ ] **Step 4: Sync SingleCLI's config into the real Claude Code home**

```bash
single install-integrations --real-home --yes --json
```

Verify: the JSON output's `writes` list shows `applied: true` for the `claude` agent's MCP write; then:

```bash
claude mcp list
```

Expected: `single-mcp` and `singlecli-mcp` both show `✔ Connected`.

- [ ] **Step 5: Install the single-lsp plugin**

```bash
claude plugin marketplace add ~/.config/single/single-lsp-marketplace
claude plugin install single-lsp@single-lsp-marketplace
```

Verify: `claude plugin list` (or the equivalent settings check) shows `single-lsp` enabled.

- [ ] **Step 6: Manually verify single-lsp on real files**

Open a Claude Code session in a directory with at least a `.rs` and a `.py` file, and exercise hover/diagnostics/go-to-definition on both. Confirm both resolve correctly (proving the multiplexer is actually routing two different concurrently-open languages to two different spawned backends, not just one).

- [ ] **Step 7: Smoke-test both new MCP servers live**

In a Claude Code session:
- Call a tool that round-trips through `single-mcp` (e.g. `list_available_mcp_tools`, then `invoke_mcp` against `github` or `playwright` with `tool: null` to confirm it lists that server's real tools).
- Call `singlecli-mcp`'s `agent_list` tool and confirm it returns the real detected-agent list.
- Call `singlecli-mcp`'s `task_run` with a trivial prompt against a lightweight agent (e.g. `codex` if installed) and confirm the result round-trips.

- [ ] **Step 8: Remove the superseded plugins and connectors**

Only after Steps 4-7 all verify green:

```bash
claude plugin uninstall github@claude-plugins-official
claude plugin uninstall playwright@claude-plugins-official
claude plugin uninstall rust-analyzer-lsp@claude-plugins-official
claude plugin uninstall typescript-lsp@claude-plugins-official
claude plugin uninstall pyright-lsp@claude-plugins-official
claude plugin uninstall gopls-lsp@claude-plugins-official
```

For the four `claude.ai` OAuth connectors (Notion, Gmail, Drive, Calendar), remove them through whatever UI/command surface added them (these are not plugin-based, so `claude plugin uninstall` doesn't apply — check `claude mcp remove --help` or the equivalent settings UI for connector-scoped removal, since this project's own exploration didn't find a CLI-only path for these four).

Before this step, snapshot the files being touched:

```bash
cp ~/.claude.json ~/.claude.json.bak-2026-08-24
cp ~/.claude/settings.json ~/.claude/settings.json.bak-2026-08-24
```

- [ ] **Step 9: Re-verify after removal**

```bash
claude mcp list
```

Expected: exactly `single-mcp` and `singlecli-mcp`, both connected — nothing else.

Confirm plugin list / settings show no LSP-only plugins remaining besides `single-lsp`.

- [ ] **Step 10: Update `~/.claude/CLAUDE.md` guidance**

Add a short section (near the existing "Reach for installed plugins" section) noting: MCP/LSP servers are now configured exclusively through SingleCLI (`single mcp add`/`enable`, `single lsp add`/`enable`, then `single install-integrations --real-home --yes`) — never edit `~/.claude.json`'s `mcpServers` by hand again; and that `singlecli-mcp`'s `task_run`/`orchestrate_*` tools are the mechanism for delegating work to other agents/models to save Claude tokens.

- [ ] **Step 11: Bump the workspace version and do the final commit**

In the SingleCLI repo's root `Cargo.toml`, bump `version.workspace` from `0.5.0` to `0.6.0` (new functionality, backwards-compatible — a `feat`-level minor bump per this plan's Global Constraints).

```bash
cd /home/navinbruas/Projects/The-Company/nbr-vault/Development/Repositories/naviNBRuas/SingleCLI
git add Cargo.toml
git commit -m "chore: bump workspace version to 0.6.0 for the claude code integration"
```

(The `~/.claude` config changes and the `~/.claude/CLAUDE.md` edit are outside this git repo and are not part of this commit — `~/.claude` is not itself a git repository per this session's own environment info, so there is nothing to commit there; the `.bak` snapshots from Step 8 are the safety net for that side.)
