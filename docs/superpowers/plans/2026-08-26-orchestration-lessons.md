# Orchestration Lessons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface rate-limit failures, per-agent `real_home` correctness, and "not invocable" agents as queryable data instead of tribal knowledge; add a per-agent concurrency guard; add an assisted (never automatic) git worktree merge-back path.

**Architecture:** Extend the existing `AgentDefinition`/`CapabilityFlags`/`TaskRecord`/`AgentInfo` structs in place (all additive), enforce at the existing `task.rs` spawn/finish call sites, add two new `singlecli-mcp` tools for merge-back.

**Tech Stack:** Rust, rusqlite (SQLite), the existing `single-core`/`single-protocol`/`single-runtime`/`single-cli`/`singlecli-mcp` crate structure.

**Spec:** `docs/superpowers/specs/2026-08-26-orchestration-lessons-design.md`

## Global Constraints

- All new struct fields are additive (no existing caller breaks) — see spec's Approach section.
- Commit format: `type: description`, no `Co-Authored-By` or other trailer, sole author `Navin B. Ruas <founder@nbr.company>` (set locally in this repo, already configured).
- Version: workspace `Cargo.toml` is `0.6.0` → bump to `0.7.0` in the final task once everything else lands.
- `HomeRequirement::Unverified` is the honest default for any agent not empirically confirmed — never guess a value for an unconfirmed agent (see spec, matches this project's existing ethos in `ratelimit.rs`'s doc comment).
- Every `cargo test`/`cargo build` command below runs from the repo root: `/home/navinbruas/Projects/The-Company/nbr-vault/Development/Repositories/naviNBRuas/SingleCLI`

---

### Task 1: Registry data model — `HomeRequirement`, `max_concurrency`, `non_interactive_run`

**Files:**
- Modify: `crates/single-core/src/registry.rs:1-619` (struct def + all 22 `AgentDefinition` literals)
- Modify: `crates/single-protocol/src/lib.rs:766-774` (`CapabilityFlags`), `:702-722` (`AgentInfo`)
- Modify: `crates/single-runtime/src/handlers.rs:1705-1723` (`to_agent_info`)
- Modify: `crates/single-cli/src/render.rs:63-88` (`agent inspect` output)
- Test: `crates/single-core/src/registry.rs` (new `#[cfg(test)]` module additions)

**Interfaces:**
- Produces: `single_core::registry::HomeRequirement` enum (`IsolatedOnly`, `RealRequired`, `Either`, `Unverified`), `AgentDefinition.home_requirement: HomeRequirement`, `AgentDefinition.max_concurrency: Option<u32>`, `single_protocol::CapabilityFlags.non_interactive_run: bool`, `single_protocol::AgentInfo.home_requirement: HomeRequirement`, `AgentInfo.max_concurrency: Option<u32>`.

- [ ] **Step 1: Add `HomeRequirement` enum (in `single-protocol`) and two new `AgentDefinition` fields**

`single-protocol` has no dependency on `single-core` — confirmed via `crates/single-core/Cargo.toml:21` depending on `single-protocol`, and no reverse dependency in `crates/single-protocol/Cargo.toml`. So `HomeRequirement` must live in `single-protocol`, the same crate `CapabilityFlags`/`InstallMethod`/`BootstrapInstall` already live in, and `registry.rs` imports it the same way it already imports those.

In `crates/single-protocol/src/lib.rs`, add this enum right before the `CapabilityFlags` struct (find it with `grep -n "struct CapabilityFlags" crates/single-protocol/src/lib.rs`):

```rust
/// Whether an agent needs `real_home: true` to authenticate, breaks under
/// it, works either way, or nobody's confirmed it empirically yet.
/// `Unverified` is the honest default — see `single_core::ratelimit`'s
/// module doc for why this project only claims what it's directly
/// checked. See `docs/superpowers/specs/2026-08-26-orchestration-lessons-design.md`
/// for how the known values below were established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeRequirement {
    /// Breaks (fails to authenticate correctly) under `real_home: true`.
    IsolatedOnly,
    /// `real_home: true` is the only way this agent ever authenticates —
    /// either structurally (its isolated home can never hold real
    /// credentials by design) or empirically confirmed.
    RealRequired,
    /// Authenticates correctly either way.
    Either,
    /// Nobody has empirically confirmed either way yet.
    Unverified,
}
```

Then in `crates/single-core/src/registry.rs`, replace the `use` line and struct definition at the top of the file:

```rust
use serde::{Deserialize, Serialize};
use single_protocol::{BootstrapInstall, CapabilityFlags, HomeRequirement, InstallMethod};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub adapter: String,
    pub command: String,
    pub install_method: InstallMethod,
    pub bootstrap_install: Option<BootstrapInstall>,
    pub unverified: bool,
    /// See `HomeRequirement`'s doc comment.
    pub home_requirement: HomeRequirement,
    /// `Some(n)` means at most `n` instances of this agent may run
    /// concurrently (enforced in `single-runtime::task`'s spawn
    /// entrypoint) — e.g. opencode's own session SQLite database takes
    /// an exclusive lock that a second concurrent instance can't acquire.
    /// `None` means no known limit.
    pub max_concurrency: Option<u32>,
    pub capabilities: CapabilityFlags,
    /// Config file paths this adapter can read/write, relative to `$HOME`.
    pub config_paths: Vec<String>,
    pub notes: Option<String>,
}
```

- [ ] **Step 2: Add `non_interactive_run` to `CapabilityFlags`**

In `crates/single-protocol/src/lib.rs`, replace:

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub streaming: bool,
    pub mcp: bool,
    pub lsp: bool,
    pub tools: bool,
    pub sessions: bool,
    pub structured_output: bool,
}
```

with:

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub streaming: bool,
    pub mcp: bool,
    pub lsp: bool,
    pub tools: bool,
    pub sessions: bool,
    pub structured_output: bool,
    /// False only for an adapter whose `run_prompt` falls through to
    /// `single_agent_sdk::adapter`'s default-trait `bail!` (no
    /// non-interactive mode exists at all, e.g. codebuff) — lets a caller
    /// check this before dispatching instead of discovering it via a
    /// failed `task_run` call. True for every adapter that overrides
    /// `run_prompt`.
    #[serde(default = "default_true")]
    pub non_interactive_run: bool,
}

fn default_true() -> bool {
    true
}
```

(`#[serde(default = "default_true")]` matters here specifically because `CapabilityFlags` derives `Default`, which would otherwise give `false` — the wrong default for this one field — for any old serialized data or any future `..Default::default()` usage.)

- [ ] **Step 3: `cargo build` to find every literal that needs updating**

Run: `cargo build -p single-core 2>&1 | head -50`
Expected: many `missing field` errors, one per `AgentDefinition { ... }` literal in `registry.rs` and one per `CapabilityFlags { ... }` literal wherever they're constructed outside `registry.rs` (should be none — `registry.rs` is the only place `CapabilityFlags` literals are built; confirm with `grep -rn "CapabilityFlags {" crates/ --include="*.rs" | grep -v registry.rs` and fix any that turn up the same way as Step 4 below).

