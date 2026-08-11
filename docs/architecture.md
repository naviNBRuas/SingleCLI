# SingleCLI architecture — Phase 1 + Phase 2 + Phase 3

This describes what's actually built, not the full long-term vision (see
the project's original request for that). Phase 1 implemented the
foundation layer: config, registry, adapters, a runtime daemon, a CLI, and
a minimal TUI dashboard. Phase 2 added the shared-capability registries:
MCP CRUD, an LSP registry, a tool metadata registry, an OS-keychain secrets
abstraction, and a local skills directory. Phase 3 added a SQLite-backed
memory subsystem and a git/project context resolver. Orchestration, a
provider abstraction, plugins, and permission *enforcement* are still out
of scope — see "Not in Phase 1, 2, or 3" below.

```text
single (CLI, no subcommand)          single <command>
        │                                    │
        ▼                                    │
  ensure daemon running                      │
        │                                    │
        ▼                                    ▼
   single-tui  ────────────────►  Unix socket (runtime.sock)
                                             │
                                    single-runtimed (daemon)
                                             │
                    ┌────────────────────────┼────────────────────────┐
                    ▼                        ▼                        ▼
              single-core              single-agent-sdk          state.rs
        (config, profiles,           (discover, configure_mcp,     (SQLite
         agent registry,              per-agent format writers)    event log)
         MCP registry)                        │
                                    ┌──────────┼──────────┬─────────────┐
                                    ▼          ▼          ▼             ▼
                                 claude      codex     opencode    agy / pplx
                              (~/.claude.json) (config.toml) (opencode.jsonc) (no verified
                                                                              config location)
```

## Crates

- **`single-protocol`** — wire types shared by every other crate:
  `Request`/`Response`, `AgentInfo`, `McpServerSpec`, `CapabilityFlags`,
  `InstallMethod`/`BootstrapInstall`. No I/O, no logic — just the shapes
  both sides of the IPC boundary agree on.
- **`single-core`** — configuration precedence (global → profile →
  project, see `config.rs`), the built-in agent registry (`registry.rs`),
  the unified MCP registry (`mcp.rs`), profile switching (`profile.rs`),
  and the canonical `~/.config/single/` directory layout (`paths.rs`).
  Pure data/logic, no process spawning.
- **`single-agent-sdk`** — real detection (`discover.rs`, shells out to
  `which`/`<cmd> --version`) and per-agent MCP config writers
  (`formats/{claude,codex,opencode}.rs`), each format independently
  verified against a real config file from the reference machine. Backups
  before every write (`backup.rs`). `AgentAdapter` (`adapter.rs`) is the
  seam Phase 4 extends with process lifecycle (start/stop/stream) without
  reshaping what's here.
- **`single-runtime`** — the headless daemon: `context.rs` loads config +
  registry per request, `handlers.rs` dispatches `Request` → `Response`,
  `doctor.rs`/`bootstrap.rs`/`integrations.rs` implement the corresponding
  CLI commands, `server.rs` is the actual Unix-socket accept loop
  (`bin/single-runtimed.rs` is the binary entry point), `state.rs` is the
  SQLite event log.
- **`single-cli`** (binary name `single`) — argument parsing (`clap`),
  talks to the runtime via `client.rs` (socket first, in-process fallback
  if no daemon is running — see ADR 0001), spawns the daemon for the TUI
  path (`daemon.rs`), and renders responses as text or `--json`
  (`render.rs`).
- **`single-tui`** — the dashboard (`dashboard.rs`, `ratatui`/`crossterm`),
  which insists on real socket IPC (`client.rs`) rather than calling into
  the runtime in-process, so it's a genuine second client of the same
  daemon the CLI talks to.

## Investigation notes (why the config formats above are what they are)

Before writing any adapter code, the real config files already present on
the reference machine were read directly, not assumed:

- `~/.claude.json` — top-level JSON object; `mcpServers` is a map of
  `{ type: "stdio", command, args, env }`. `~/.claude/settings.json`
  separately holds model/plugin/theme settings.
- `~/.codex/config.toml` — `[mcp_servers.<name>]` TOML tables with
  `command`, `args`, and an optional `[mcp_servers.<name>.env]` sub-table.
- `~/.config/opencode/opencode.jsonc` — JSON-with-comments; `mcp` is a map
  of `{ type: "local", command: [...] (array, not a string), environment,
  enabled }`; `lsp` is a sibling top-level key with the same shape idea.
- `agy` (Antigravity) has no on-disk config directory that could be found
  on this machine — its adapter shells out to `agy` subcommands
  (`agy agents`, `agy install`) for anything it needs, rather than assuming
  a config file location that was never actually observed.

See `docs/install-methods.md` for the install-command side of this same
investigation.

## Known Phase 1 simplifications (adapter emulation, not native capability)

- **OpenCode's JSONC writer strips comments on write.** `formats/opencode.rs`
  parses JSONC by stripping `//`/`/* */` comments (outside string literals)
  before calling `serde_json`, then re-serializes as plain JSON. Any
  comments a user had in `opencode.jsonc` are lost on the first
  `single install-integrations` run. A backup is always taken first
  (`backup_before_write`), so nothing is unrecoverable, but this is
  explicitly a lossy emulation, not a native capability — a
  format-preserving JSONC editor is the fix if this becomes a problem in
  practice.
- **Codex's TOML writer reformats the whole file.** Same tradeoff as
  above: `toml::Table` round-trips values but not comments/formatting.
