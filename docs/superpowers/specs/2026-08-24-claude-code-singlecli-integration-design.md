# Claude Code ⟷ SingleCLI Integration — Design

**Date**: 2026-08-24
**Status**: approved, not yet implemented
**Author**: Navin B. Ruas (via Claude Code)

## Problem

Navin's local Claude Code install (`~/.claude`) currently has its MCP and LSP
surface configured by hand and piecemeal: two plugin-bundled MCP servers
(`github`, `playwright`), four `claude.ai`-native OAuth connectors (Notion,
Gmail, Google Drive, Google Calendar), and four single-language LSP plugins
(rust-analyzer, typescript, pyright, gopls). Meanwhile SingleCLI (`single`)
already runs a unified registry of 100+ MCP server presets and 150+ LSP
server presets on this machine, plus agent orchestration (`task`,
`orchestrate`, `orchestrate-parallel`) across 19+ detected agent CLIs.

Goal: make SingleCLI the single place Navin configures MCP servers, LSP
servers, and provider credentials, with Claude Code automatically gaining
access — instead of maintaining Claude Code's config by hand — and give
Claude Code a real way to delegate work to other agents/models through
SingleCLI to save Claude tokens.

## Ground truth (from direct codebase inspection, 2026-08-24)

These facts drove every decision below; they are not assumptions.

- **`single mcp gateway` is real and working.** Crate `crates/single-mcp`,
  binary `single-mcp`, stdio transport (`rmcp` crate), 4 tools
  (`list_available_mcp_tools`, `invoke_mcp`, `notes_leave`, `notes_read`).
  Lazily spawns registry servers as child processes, evicts idle ones after
  10 minutes, permission-checks every `invoke_mcp` call. This is the "one
  entry, 100+ servers, dynamic-loaded" mechanism Navin wants for
  `single-mcp`, and it already exists.
- **No self-exposing MCP server exists.** Nothing in the codebase exposes
  `task run`, `orchestrate`, `orchestrate-parallel`, `memory`, `note`, or
  `provider` as MCP tools an external agent can call. `notes_leave`/
  `notes_read` (part of the gateway above) are the only piece of SingleCLI's
  *own* functionality reachable via MCP today, and they're a narrow
  async inbox, not a general command surface. `singlecli-mcp` is net-new
  work, not configuration.
- **No LSP equivalent of the gateway exists**, and Claude Code's LSP
  integration is a marketplace-*plugin* mechanism (an `enabledPlugins`
  entry with a bundled server), not "point at an arbitrary command" the
  way MCP is. `ClaudeAdapter::configure_lsp` is deliberately unimplemented
  for exactly this reason. A single dynamic `single-lsp` entry for Claude
  Code therefore cannot be wired the way `single-mcp` was — it requires a
  new Claude Code *plugin* whose bundled binary is itself a dynamic,
  file-type-routing LSP proxy.
- **SingleCLI's sync commands never touch the ambient Claude Code config.**
  `install-integrations`, `plugin sync`, and `provider sync` all resolve an
  *isolated* home at `~/.config/single/homes/claude/` (bootstrapped once
  from the real home, then never touched again) and write there —
  confirmed by a repo test asserting the real `~/.claude.json` is
  byte-for-byte unchanged after a real `install-integrations --yes` run.
  A `--real-home` opt-in exists today only for `task run`/`orchestrate`.
  Without extending that flag to the sync commands, nothing SingleCLI
  configures would ever reach the Claude Code Navin actually runs.
- **`provider sync` cannot change which model Claude Code talks to.** For
  `claude`, it only writes `env.<VAR> = <value>` into
  `~/.claude/settings.json`'s `env` block (e.g. `ANTHROPIC_API_KEY` or a
  documented Bedrock/Vertex var). `base_url` is registry metadata only —
  grep confirms zero writes of `ANTHROPIC_BASE_URL` or equivalent anywhere
  in the codebase. So Claude Code cannot be made to run other models
  directly; the token-saving path has to be *delegation* (Claude Code asks
  another agent/model to do a subtask via SingleCLI and gets the result
  back as a tool call), not backend-switching.