- [ ] **Step 4: Update all 22 `AgentDefinition` literals**

For **every** `AgentDefinition { ... }` literal in `crates/single-core/src/registry.rs`, make two edits:

1. Insert `home_requirement: <value>,` then `max_concurrency: <value>,` as new lines immediately after that literal's `unverified: <bool>,` line (before `capabilities: CapabilityFlags {`).
2. Insert `non_interactive_run: <value>,` as a new line immediately after that literal's `structured_output: <bool>,` line, still inside the `CapabilityFlags { ... }` block (before its closing `},`).

Worked example — `claude`'s full literal (line 33-57 today) becomes:

```rust
        AgentDefinition {
            name: "claude".into(),
            adapter: "claude-code".into(),
            command: "claude".into(),
            install_method: InstallMethod::Native {
                detail: "Anthropic's native installer; installs to ~/.local/share/claude, \
                         symlinked from ~/.local/bin/claude"
                    .into(),
            },
            bootstrap_install: Some(BootstrapInstall {
                command: "curl -fsSL https://claude.ai/install.sh | bash".into(),
                source: "https://code.claude.com/docs/en/setup".into(),
            }),
            unverified: false,
            home_requirement: HomeRequirement::Either,
            max_concurrency: None,
            capabilities: CapabilityFlags {
                streaming: true,
                mcp: true,
                lsp: true, // observed via ~/.claude/settings.json enabledPlugins (gopls/pyright/etc LSP plugins)
                tools: true,
                sessions: true, // resumable conversation history observed in ~/.claude/history.jsonl
                structured_output: false,
                non_interactive_run: true,
            },
            config_paths: vec![".claude.json".into(), ".claude/settings.json".into()],
            notes: None,
        },
```

Apply the same two insertions to the remaining 21 entries, using this table for the two new field values (`non_interactive_run` is `true` for every agent except `codebuff`):

| Agent (`name`) | `home_requirement` | `max_concurrency` |
|---|---|---|
| `claude` | `HomeRequirement::Either` | `None` |
| `codex` | `HomeRequirement::IsolatedOnly` | `None` |
| `opencode` | `HomeRequirement::RealRequired` | `Some(1)` |
| `agy` | `HomeRequirement::Unverified` | `None` |
| `perplexity` | `HomeRequirement::Unverified` | `None` |
| `cursor` | `HomeRequirement::IsolatedOnly` | `None` |
| `aider` | `HomeRequirement::Unverified` | `None` |
| `goose` | `HomeRequirement::Unverified` | `None` |
| `copilot` | `HomeRequirement::RealRequired` | `None` |
| `kiro` | `HomeRequirement::Unverified` | `None` |
| `cody` | `HomeRequirement::Unverified` | `None` |
| `gemini` | `HomeRequirement::Unverified` | `None` |
| `qwen-code` | `HomeRequirement::Unverified` | `None` |
| `amp` | `HomeRequirement::Unverified` | `None` |
| `openhands` | `HomeRequirement::Unverified` | `None` |
| `droid` | `HomeRequirement::Unverified` | `None` |
| `codebuff` | `HomeRequirement::Unverified` | `None` |
| `plandex` | `HomeRequirement::Unverified` | `None` |
| `continue-cli` | `HomeRequirement::Unverified` | `None` |
| `grok` | `HomeRequirement::IsolatedOnly` | `None` |
| `mistral-vibe` | `HomeRequirement::Unverified` | `None` |
| `crush` | `HomeRequirement::Unverified` | `None` |

For `copilot`'s entry specifically, add this comment right above its `home_requirement` line (it's structural, not just empirical — matches `agent_home.rs`'s `strip_embedded_credential_fields` doc comment):

```rust
            // Structural, not just empirical: agent_home.rs's
            // strip_embedded_credential_fields always strips copilot's
            // keyring pointer from its isolated home, so isolated auth is
            // impossible by design — real_home:true is the only path.
            home_requirement: HomeRequirement::RealRequired,
            max_concurrency: None,
```

For `codebuff`'s `capabilities` block, set `non_interactive_run: false,` and add a comment:

```rust
                structured_output: false,
                // run_prompt falls through to single-agent-sdk's
                // default-trait `bail!` — no non-interactive mode exists.
                non_interactive_run: false,
```

- [ ] **Step 5: `cargo build` again to confirm all 22 are fixed**

Run: `cargo build -p single-core 2>&1 | tail -20`
Expected: clean build, no errors.

- [ ] **Step 6: Add the two new fields to `AgentInfo` and copy them in `to_agent_info`**

In `crates/single-protocol/src/lib.rs`, replace the `AgentInfo` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub adapter: String,
    pub command: String,
    pub detected: bool,
    pub version: Option<String>,
    pub install_method: InstallMethod,
    pub bootstrap_install: Option<BootstrapInstall>,
    pub unverified: bool,
    /// See `HomeRequirement`'s doc comment.
    pub home_requirement: HomeRequirement,
    pub max_concurrency: Option<u32>,
    pub capabilities: CapabilityFlags,
    pub config_paths: Vec<String>,
    /// Free-text caveat surfaced in `doctor`/`agent inspect`, e.g. when an
    /// entry doesn't fit the coding-agent model cleanly (see `perplexity`).
    pub notes: Option<String>,
    /// Auto-detected presence of *some* live login for this agent, checked
    /// across both SingleCLI's isolated home and the real ambient home. See
    /// `AuthState` docs for how this differs from `AccountProfileInfo::status`.
    #[serde(default)]
    pub authenticated: AuthState,
}
```

`HomeRequirement` is already in scope in `single-protocol/src/lib.rs` from Step 1 above, so no extra import is needed for this struct.

In `crates/single-runtime/src/handlers.rs`, replace `to_agent_info`:

```rust
fn to_agent_info(def: &AgentDefinition, ctx: &Context) -> AgentInfo {
    let discovery = cached_discover(def, ctx);
    let isolated_home = ctx.dirs.homes_dir().join(&def.name);
    let authenticated = single_core::account::is_authenticated(&isolated_home, &def.name);
    AgentInfo {
        name: def.name.clone(),
        adapter: def.adapter.clone(),
        command: def.command.clone(),
        detected: discovery.as_ref().map(|d| d.detected).unwrap_or(false),
        version: discovery.and_then(|d| d.version),
        install_method: def.install_method.clone(),
        bootstrap_install: def.bootstrap_install.clone(),
        unverified: def.unverified,
        home_requirement: def.home_requirement,
        max_concurrency: def.max_concurrency,
        capabilities: def.capabilities,
        config_paths: def.config_paths.clone(),
        notes: def.notes.clone(),
        authenticated,
    }
}
```

- [ ] **Step 7: Show the new fields in `single agent inspect`**

In `crates/single-cli/src/render.rs`, in the `ResponseData::Agent(agent) => { ... }` arm, replace the `capabilities:` println block:

```rust
            println!(
                "  capabilities: mcp={} lsp={} tools={} sessions={} streaming={} structured_output={} non_interactive_run={}",
                agent.capabilities.mcp,
                agent.capabilities.lsp,
                agent.capabilities.tools,
                agent.capabilities.sessions,
                agent.capabilities.streaming,
                agent.capabilities.structured_output,
                agent.capabilities.non_interactive_run
            );
            println!(
                "  home_requirement: {:?}{}",
                agent.home_requirement,
                agent
                    .max_concurrency
                    .map(|n| format!(" (max_concurrency: {n})"))
                    .unwrap_or_default()
            );
