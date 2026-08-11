# SingleCLI architecture — Phase 1

This describes what's actually built, not the full long-term vision (see
the project's original request for that). Phase 1 implements the
foundation layer: config, registry, adapters, a runtime daemon, a CLI, and
a minimal TUI dashboard. Orchestration, memory, LSP management, skills,
plugins, secrets, and sandboxing are explicitly out of scope here — see
"Not in Phase 1" below.

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

## Not in Phase 1

Per the original spec's own §50 "Development Strategy" (build vertically,
don't implement everything at once): agent orchestration/task graphs,
shared memory subsystem, LSP management, skills, plugins, secrets vault,
sandboxing/permission engine, workflows, provider abstraction, and process
lifecycle management (start/stop/stream a running agent session) are all
future-phase work. Where the full spec's shape is visible in this
codebase (e.g. `AgentAdapter`'s doc comment, `Envelope<T>` in
`single-protocol`), it's a deliberate seam for that later work, not a
stub pretending to be a finished feature.