- **`orchestrate-graph` does not exist** — only `orchestrate` (sequential
  relay, each agent gets the previous agent's real output) and
  `orchestrate-parallel` (independent parallel sub-tasks, no auto
  decomposition, no auto-merge) are implemented.
- **`single plugin sync` genuinely installs real Claude Code plugins**,
  running `claude plugin install <target>` inside the target home.
  Installation only — no marketplace discovery.

## Non-goals (explicitly out of scope for this spec)

- Broader SingleCLI reliability work (orchestration robustness beyond what
  this integration needs, wider registry preset coverage, general provider
  routing improvements) — separate spec, audit-first, once this lands.
- Deep token-optimization heuristics for *when* Claude Code should delegate
  — this spec ships the mechanism (`singlecli-mcp` tools) and minimal usage
  guidance; tuning is a fast-follow.
- Making Claude Code itself speak a non-Anthropic model backend — not
  possible today (see ground truth above) and not attempted here.
- `orchestrate-graph` — does not exist; not built as part of this spec.

## Design

### 1. `--real-home` on sync commands (SingleCLI)

Add a `--real-home` flag to `install-integrations`, `single plugin sync`,
and `single provider sync`, mirroring the existing `task run --real-home`
opt-in. When set, the write target resolves to the caller's actual `$HOME`
instead of `~/.config/single/homes/<agent>/`. Every write still goes
through the existing `backup_before_write` mechanism already used for
isolated-home writes — this is the safety net that makes writing to a
real, in-daily-use config acceptable. Isolation stays the default for
everyone who doesn't pass the flag; nothing about the safety model changes
for other agents or other use cases.

### 2. `singlecli-mcp` (new binary, SingleCLI)

A new stdio MCP server (new bin target, likely alongside `single-mcp` in
`crates/single-mcp` to reuse the existing `rmcp` server setup) that talks
to the running `single-runtimed` daemon over its existing Unix socket —
the same protocol `single-cli` itself already uses — rather than shelling
out to the `single` binary.

First-cut tool surface (final list confirmed during implementation
planning):

| Tool | Wraps |
|---|---|
| `task_run` | `single task run` — delegate one prompt to one agent, synchronously |
| `orchestrate_run` | `single orchestrate` — sequential relay across agents |
| `orchestrate_parallel_run` | `single orchestrate-parallel` — independent parallel sub-tasks |
| `agent_list` / `agent_inspect` | `single agent list`/`inspect` — what's available to delegate to |
| `memory_query` / `memory_store` | `single memory` — shared memory store |
| `note_leave` / `note_read` | same inbox as the gateway's `notes_*` tools |
| `provider_list` | `single provider` — what's configured, for delegation decisions |

This is the mechanism for token-saving delegation: Claude Code calls
`task_run` (or an orchestrate variant) to hand a subtask to a cheaper or
better-suited agent/model, and gets the result back as an ordinary MCP
tool result — it never needs to run that work itself.

### 3. `single-lsp` (new Claude Code plugin, cross-repo)

A new Claude Code plugin, `single-lsp`, whose bundled binary is a dynamic
LSP proxy: it speaks real LSP over stdio to Claude Code, and internally
spawns/reuses the correct real language server per opened file (by
extension/language ID) using SingleCLI's existing LSP registry/presets as
the routing table. This is the highest-uncertainty piece of the spec —
implementation planning must confirm whether Claude Code's plugin LSP
mechanism allows one plugin to register for many language IDs/extensions,
since the whole point is one entry covering 150+ presets the way the MCP
gateway covers 100+.

### 4. Claude Code rewiring (this install, `~/.claude`)

Once #1–3 exist and are verified:

- **Remove**: `github` and `playwright` plugins (their MCP function is
  subsumed by `single-mcp`'s `github`/`playwright` registry presets, both
  already enabled), the four `claude.ai` OAuth connectors (Notion, Gmail,
  Drive, Calendar — refolded via Single's `notion`/`gdrive`/
  `google-calendar` presets once their secrets are set with
  `single secret set`), and the four standalone LSP plugins
  (rust-analyzer-lsp, typescript-lsp, pyright-lsp, gopls-lsp — subsumed by
  `single-lsp`).
- **Add**: `mcpServers.single-mcp` and `mcpServers.singlecli-mcp` (both
  stdio, `command` resolved via `PATH` since both binaries install
  alongside `single`), and enable the `single-lsp` plugin.
- **Verify parity before removing anything**: every registry preset that
  replaces a removed piece (github, playwright, notion, gdrive,
  google-calendar, gmail-equivalent) must be `[enabled]` in
  `single mcp list` with any required secret already set, so no
  capability regresses during the swap.
- Update `~/.claude/CLAUDE.md`'s "Reach for installed plugins" section to
  mention `singlecli-mcp` as the delegation path and note the new
  MCP/LSP setup, so a future session doesn't try to reinvent this.

### 5. Sequencing / safety

Build and verify #1–3 first, in the SingleCLI repo, independent of the
live Claude Code config. Only perform #4 (the actual swap in `~/.claude`)
once `single install-integrations --real-home --yes` has been run and
verified — `claude mcp list` shows both `single-mcp` and `singlecli-mcp`
connected, and `single-lsp` resolves hover/diagnostics on a real file of
at least one language. This session's own Claude Code process is the one
being reconfigured, so the old config must not be removed until the
replacement is confirmed working, to avoid leaving the environment without
MCP/LSP capability mid-migration. Every edit to `~/.claude.json` /
`~/.claude/settings.json` during the swap gets a snapshot taken first.

## Data flow

- **MCP (registry tools)**: Claude Code → `single-mcp` (stdio) → gateway
  lazily spawns/reuses the target registry server as a child process,
  permission-checks the call → result flows back to Claude Code.
- **MCP (delegation/self)**: Claude Code → `singlecli-mcp` (stdio) →
  `single-runtimed` daemon over its Unix socket → `task`/`orchestrate`/
  memory/etc. executes → result flows back as an MCP tool result.
- **LSP**: Claude Code → `single-lsp` (stdio, LSP protocol) → proxy resolves
  the opened file's language against the LSP registry → spawns/reuses the
  mapped real language server → proxies `textDocument/*` both ways.
- **Configuration**: `single mcp add/enable`, `single lsp add/enable`,
  `single provider sync --real-home`, `single plugin sync --real-home`,
  `single install-integrations --real-home --yes` are the only commands
  that ever need to run again. Claude Code's own config (two MCP entries,
  one LSP plugin) does not change when new servers are added to the
  registry — only `~/.config/single/mcp.toml` /
  `~/.config/single/lsp.toml` do.