```

- [ ] **Step 8: Write tests confirming the empirically-known values**

Add to `crates/single-core/src/registry.rs`'s existing `#[cfg(test)] mod tests` block (create one at the bottom of the file if none exists — check first with `grep -n "mod tests" crates/single-core/src/registry.rs`):

```rust
#[cfg(test)]
mod home_requirement_tests {
    use super::*;

    fn find(name: &str) -> AgentDefinition {
        builtin_registry()
            .into_iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no such agent in builtin_registry: {name}"))
    }

    #[test]
    fn copilot_and_opencode_require_real_home() {
        assert_eq!(find("copilot").home_requirement, HomeRequirement::RealRequired);
        assert_eq!(find("opencode").home_requirement, HomeRequirement::RealRequired);
    }

    #[test]
    fn cursor_grok_codex_break_under_real_home() {
        assert_eq!(find("cursor").home_requirement, HomeRequirement::IsolatedOnly);
        assert_eq!(find("grok").home_requirement, HomeRequirement::IsolatedOnly);
        assert_eq!(find("codex").home_requirement, HomeRequirement::IsolatedOnly);
    }

    #[test]
    fn claude_works_either_way() {
        assert_eq!(find("claude").home_requirement, HomeRequirement::Either);
    }

    #[test]
    fn most_agents_are_unverified_by_default() {
        assert_eq!(find("gemini").home_requirement, HomeRequirement::Unverified);
        assert_eq!(find("aider").home_requirement, HomeRequirement::Unverified);
    }

    #[test]
    fn opencode_has_a_concurrency_limit_of_one() {
        assert_eq!(find("opencode").max_concurrency, Some(1));
    }

    #[test]
    fn most_agents_have_no_concurrency_limit() {
        assert_eq!(find("claude").max_concurrency, None);
        assert_eq!(find("codex").max_concurrency, None);
    }

    #[test]
    fn codebuff_has_no_non_interactive_run_mode() {
        assert!(!find("codebuff").capabilities.non_interactive_run);
    }

    #[test]
    fn every_other_agent_has_non_interactive_run() {
        for agent in builtin_registry() {
            if agent.name != "codebuff" {
                assert!(
                    agent.capabilities.non_interactive_run,
                    "{} should have non_interactive_run: true",
                    agent.name
                );
            }
        }
    }
}
```

- [ ] **Step 9: Run the tests**

Run: `cargo test -p single-core home_requirement_tests -- --nocapture`
Expected: all 8 tests PASS.

- [ ] **Step 10: Full workspace build check and commit**