- **No persistent multi-connection daemon lifecycle commands.** There's no
  `single daemon start/stop/status` yet. `single-runtimed` is started
  on-demand by the TUI (`daemon.rs::ensure_running`) and otherwise callers
  either connect to an already-running one or fall back to an in-process
  call. This is a deliberate Phase 1 scope cut, not an oversight — see ADR
  0001's "Consequences" section.
- **`agy` and `perplexity` (`pplx`) have no MCP integration.** Neither has
  a verified on-disk MCP config location (`agy`) or an MCP-capable surface
  at all (`pplx` is a Search API client, not a coding agent — see
  `docs/install-methods.md`). Their `configure_mcp`/`remove_mcp` are
  documented no-ops, not silently-skipped failures.

## Phase 2 additions

- **`single-core::mcp`** gained CRUD (`add`/`remove`/`set_enabled`/`find`)
  on top of Phase 1's `load`/`save`, exposed as `single mcp
  add/remove/enable/disable/inspect`.
- **`single-core::lsp`** — a registry mirroring `mcp.rs`'s shape
  (`~/.config/single/lsp.toml`), exposed as `single lsp
  list/add/remove/inspect`. There is **no agent sync yet** for LSP (unlike
  MCP's `configure_mcp`) — Claude Code's LSP support is a marketplace
  *plugin* mechanism (`enabledPlugins` in `~/.claude/settings.json`), a
  fundamentally different shape from "register an arbitrary command," and
  OpenCode's native `lsp` key was left unwired to avoid promising a
  capability without deciding how to reconcile the two mechanisms. The
  registry itself is real and tested; syncing it into agents is future work.
- **`single-core::tools`** — a metadata catalog
  (`~/.config/single/tools.toml`: name, description, risk level, enabled),
  exposed as `single tool list/add/inspect/enable/disable`. Deliberately
  metadata-only: there is no execution engine yet to actually invoke a tool
  on an agent's behalf (that's the Phase 4 orchestrator), so this doesn't
  pretend tools are wired into any agent today.
- **`single-core::secrets`** — an OS-keychain-backed secret store
  (`SecretStore` trait, `SecretTool` impl using `secret-tool`/libsecret on
  Linux — the same mechanism this machine's own existing MCP configs
  already rely on), exposed as `single secret list/set/get/delete`. Values
  never pass through `single-runtime`'s SQLite event log. macOS Keychain /
  Windows Credential Manager backends are unimplemented; `SecretStore` is
  the seam for them.
- **`single-core::permissions`** — a `deny`/`ask`/`allow` rule model with
  longest-prefix-match evaluation (`~/.config/single/permissions.toml`).
  **Not exposed via CLI or IPC** — nothing in SingleCLI executes a tool or
  agent action on the user's behalf yet, so a `single permission allow ...`
  command would have no enforcement behind it. This stays a library-only
  seam (with its own unit tests) until the Phase 4 orchestrator is a real
  caller; shipping the CLI surface first would be exactly the "fake
  integration" spec section 52 rules out.
- **`single-core::skills`** — local directory-based skills under
  `~/.config/single/skills/<name>/`, exposed as `single skill
  list/install/remove/inspect`. `install` copies a local source directory
  in; there's no network/marketplace fetch (that needs the Phase 6 plugin
  system) and SingleCLI doesn't interpret a skill's contents yet
  (translating it into each agent's native skill mechanism is future work).

## Phase 3 additions

- **`single-runtime::memory`** — SQLite-backed structured memory
  (`~/.config/single/state/single.db`, `memories` table), scoped
  (working/project/user/agent/task/long_term/knowledge) and provenance-
  tagged (`MemorySource`: user_instruction/agent_output/tool_output/
  project_content/external_content — spec sections 46-47's trust
  classification). Exposed as `single memory store/search/get/delete/list`.
  `search` is SQLite `LIKE` substring matching, **not semantic/embedding
  search** — that needs a configurable embedding provider, which doesn't
  exist until Phase 6's provider abstraction. Nothing auto-promotes agent
  output into this store; every write is an explicit, caller-tagged call,
  since there's no orchestrator yet that could do such promotion
  responsibly (or irresponsibly).
- **`single-core::project_context`** — resolves git state (repo root,
  branch, changed files via real `git` subprocess calls) and finds project
  documentation files (README/CLAUDE.md/AGENTS.md/CONTRIBUTING.md) for a
  given directory. Exposed as `single context [cwd]`. This is an *ambient
  snapshot*, not the full spec section 10 picture: "relevant source
  files," "relevant memory," and "previous agent actions" require
  relevance-ranking against an actual task, which needs the Phase 4
  orchestrator to have a task to resolve context for — that selection
  logic doesn't exist yet.

## Not in Phase 1, 2, or 3

Per the original spec's own §50 "Development Strategy" (build vertically,
don't implement everything at once): agent orchestration/task graphs, an
LSP-to-agent sync layer, a plugin/marketplace system, permission
*enforcement* (the model exists, nothing calls it), semantic/embedding
memory search, task-scoped context selection, workflows, a provider
abstraction, and process lifecycle management (start/stop/stream a running
agent session) are all future-phase work. Where the full spec's shape is
visible in this codebase (e.g. `AgentAdapter`'s doc comment, `Envelope<T>`
in `single-protocol`, `permissions.rs`), it's a deliberate seam for that
later work, not a stub pretending to be a finished feature.