## Error handling

- `--real-home` writes reuse the existing `backup_before_write` mechanism
  already exercised for isolated-home writes — a bad sync must not
  corrupt Navin's real, in-use Claude Code config.
- `singlecli-mcp` tool handlers surface daemon-side errors (agent not
  detected, rate-limited, task failed) as normal MCP tool errors rather
  than silently swallowing them — Claude Code needs to know a delegated
  task failed, not just that it returned nothing.
- The gateway's existing permission-check path (`invoke_mcp` →
  `permissions::evaluate`) is unchanged; `singlecli-mcp`'s new tools
  should go through the same permission/preference system rather than
  bypassing it, since they can trigger real agent runs.
- Migration step (#4) is performed by hand against the real config, with a
  pre-edit snapshot, precisely because `--real-home` sync commands don't
  exist until #1 ships — this is a one-time bootstrap, not the steady
  state.

## Testing

- **SingleCLI**: extend the existing `integrations.rs` and
  `formats/claude.rs` test patterns to cover `--real-home` paths (writes
  the real path, still backs up first, still preserves unrelated keys);
  new unit tests for each `singlecli-mcp` tool handler against the daemon
  protocol.
- **`single-lsp`**: unit-testing LSP correctness end-to-end is low-value;
  instead, a manual verification pass — open real files in at least two
  languages through a Claude Code session pointed at the plugin, confirm
  hover/diagnostics/go-to-definition work.
- **Claude Code rewiring**: post-swap, `claude mcp list` must show both
  `single-mcp` and `singlecli-mcp` connected; manually exercise one
  registry-proxied tool call through `single-mcp` (e.g. a GitHub or
  Playwright call) and one delegated task through `singlecli-mcp`
  (`task_run` against a trivial prompt) to confirm both paths work
  end-to-end, not just that the config parses.

## Follow-up specs (not this one)

1. Broader SingleCLI hardening: orchestration reliability, wider registry
   coverage, general provider-routing improvements — audit-first.
2. Token-saving delegation heuristics: when Claude Code should actually
   reach for `singlecli-mcp`'s tools instead of doing work itself, as a
   skill/CLAUDE.md guidance layer once the mechanism above is live.