Run: `cargo build --workspace 2>&1 | tail -30` — expected clean (this catches any other `CapabilityFlags`/`AgentDefinition`/`AgentInfo` literal outside the files touched above, e.g. in `single-agent-sdk`'s test helpers if any construct these types directly; fix the same way as Step 4/6 if the build surfaces one).

```bash
git add crates/single-core/src/registry.rs crates/single-protocol/src/lib.rs crates/single-runtime/src/handlers.rs crates/single-cli/src/render.rs
git commit -m "feat: add HomeRequirement, max_concurrency, and non_interactive_run agent metadata"
```

---

### Task 2: `TaskRecord.rate_limited` field, schema migration, and plumbing

**Files:**
- Modify: `crates/single-protocol/src/lib.rs:1076-1100` (`TaskRecord`)
- Modify: `crates/single-runtime/src/task.rs:26-48` (schema), `:161-179` (`row_to_task`), `:240-257` (`create`), `:300-317` (`finish`), all 7 `finish(...)` call sites (`:286, 647, 680, 765, 818, 904, 937`)
- Modify: `crates/single-runtime/src/orchestrate_graph.rs:413-427` (`fake()` test helper)
- Test: `crates/single-runtime/src/task.rs` (new tests)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `TaskRecord.rate_limited: bool`, `finish()`'s new 9th parameter `rate_limited: bool` (positional, added at the end).

- [ ] **Step 1: Add `rate_limited` to `TaskRecord`**

In `crates/single-protocol/src/lib.rs`, add to the end of the `TaskRecord` struct (right before its closing `}`, after `workspace_id`):

```rust
    /// True if `single_core::ratelimit::looks_like_rate_limit` matched
    /// this task's captured output. Checked unconditionally on every
    /// non-zero-exit completion, regardless of `allow_fallback` — see
    /// `task::execute`.
    #[serde(default)]
    pub rate_limited: bool,
}
```

(remove the old closing `}` that followed `workspace_id`, so the field list ends with this new field.)

- [ ] **Step 2: Migrate the schema**

In `crates/single-runtime/src/task.rs`'s `ensure_schema`, add one line after the existing `add_column_if_missing` calls:

```rust
    add_column_if_missing(conn, "tasks", "cwd", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "tasks", "workspace_id", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "tasks", "rate_limited", "INTEGER NOT NULL DEFAULT 0")?;
```

- [ ] **Step 3: Read the column in `row_to_task`**

Replace the `Ok(TaskRecord { ... })` block in `row_to_task`:

```rust
fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<TaskRecord> {
    let status_str: String = row.get("status")?;
    Ok(TaskRecord {
        id: row.get("id")?,
        description: row.get("description")?,
        agent: row.get("agent")?,
        status: parse_status(&status_str).map_err(|e| {
            rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text)
        })?,
        worktree_path: row.get("worktree_path")?,
        artifact_path: row.get("artifact_path")?,
        exit_code: row.get("exit_code")?,
        timed_out: row.get::<_, i64>("timed_out")? != 0,
        summary: row.get("summary")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        cwd: row.get("cwd")?,
        workspace_id: row.get("workspace_id")?,
        rate_limited: row.get::<_, i64>("rate_limited")? != 0,
    })
```

- [ ] **Step 4: Update `finish()`'s signature and its `UPDATE` statement**

Replace the `finish` function:

```rust
#[allow(clippy::too_many_arguments)]
fn finish(
    conn: &Connection,
    id: i64,
    status: TaskStatus,
    worktree_path: Option<&str>,
    artifact_path: Option<&str>,
    exit_code: Option<i32>,
    timed_out: bool,
    summary: Option<&str>,
    rate_limited: bool,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE tasks SET status = ?1, worktree_path = ?2, artifact_path = ?3, exit_code = ?4, timed_out = ?5, summary = ?6, updated_at = ?7, rate_limited = ?8 WHERE id = ?9",
        params![status_as_str(status), worktree_path, artifact_path, exit_code, timed_out as i64, summary, now, rate_limited as i64, id],
    )?;
    Ok(())
}
```

- [ ] **Step 5: Update every `finish(...)` call site to pass `rate_limited`**

Six of the seven call sites (`:286, 647, 680, 765, 818, 937`) are non-agent-output failures (git-repo check, worktree setup, home materialization, docker startup, task-cancellation, and the `Err(e)` spawn-error branch) — add `false,` as the new last argument to each of these six.

The seventh call site (around line 904, inside the `Ok(outcome) => { ... }` arm) is the one real case: replace

```rust
            let summary = summarize(&outcome.stdout, outcome.timed_out, outcome.exit_code);
            finish(
                conn,
                id,
                status,
                worktree_path_str.as_deref(),
                Some(&artifact_path.display().to_string()),
                outcome.exit_code,
                outcome.timed_out,
                Some(&summary),
            )?;
```

with:

```rust
            let summary = summarize(&outcome.stdout, outcome.timed_out, outcome.exit_code);
            let combined_output = format!("{}\n{}", outcome.stdout, outcome.stderr);
            let rate_limited = single_core::ratelimit::looks_like_rate_limit(&combined_output);
            finish(
                conn,
                id,
                status,
                worktree_path_str.as_deref(),
                Some(&artifact_path.display().to_string()),
                outcome.exit_code,
                outcome.timed_out,
                Some(&summary),
                rate_limited,
            )?;
```

And for the `Err(e) => { ... }` branch (around line 937), which is a process-spawn-level error rather than agent output but may still carry a rate-limit-shaped message — replace:

```rust
        Err(e) => {
            finish(
                conn,
                id,
                TaskStatus::Failed,
                worktree_path_str.as_deref(),
                None,
                None,
                false,
                Some(&format!("{e:#}")),
            )?;
```

with:

```rust
        Err(e) => {
            let error_text = format!("{e:#}");
            let rate_limited = single_core::ratelimit::looks_like_rate_limit(&error_text);
            finish(
                conn,
                id,
                TaskStatus::Failed,
                worktree_path_str.as_deref(),
                None,
                None,
                false,
                Some(&error_text),
                rate_limited,
            )?;
```

(the remaining code in that `Err(e)` arm — `remember_failure`, `failure_text = Some(...)` — is unchanged; only the `finish(...)` call and the two new lines above it change.)

- [ ] **Step 6: Update `orchestrate_graph.rs`'s test helper**

In `crates/single-runtime/src/orchestrate_graph.rs`'s `fake()` function, add `rate_limited: false,` as the last field before the closing `}`:

```rust
    fn fake(status: TaskStatus) -> TaskRecord {
        TaskRecord {
            id: 0,
            description: String::new(),
            agent: String::new(),
            status,
            worktree_path: None,
            artifact_path: None,
            exit_code: None,
            timed_out: false,
            summary: None,
            created_at: String::new(),
            updated_at: String::new(),
            cwd: String::new(),
            workspace_id: String::new(),
            rate_limited: false,
        }
    }
```

- [ ] **Step 7: `cargo build` to find any other `TaskRecord { ... }` literal**

Run: `cargo build --workspace 2>&1 | grep -A3 "missing field"`
Expected: none left after Steps 3, 6 above (if any turn up — e.g. in a test file not covered here — add `rate_limited: false,` to each, same pattern as Step 6).

- [ ] **Step 8: Write a test proving the field is set on a real failing task**

Add to `crates/single-runtime/src/task.rs`'s existing `#[cfg(test)] mod tests` block (find it with `grep -n "mod tests" crates/single-runtime/src/task.rs`):

```rust
    #[test]
    fn rate_limited_flag_is_set_when_output_matches_a_known_signal() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        let id = create(&conn, "test task", "fake-agent", dir.path().to_str().unwrap(), "ws1").unwrap();
        finish(&conn, id, TaskStatus::Failed, None, None, Some(1), false, Some("HTTP 429 Too Many Requests"), true).unwrap();
        let task = get(&conn, id).unwrap().unwrap();
        assert!(task.rate_limited);
    }

    #[test]
    fn rate_limited_flag_defaults_false_for_an_ordinary_failure() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        let id = create(&conn, "test task", "fake-agent", dir.path().to_str().unwrap(), "ws1").unwrap();
        finish(&conn, id, TaskStatus::Failed, None, None, Some(1), false, Some("panic: index out of bounds"), false).unwrap();
        let task = get(&conn, id).unwrap().unwrap();
        assert!(!task.rate_limited);
    }
```

(Check the exact visibility/signature of `create`/`get` used elsewhere in this file's existing tests first — `grep -n "fn create\b\|fn get\b" crates/single-runtime/src/task.rs` — and adjust the calls above to match if they differ from what's shown, e.g. if `get` takes `&Connection` positionally in a different order.)

- [ ] **Step 9: Run the tests**

Run: `cargo test -p single-runtime rate_limited -- --nocapture`
Expected: both tests PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/single-protocol/src/lib.rs crates/single-runtime/src/task.rs crates/single-runtime/src/orchestrate_graph.rs
git commit -m "feat: add TaskRecord.rate_limited, detected unconditionally on task completion"
```

---

### Task 3: Reuse the same detection inside `maybe_fail_over` (no duplicate logic)

**Files:**
- Modify: `crates/single-runtime/src/task.rs:960-1014` (the `allow_fallback` call site and `maybe_fail_over`)

**Interfaces:**
- Consumes: the `rate_limited: bool` computed in Task 2 Step 5, at the two `finish(...)` call sites inside the `match outcome` block.

Right now `maybe_fail_over` independently recomputes `single_core::ratelimit::looks_like_rate_limit(failure_text)` internally. After Task 2, that computation already happened once (right before each relevant `finish()` call) — pass it through instead of recomputing, so there's exactly one call site for the detector per task completion.

- [ ] **Step 1: Thread `rate_limited` through to `maybe_fail_over`**

In `execute()`, the two `finish(...)` calls from Task 2 Step 5 each already have a local `rate_limited` binding. Immediately after the `match outcome { ... }` block (where `failure_text` is checked), change:

```rust
    if opts.allow_fallback {
        if let Some(text) = failure_text {
            maybe_fail_over(conn, ctx, id, opts, &text, cancel);
        }
    }
```

to:

```rust
    if opts.allow_fallback {
        if let Some(text) = failure_text {
            maybe_fail_over(conn, ctx, id, opts, &text, rate_limited_for_fallback, cancel);
        }
    }
```

and introduce `rate_limited_for_fallback` as a `let mut` alongside the existing `let mut failure_text: Option<String> = None;` declaration:

```rust
    let mut failure_text: Option<String> = None;
    let mut rate_limited_for_fallback = false;
```

Then in the `Ok(outcome) => { ... }` arm, right after computing `let rate_limited = ...` (added in Task 2 Step 5), add:

```rust
            rate_limited_for_fallback = rate_limited;
```

And in the `Err(e) => { ... }` arm, right after its own `let rate_limited = ...` line, add the same:

```rust
            rate_limited_for_fallback = rate_limited;
```

- [ ] **Step 2: Update `maybe_fail_over`'s signature to accept the precomputed flag**

Replace:

```rust
fn maybe_fail_over(conn: &Connection, ctx: &Context, id: i64, opts: &RunTaskOptions, failure_text: &str, cancel: Option<&std::sync::atomic::AtomicBool>) {
    let already_rate_limited = opts.account.is_some_and(|account_name| {
        single_core::account::list(&ctx.dirs.accounts_dir(), Some(opts.agent))
            .unwrap_or_default()
            .into_iter()
            .any(|a| a.name == account_name && a.status == single_protocol::AccountStatus::RateLimited)
    });
    if !already_rate_limited && !single_core::ratelimit::looks_like_rate_limit(failure_text) {
        return;
    }
```

with:

```rust
fn maybe_fail_over(conn: &Connection, ctx: &Context, id: i64, opts: &RunTaskOptions, failure_text: &str, rate_limited: bool, cancel: Option<&std::sync::atomic::AtomicBool>) {
    let already_rate_limited = opts.account.is_some_and(|account_name| {
        single_core::account::list(&ctx.dirs.accounts_dir(), Some(opts.agent))
            .unwrap_or_default()
            .into_iter()
            .any(|a| a.name == account_name && a.status == single_protocol::AccountStatus::RateLimited)
    });
    if !already_rate_limited && !rate_limited {
        return;
    }
```

(the rest of `maybe_fail_over`'s body — reading `failure_text` is actually unused after this change; check with `cargo build` in Step 3 whether `failure_text` is now an unused parameter. If the compiler warns/errors on an unused parameter, keep it prefixed `_failure_text: &str` rather than removing it, since it documents what triggered the fallback in the function's doc comment and future callers may want it logged — check the function body below the shown excerpt for any remaining use of `failure_text` first with `grep -n "failure_text" crates/single-runtime/src/task.rs` before deciding.)

- [ ] **Step 3: Build and check for unused-parameter warnings**

Run: `cargo build -p single-runtime 2>&1 | grep -i warning`
If `failure_text` is flagged unused inside `maybe_fail_over`, rename its parameter to `_failure_text` in the signature only (leave the call sites passing `&text` as-is).

- [ ] **Step 4: Run the existing fallback tests to confirm no regression**

Run: `cargo test -p single-runtime fallback -- --nocapture`
Expected: all existing fallback-related tests (search `grep -n "fn.*fallback" crates/single-runtime/src/task.rs` for exact names, e.g. around lines 1359, 1408 seen earlier referencing `allow_fallback: true`) still PASS unchanged — this task only changes *how* the rate-limit signal reaches `maybe_fail_over`, not *when* it fires.

- [ ] **Step 5: Commit**

```bash
git add crates/single-runtime/src/task.rs
git commit -m "refactor: compute rate-limit detection once per task, reuse for fallback gating"
```

---

### Task 4: Per-agent concurrency guard

**Files:**
- Modify: `crates/single-runtime/src/task.rs` (add a semaphore module-level, gate the `adapter.run_prompt(...)` call in `execute()`)
- Test: `crates/single-runtime/src/task.rs`

**Interfaces:**
- Consumes: `AgentDefinition.max_concurrency: Option<u32>` (Task 1), `ctx.registry: Vec<AgentDefinition>` (already exists on `Context`, used elsewhere in this file, e.g. `ctx.registry.iter()` at line ~54 of `handlers.rs`).
- Produces: `acquire_agent_slot(agent: &str, max_concurrency: Option<u32>) -> AgentSlotGuard`, a private RAII guard type that releases on drop.

- [ ] **Step 1: Add the semaphore state and RAII guard**

Add near the top of `crates/single-runtime/src/task.rs`, after the existing `use` statements:

```rust
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// Per-agent-name counting semaphore, keyed the same way
/// `handlers.rs::DISCOVERY_LOCKS` already keys its per-agent cache — one
/// `(Mutex<u32>, Condvar)` pair per agent name, created on first use.
/// Blocks the calling thread (not async — this whole module is
/// thread-based, see `run_background`'s `std::thread::spawn`) until a
/// slot is free, rather than failing the caller outright.
static AGENT_SLOTS: OnceLock<Mutex<std::collections::HashMap<String, Arc<(Mutex<u32>, Condvar)>>>> = OnceLock::new();

struct AgentSlotGuard {
    slot: Option<Arc<(Mutex<u32>, Condvar)>>,
}

impl Drop for AgentSlotGuard {
    fn drop(&mut self) {
        if let Some(slot) = &self.slot {
            let (lock, cvar) = &**slot;
            let mut count = lock.lock().unwrap();
            *count -= 1;
            cvar.notify_one();
        }
    }
}

/// Blocks until a concurrency slot for `agent` is available, per its
/// registry `max_concurrency` (`None` = unlimited, returns immediately
/// with a no-op guard). See `AgentDefinition.max_concurrency`'s doc
/// comment for why this exists (opencode's own SQLite session lock).
fn acquire_agent_slot(agent: &str, max_concurrency: Option<u32>) -> AgentSlotGuard {
    let Some(limit) = max_concurrency else {
        return AgentSlotGuard { slot: None };
    };
    let registry = AGENT_SLOTS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let slot = {
        let mut map = registry.lock().unwrap();
        map.entry(agent.to_string())
            .or_insert_with(|| Arc::new((Mutex::new(0u32), Condvar::new())))
            .clone()
    };
    let (lock, cvar) = &*slot;
    let mut count = lock.lock().unwrap();
    while *count >= limit {
        count = cvar.wait(count).unwrap();
    }
    *count += 1;
    drop(count);
    AgentSlotGuard { slot: Some(slot) }
}
```

- [ ] **Step 2: Acquire the slot before spawning the agent**

In `execute()`, immediately before the `let outcome = adapter.run_prompt(...)` call, add:

```rust
    let max_concurrency = ctx.registry.iter().find(|a| a.name == opts.agent).and_then(|a| a.max_concurrency);
    let _slot_guard = acquire_agent_slot(opts.agent, max_concurrency);
    let outcome = adapter.run_prompt(
        &run_cwd,
        &prompt,
        &backend,
        Some(&live_output_path),
        opts.timeout,
        cancel,
    );
```

`_slot_guard` stays alive until the end of `execute()`'s scope (dropped when the function returns, after `finish(...)` and `notify_task_hooks(...)` have already run) — this deliberately holds the slot slightly longer than the subprocess itself runs, which is fine and slightly safer than releasing the instant the subprocess exits (avoids a narrow window where a second waiting task could start before this task's own artifact/DB writes finish).

- [ ] **Step 3: Write a test proving two same-agent runs serialize**

Add to `crates/single-runtime/src/task.rs`'s test module:

```rust
    #[test]
    fn acquire_agent_slot_serializes_when_limit_is_one() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc as StdArc;

        let concurrent_count = StdArc::new(AtomicU32::new(0));
        let max_seen = StdArc::new(AtomicU32::new(0));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let concurrent_count = concurrent_count.clone();
                let max_seen = max_seen.clone();
                std::thread::spawn(move || {
                    let _guard = acquire_agent_slot("test-agent-serialize", Some(1));
                    let now = concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    concurrent_count.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1, "at most 1 concurrent run should ever have been observed");
    }

    #[test]
    fn acquire_agent_slot_is_a_noop_when_unlimited() {
        let _guard = acquire_agent_slot("test-agent-unlimited", None);
        // No assertion needed beyond "doesn't block/panic" — unlimited
        // means immediate return every time, proven by this test completing.
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p single-runtime acquire_agent_slot -- --nocapture`
Expected: both PASS. The first test is timing-sensitive but robust: with a 20ms hold and a hard `Some(1)` limit, `max_seen` can only ever be observed as `1` if the semaphore genuinely serializes — there's no flaky window, since the assertion isn't about timing, it's about whether two threads were ever inside the guarded section simultaneously (which the atomic counter update flanking `_guard`'s lifetime measures directly, not by wall-clock).

- [ ] **Step 5: Commit**

```bash
git add crates/single-runtime/src/task.rs
git commit -m "feat: add per-agent concurrency guard, serialize opencode to max_concurrency 1"
```

---

### Task 5: Worktree diff/merge core functions

**Files:**
- Modify: `crates/single-core/src/worktree.rs`
- Test: `crates/single-core/src/worktree.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `single_core::worktree::diff(repo_root: &Path, branch: &str) -> Result<String>`, `single_core::worktree::merge(repo_root: &Path, branch: &str) -> Result<String>` (returns `git merge`'s combined stdout+stderr on success).

- [ ] **Step 1: Add `diff` and `merge` functions**

Add to `crates/single-core/src/worktree.rs`, after the existing `list` function and before `is_git_repo`:

```rust
/// Diffs `branch` against the repo's current `HEAD` (`git diff
/// HEAD...branch`) — the same three-dot range a GitHub PR view shows,
/// i.e. what actually landed on `branch` since it forked off, not
/// polluted by anything that's happened on the base branch since. Never
/// merges anything — see `merge` below, kept as a deliberately separate
/// call so a caller looks before deciding.
pub fn diff(repo_root: &Path, branch: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["diff", &format!("HEAD...{branch}")])
        .output()
        .context("spawning git diff")?;
    if !output.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Merges `branch` into the repo's current `HEAD` (`git merge --no-ff
/// branch`) — `--no-ff` always creates a merge commit, so the fact that
/// this went through an isolated worktree stays visible in history
/// rather than silently fast-forwarding. Per `docs/architecture.md`:
/// "branches are never auto-merged; that stays a human decision" — this
/// function performs the merge once a caller has explicitly decided to
/// (see `diff` above, meant to be called first), it does not decide FOR
/// the caller.
pub fn merge(repo_root: &Path, branch: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["merge", "--no-ff", branch, "-m", &format!("Merge branch '{branch}'")])
        .output()
        .context("spawning git merge")?;
    if !output.status.success() {
        bail!("git merge failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}
```

- [ ] **Step 2: Write the failing tests**

Add to `crates/single-core/src/worktree.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn diff_shows_changes_made_on_the_worktree_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-diff");

        add(repo.path(), &worktree_path, "single/task-diff").unwrap();
        std::fs::write(worktree_path.join("new-file.txt"), "hello from the worktree").unwrap();
        let status = Command::new("git").current_dir(&worktree_path).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git").current_dir(&worktree_path).args(["commit", "-q", "-m", "add new-file"]).status().unwrap();
        assert!(status.success());

        let diff_output = diff(repo.path(), "single/task-diff").unwrap();
        assert!(diff_output.contains("new-file.txt"));
        assert!(diff_output.contains("hello from the worktree"));
    }

    #[test]
    fn merge_brings_worktree_changes_into_the_main_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree_path = worktree_parent.path().join("task-merge");

        add(repo.path(), &worktree_path, "single/task-merge").unwrap();
        std::fs::write(worktree_path.join("merged-file.txt"), "merge me").unwrap();
        let status = Command::new("git").current_dir(&worktree_path).args(["add", "."]).status().unwrap();
        assert!(status.success());
        let status = Command::new("git").current_dir(&worktree_path).args(["commit", "-q", "-m", "add merged-file"]).status().unwrap();
        assert!(status.success());

        merge(repo.path(), "single/task-merge").unwrap();
        assert!(repo.path().join("merged-file.txt").is_file());
    }

    #[test]
    fn diff_fails_for_a_nonexistent_branch() {
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        assert!(diff(repo.path(), "no-such-branch").is_err());
    }
```

- [ ] **Step 3: Run the tests to verify they fail first (functions not yet compiled without Step 1)**

If Step 1 was already applied before this step, skip the "verify it fails" sub-step (the skill's normal TDD red/green ordering is inverted here since Step 1 and Step 2 are presented together for readability) — go straight to Step 4.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p single-core worktree:: -- --nocapture`
Expected: all PASS, including the 3 pre-existing tests (`add_creates_a_real_worktree_on_a_new_branch`, `remove_deletes_the_worktree`, `add_fails_outside_a_git_repo`) and the 3 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/single-core/src/worktree.rs
git commit -m "feat: add worktree diff/merge — assists, never automates, the merge-back decision"
```

---

### Task 6: `Request`/`ResponseData` wiring + `single worktree diff/merge` CLI commands

**Files:**
- Modify: `crates/single-protocol/src/lib.rs` (new `Request`/`ResponseData` variants, new `WorktreeMergeResult` struct)
- Modify: `crates/single-runtime/src/handlers.rs` (dispatch arms)
- Modify: `crates/single-cli/src/main.rs` (new `worktree` subcommand)
- Modify: `crates/single-cli/src/render.rs` (render the two new `ResponseData` variants)
- Test: `crates/single-runtime/src/handlers.rs`

**Interfaces:**
- Consumes: `single_core::worktree::diff`/`merge` (Task 5), `task::get` (existing, used elsewhere in `task.rs`).
- Produces: `Request::WorktreeMergePreview { task_id: i64 }`, `Request::WorktreeMergeApply { task_id: i64 }`, `ResponseData::WorktreeDiff(String)`, `ResponseData::WorktreeMerged(WorktreeMergeResult)`, `pub struct WorktreeMergeResult { pub task_id: i64, pub branch: String, pub output: String }`.

- [ ] **Step 1: Add the two `Request` variants**

In `crates/single-protocol/src/lib.rs`, add near the other `Task*` request variants (find with `grep -n "TaskCleanup\|TaskCancel" crates/single-protocol/src/lib.rs` and add right after whichever of those appears last):

```rust
    /// Shows the diff a worktree-isolated task's branch would bring in if
    /// merged — never merges. See `single_core::worktree::diff`.
    WorktreeMergePreview {
        task_id: i64,
    },
    /// Merges a worktree-isolated task's branch into the repo it ran
    /// against. Deliberately a separate request from
    /// `WorktreeMergePreview` — a caller must have looked at the diff
    /// first (`docs/architecture.md`: "branches are never auto-merged;
    /// that stays a human decision").
    WorktreeMergeApply {
        task_id: i64,
    },
```

- [ ] **Step 2: Add the two `ResponseData` variants and `WorktreeMergeResult`**

In `crates/single-protocol/src/lib.rs`, add `WorktreeDiff(String)` and `WorktreeMerged(WorktreeMergeResult)` to the `ResponseData` enum (right before its final `Empty,` line):

```rust
    WorktreeDiff(String),
    WorktreeMerged(WorktreeMergeResult),
    Empty,
}
```

And define the struct near `ProviderSyncResult` (find with `grep -n "struct ProviderSyncResult" crates/single-protocol/src/lib.rs`, add right after it):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeMergeResult {
    pub task_id: i64,
    pub branch: String,
    pub output: String,
}
```

- [ ] **Step 3: Add the dispatch arms**

In `crates/single-runtime/src/handlers.rs`'s `dispatch` function, add two new arms (near the other `Request::Task*` arms):

```rust
        Request::WorktreeMergePreview { task_id } => {
            let conn = crate::state::open(&ctx.dirs.db_path())?;
            crate::task::ensure_schema(&conn)?;
            let task = crate::task::get(&conn, task_id)?.ok_or_else(|| anyhow::anyhow!("no such task: #{task_id}"))?;
            let repo_root = single_core::project_context::resolve(std::path::Path::new(&task.cwd))
                .repo_root
                .ok_or_else(|| anyhow::anyhow!("task #{task_id}'s original cwd '{}' is not inside a git repository", task.cwd))?;
            let branch = format!("single/task-{task_id}");
            let diff_output = single_core::worktree::diff(std::path::Path::new(&repo_root), &branch)?;
            Ok(ResponseData::WorktreeDiff(diff_output))
        }
        Request::WorktreeMergeApply { task_id } => {
            let conn = crate::state::open(&ctx.dirs.db_path())?;
            crate::task::ensure_schema(&conn)?;
            let task = crate::task::get(&conn, task_id)?.ok_or_else(|| anyhow::anyhow!("no such task: #{task_id}"))?;
            let repo_root = single_core::project_context::resolve(std::path::Path::new(&task.cwd))
                .repo_root
                .ok_or_else(|| anyhow::anyhow!("task #{task_id}'s original cwd '{}' is not inside a git repository", task.cwd))?;
            let branch = format!("single/task-{task_id}");
            let output = single_core::worktree::merge(std::path::Path::new(&repo_root), &branch)?;
            Ok(ResponseData::WorktreeMerged(single_protocol::WorktreeMergeResult { task_id, branch, output }))
        }
```

Check the exact visibility of `crate::task::get`/`crate::task::ensure_schema` first (`grep -n "^pub fn get\b\|^pub fn ensure_schema" crates/single-runtime/src/task.rs`) — both should already be `pub(crate)` or `pub` since `handlers.rs` calls into `task.rs` elsewhere in this same file (e.g. its existing `Request::TaskGet`-style arms, find one with `grep -n "task::get" crates/single-runtime/src/handlers.rs` to copy its exact calling convention if it differs from what's shown above).

- [ ] **Step 4: Add the CLI subcommand**

In `crates/single-cli/src/main.rs`, find the `enum` that already defines `worktree`-adjacent commands or add a new top-level command. Search first: `grep -n "enum.*Command\b" crates/single-cli/src/main.rs | head -20` to find the right enum to extend (or confirm no `WorktreeCommand` exists yet, in which case add a new top-level `Worktree(WorktreeCommand)` variant to the main CLI enum the same way `Provider(ProviderCommand)` is wired — find that wiring with `grep -n "Provider(ProviderCommand)\|Commands::Provider" crates/single-cli/src/main.rs` and mirror its exact pattern).

Add:

```rust
#[derive(Subcommand)]
enum WorktreeCommand {
    /// Show the diff a task's worktree branch would bring in if merged — never merges.
    Diff {
        task_id: i64,
    },
    /// Merge a task's worktree branch into the repo it ran against.
    Merge {
        task_id: i64,
    },
}
```

and its handler (mirroring `ProviderCommand`'s match-arm style shown earlier in this codebase):

```rust
            WorktreeCommand::Diff { task_id } => {
                let response = client::send(&socket_path, Request::WorktreeMergePreview { task_id })?;
                render::print(response, false);
            }
            WorktreeCommand::Merge { task_id } => {
                let response = client::send(&socket_path, Request::WorktreeMergeApply { task_id })?;
                render::print(response, false);
            }
```

- [ ] **Step 5: Render the two new response types**

In `crates/single-cli/src/render.rs`'s `print_data` function, add two new match arms (near `ResponseData::Provider`/`ResponseData::ProviderSyncResults`):

```rust
        ResponseData::WorktreeDiff(diff) => {
            if diff.is_empty() {
                println!("(no differences — branch has nothing new to merge)");
            } else {
                println!("{diff}");
            }
        }
        ResponseData::WorktreeMerged(result) => {
            println!("Merged {} into the current branch.", result.branch);
            if !result.output.trim().is_empty() {
                println!("{}", result.output.trim());
            }
        }
```

- [ ] **Step 6: Build and manually verify end-to-end**

Run: `cargo build --workspace 2>&1 | tail -30` — expected clean.

Run a real end-to-end check against a scratch repo:
```bash
mkdir -p /tmp/worktree-merge-manual-test && cd /tmp/worktree-merge-manual-test
git init -q && git config user.email t@t.com && git config user.name T && echo hi > README.md && git add . && git commit -q -m initial
cargo run -p single-cli -- task run "echo done > proof.txt && git add proof.txt && git commit -m proof" --agent claude --use-worktree --cwd /tmp/worktree-merge-manual-test
# note the printed task id, then:
cargo run -p single-cli -- worktree diff <task-id>
cargo run -p single-cli -- worktree merge <task-id>
ls /tmp/worktree-merge-manual-test/proof.txt  # should now exist on the main branch
```
(Skip this manual check if `claude` isn't authenticated/available in this environment — the automated tests in Task 5 already prove `diff`/`merge` work; this step is extra end-to-end confidence, not required for the task to be considered done, since it depends on an external agent CLI actually running successfully.)

- [ ] **Step 7: Commit**

```bash
git add crates/single-protocol/src/lib.rs crates/single-runtime/src/handlers.rs crates/single-cli/src/main.rs crates/single-cli/src/render.rs
git commit -m "feat: add single worktree diff/merge commands"
```

---

### Task 7: `singlecli-mcp` tools for worktree merge-back

**Files:**
- Modify: `crates/singlecli-mcp/src/server.rs`

**Interfaces:**
- Consumes: `Request::WorktreeMergePreview`/`WorktreeMergeApply` (Task 6).
- Produces: two new MCP tools, `worktree_merge_preview` and `worktree_merge_apply`.

- [ ] **Step 1: Add the two handler methods**

In `crates/singlecli-mcp/src/server.rs`, add near `agent_inspect`:

```rust
    fn worktree_merge_preview(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        let task_id = args
            .get("task_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("missing required integer argument \"task_id\""))?;
        self.send(Request::WorktreeMergePreview { task_id })
    }

    fn worktree_merge_apply(&self, args: &Map<String, Value>) -> anyhow::Result<Value> {
        if let Some(response) = Self::permission_gate("worktree_merge_apply")? {
            return Ok(response);
        }
        let task_id = args
            .get("task_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("missing required integer argument \"task_id\""))?;
        self.send(Request::WorktreeMergeApply { task_id })
    }
```

(`worktree_merge_preview` is read-only — no permission gate, matching `agent_list`/`agent_inspect`'s ungated pattern above it. `worktree_merge_apply` mutates the repo — gated, matching the precedent set by the recent "gate singlecli-mcp's run-triggering tools through the permission system" commit; check `task_run`'s gate call above for the exact copy-pasteable pattern, already shown at line 212 of this file.)

- [ ] **Step 2: Register both tools in `list_tools`**

Add two `Tool::new(...)` entries to the `vec![...]` in `list_tools`, right after the `agent_inspect` entry:

```rust
            Tool::new(
                "worktree_merge_preview",
                "Shows the diff a worktree-isolated task's branch would bring in if merged into the repo it ran against — never merges. Call this before worktree_merge_apply.",
                schema(json!({
                    "type": "object",
                    "properties": { "task_id": { "type": "integer", "description": "The task id returned by task_run/orchestrate_parallel_run/orchestrate_graph_run for a run started with use_worktree: true." } },
                    "required": ["task_id"],
                    "additionalProperties": false
                })),
            ),
            Tool::new(
                "worktree_merge_apply",
                "Merges a worktree-isolated task's branch into the repo it ran against. Look at worktree_merge_preview's diff first — branches are never auto-merged.",
                schema(json!({
                    "type": "object",
                    "properties": { "task_id": { "type": "integer" } },
                    "required": ["task_id"],
                    "additionalProperties": false
                })),
            ),
```

- [ ] **Step 3: Add the dispatch arms in `call_tool`**

In `call_tool`'s match, add:

```rust
            "worktree_merge_preview" => self.worktree_merge_preview(arguments),
            "worktree_merge_apply" => self.worktree_merge_apply(arguments),
```

right after the `"agent_inspect" => self.agent_inspect(arguments),` line.

- [ ] **Step 4: Point `orchestrate_parallel_run`/`orchestrate_graph_run`'s tool descriptions at the new tools**

Update their `Tool::new(...)` description strings (currently ending `"...you supply each agent's task."` and `"...with real cycle validation."` respectively) to each append: `" Each task's worktree isn't merged back automatically — call worktree_merge_preview then worktree_merge_apply once you're satisfied with a task's result."`

- [ ] **Step 5: Write a permission-gating test matching the existing style**

The existing `task_run_denied_by_permission_never_reaches_self_send` test (around line 575) already establishes the exact pattern: set `SINGLE_CONFIG_DIR` to a temp dir, save a `deny_rule` for the resource, construct the server, call the gated method with an empty (or otherwise-invalid) args map, and assert the JSON comes back `denied: true` rather than an argument-parsing error — proving the gate ran before `self.send`. Add this test using the same `ENV_LOCK`/`deny_rule` helpers already defined above it in the same `mod tests` block:

```rust
    #[test]
    fn worktree_merge_apply_denied_by_permission_never_reaches_self_send() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SINGLE_CONFIG_DIR", dir.path());
        let dirs = single_core::SingleDirs::discover().unwrap();
        single_core::permissions::save(&dirs.permissions_file(), &deny_rule("singlecli:worktree_merge_apply")).unwrap();

        let server = SingleCliServer::new().unwrap();
        // Deliberately empty args: worktree_merge_apply would normally
        // fail immediately on the missing "task_id" via the
        // as_i64()/ok_or_else check. Getting the denial JSON back instead
        // proves the permission gate ran first, before any argument
        // parsing or a call to self.send.
        let result = server.worktree_merge_apply(&Map::new()).unwrap();
        assert_eq!(result["denied"], true);
        assert_eq!(result["tool"], "worktree_merge_apply");

        std::env::remove_var("SINGLE_CONFIG_DIR");
    }
```

- [ ] **Step 6: Run the full singlecli-mcp test suite**

Run: `cargo test -p singlecli-mcp -- --nocapture`
Expected: all PASS, including the new test and the existing `task_run_*` permission tests unchanged.

- [ ] **Step 7: Commit**

```bash
git add crates/singlecli-mcp/src/server.rs
git commit -m "feat: add worktree_merge_preview/apply MCP tools, gate apply through permissions"
```

---

### Task 8: Version bump and final full-workspace verification

**Files:**
- Modify: `Cargo.toml` (workspace version)
- Modify: `CHANGELOG.md`

**Interfaces:**
- None — this task only verifies and documents.

- [ ] **Step 1: Bump the workspace version**

In `Cargo.toml`'s `[workspace.package]`, change `version = "0.6.0"` to `version = "0.7.0"`.

- [ ] **Step 2: Add a CHANGELOG entry**

Read the existing `CHANGELOG.md`'s most recent entry first (`head -40 CHANGELOG.md`) to match its exact heading/format, then add a new `## [0.7.0]` section above it summarizing: rate-limit detection now runs unconditionally and is surfaced on every `TaskRecord`; `HomeRequirement` and `max_concurrency` are now queryable per agent via `agent_list`/`agent inspect`; `non_interactive_run` flags agents with no non-interactive mode (codebuff); new `single worktree diff`/`merge` commands and `worktree_merge_preview`/`worktree_merge_apply` MCP tools for assisted (never automatic) merge-back.

- [ ] **Step 3: Full workspace build, test, and lint**

Run in sequence, fixing anything that surfaces before moving to the next:
```bash
cargo build --workspace 2>&1 | tail -30
cargo test --workspace 2>&1 | tail -60
cargo clippy --workspace 2>&1 | tail -60
```
Expected: clean build, all tests pass (including every test added in Tasks 1-7), no new clippy warnings beyond what already existed before this plan (check `git stash` + `cargo clippy --workspace` on the pre-change tree first if unsure whether a warning is pre-existing).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump workspace version to 0.7.0 for orchestration-lessons fixes"
```

- [ ] **Step 5: Push**

```bash
git push origin main
```
