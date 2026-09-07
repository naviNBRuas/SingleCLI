# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/) —
pre-1.0, so the minor version (`0.x.0`) carries new functionality and the
patch version (`0.0.x`) carries fixes, per [CONTRIBUTING.md](CONTRIBUTING.md).

## [Unreleased]

## [0.10.0]

The Coordinator subsystem (`docs/queue/E27-singlecli-followups/02-coordinator-redesign.md`).
SingleCLI can now take a goal and drive a self-organising pool of agents:
a deterministic scheduler owns concurrency, the queue, per-goal budgets,
retries and restart reconciliation, while an LLM is consulted only to
plan, supervise on failure, and integrate. `single task run` and
`single orchestrate-*` are unchanged low-level escape hatches.

- Added: `crates/single-runtime/src/coordinator/` — four additive SQLite
  tables (`sessions`, `goals`, `graph_nodes`, `coordinator_events`),
  created idempotently and reconciled on daemon start so an interrupted
  run leaves no permanent zombie rows.
- Added: a pure `tick()` scheduler — ready-set from the dependency graph,
  critical-path-first admission, global + per-agent concurrency caps,
  a per-goal budget (default 25 dispatches or 60 minutes → the goal
  `blocked`, raised with `single goal amend <id> budget=N`), single-retry
  with agent advance, then a supervisor patch (capped at 5 per goal,
  then `blocked`).
- Added: routing — `~/.config/single/routing.toml` (per `kind` × `effort`
  capability-ranked agent lists) and `~/.config/single/coordinator.toml`
  (`max_parallel`, tick interval, budget caps). Nothing is pinned for
  reasoning; planner / supervisor / integrator route through the same
  table and pool-health filter as work, with dispatch-time fallback.
- Added: socket requests `SessionNew/List/Close`, `GoalSubmit/Status/
  List/Amend/Cancel`, `SessionEvents`, `CoordinatorStatus`, and the CLI
  mirrors `single session {new,list,close}`,
  `single goal {submit,status,list,amend,cancel}`,
  `single coordinator status`.
- Added: a scheduler tick-timer thread in `single-runtimed` that drives
  every active goal on `coordinator.toml`'s interval (default 5s).
- Fixed: the coordinator pool-health probe shelled out `<cmd> --version`
  for every registered agent on every scheduler tick and every
  `CoordinatorStatus` (~24s). It is now an in-process `$PATH` check;
  `single coordinator status` returns in ~10ms.
- Added: `single acp` — a native Agent Client Protocol stdio bridge
  (newline-delimited JSON-RPC 2.0, protocol v1) for Zed's agent panel.
  It burns no agent itself: `/`-commands and status-y prompts answer
  from the socket, every other prompt becomes a `GoalSubmit` and the
  coordinator's events stream back as ACP `session/update`s. A blocked
  goal asks the human via `session/request_permission`. Replaces the
  Python prototype `nbr-workspace/tools/single-acp`.

## [0.9.6]

Cleanup pass from `docs/queue/E27-singlecli-followups/singlecli-followups.md`.
Kept as a patch release; the additive flags below are conveniences.

- Fixed: `single provider add` on an already-registered provider rebuilt
  the whole entry from the flags passed, silently dropping an existing
  `base_url` or `models` list. It now merges — omitted fields keep their
  value, `models` upsert by id — so `add` is a safe way to add one model
  to a provider (there is still no separate `provider update`).
- Fixed: `--allow-fallback` did not fail over on an Anthropic HTTP 529
  `overloaded_error`; `529` and `overloaded` are now detected signals.
- Added: `--json` on `single worktree diff` / `single worktree merge`
  (the only list-ish commands that lacked it), and a post-merge hint
  pointing at `single task cleanup <id>` since the worktree and branch
  outlive the merge.
- Added: `single provider inspect` shows the provider's `models` list,
  which it persisted but never displayed.
- Added: `SINGLE_MCP_IDLE_TIMEOUT_SECS` / `SINGLE_MCP_SWEEP_INTERVAL_SECS`
  override `single-mcp`'s 600 s / 60 s defaults — mainly so the
  lazy-spawn / reuse / idle-eviction cycle is testable without a
  ten-minute wait. (Verified end-to-end: discovery spawns nothing, first
  `invoke_mcp` spawns one child, a second reuses its PID, idle eviction
  kills the OS process, a later call respawns.)
- Internal: reuse one error binding in `task::execute`'s failure arm; use
  `Context::find_agent` instead of an inline registry scan; correct
  `maybe_fail_over`'s doc comment.
- Docs: backfilled the missing `## [0.6.0]` changelog section.

## [0.9.5]

- Fixed: `single doctor` (and the `status` / `agent list` fan-out) drove
  `single-runtimed` to ~2.6 GB. The ~30 agent probes ran with no
  concurrency cap — each faulting in a full node/bun runtime for
  `--version` — a `try_wait` error left the child unreaped, and captured
  output was unbounded. Now a global 4-permit gate around every probe, a
  reap on the error path, a 64 KiB cap per probe, and a second concurrent
  `doctor` is refused with a clear error.
- Fixed: read-only commands (`status`, `task list`, `usage`,
  `--background` dispatch) hung for minutes while a `task run` executed.
  The daemon ran each request handler inline on the tokio worker, so a
  blocking agent run pinned that worker; handlers now run on the blocking
  pool and a panicking handler returns a structured error.
- Fixed: the daemon's agent detection depended on its launcher's pinned
  `PATH` being complete, so a new agent bin dir or a bumped nvm Node
  version silently read as "not installed". `single-runtimed` now appends
  the standard install locations that exist on disk at startup, keeping
  inherited entries first.

## [0.9.4]

- Fixed: `mistral-vibe` had a registry entry but no adapter, so every run
  fell through to `GenericAdapter` and failed with "no [run] mode
  defined". Added `MistralVibeAdapter` (`vibe -p <prompt> --auto-approve
  --output text --trust`).
- Fixed: `kilocode` passed `kilo run`'s old `--auto` / `--dir` flags,
  which kilo 7.x rejects (hanging the run). Now `kilo run -- <prompt>`
  (kilo run auto-approves by default).
- Fixed: `copy_dir_recursive` unwrapped every `fs::copy`, so one
  unreadable entry in a real agent config dir (an IDE socket, a
  FIFO-backed lock, a file deleted mid-copy) aborted the whole
  isolated-home bootstrap. Now best-effort per file, special types
  skipped.

## [0.9.3]

- Removed: the `gemini` agent and the `gemini` provider preset. Google
  discontinued the standalone Gemini CLI (its agent role is covered by
  `agy`/Antigravity) and its API is now Google AI Studio — the provider
  preset is replaced by `google` (`GOOGLE_API_KEY`, same
  OpenAI-compatible `v1beta/openai` endpoint). `qwen-code` is unaffected.

## [0.9.2]

- Fixed: `single-runtimed` now reconciles orphaned tasks on startup. A
  daemon killed mid-run left its rows stuck `running`/`created` forever
  with no way to reap them; a freshly started daemon sweeps every
  non-terminal row to `failed` (summary "interrupted") and records a
  `task.reconciled` event.
- Added: `--force` on `single task cancel` and `single task cleanup` to
  clear a row wedged non-terminal with no live process behind it.
- Fixed: task-failure memories (`task #N failed`) are written at `task`
  scope instead of `project` and are skipped by the task-run context
  preamble, so they no longer accumulate as noise in every agent's
  prompt. Still queryable via `single memory list --scope task`.
- Fixed: `single task list` truncates each description to its first line,
  clipped, so multi-line agent prompts no longer break the table.

## [0.9.0]

- Added: `crates/single-native-agent` — a native, in-process coding agent
  (binary `single-agent`) that talks directly to any provider registered
  in `providers.toml` via its OpenAI-compatible chat-completions endpoint,
  no third-party CLI required. Ships a minimal tool loop (`read_file`,
  `write_file`, `run_shell`) scoped to `--cwd` with path-traversal and
  absolute-path escapes rejected before any filesystem access, and a
  `call_mcp` tool that spawns `single-mcp` once per run and proxies
  through to its `invoke_mcp` tool — the same lazy MCP-gateway access
  codex/opencode already get. Wired into the agent registry as
  `single-agent`, dispatchable via `single task run --agent single-agent`.
  Provider/model selection currently reads `SINGLE_AGENT_PROVIDER`/
  `SINGLE_AGENT_MODEL` env vars (default `opencode-zen`/
  `laguna-s-2.1-free`) — the standard adapter `run_prompt` signature has
  no field for it yet.
- Added: Kilo Code (`kilocode`) to the agent registry — a real,
  actively-maintained open-source fork of OpenCode
  (github.com/Kilo-Org/kilocode) with its own standalone CLI and free
  models available without an API key.
- Fixed: `single task run`'s failure summary picked whichever line came
  first even when it was pure decoration (box-drawing separators some
  TUI-styled agents print around their real output) — now skips any line
  with no alphanumeric content.
- Fixed: `qwen-code`'s `login()` was unimplemented (`qwen auth` is
  genuinely removed upstream) — now launches `qwen` interactively in its
  isolated home so a user can reach the in-TUI `/auth` command
  themselves. Note: Qwen's free OAuth tier was discontinued 2026-04-15
  upstream — this reaches the right screen, it doesn't restore free auth.
- Fixed: the TUI's Agents-tab status dot was colored solely from whether
  the agent's binary was detected on disk, so a detected-but-not-
  authenticated agent rendered an identical green dot to a fully working
  one. Now factors in auth state: green only when confirmed authenticated,
  yellow when detected-but-unauthenticated or auth state is unconfirmed.
- Fixed: `amp`'s adapter passed `-x -- "<prompt>"`, which strips the
  message from `-x`/`--execute` entirely — that flag takes its value
  inline (`-x, --execute [message]`), not as a following positional, so
  `--` (added to dodge task_run's `---`-prefixed memory preamble) ended
  option parsing before the message could bind. Every real `amp` call
  failed with a parse error regardless of auth. Fixed to
  `--execute=<prompt>` as one token.
- Fixed: `single-native-agent` sent `content: null` on assistant messages
  alongside `tool_calls` — valid per the OpenAI spec, but Cloudflare
  Workers AI's "OpenAI-compatible" endpoint rejected it outright with a
  schema-validation error. Now sends an empty string instead.
- Docs: registered ~40 additional free/freemium LLM providers in
  `providers.toml` (metadata only — Cerebras, Groq-family aggregators,
  OpenRouter, Nebius, Hyperbolic, ModelScope, and others), cross-checked
  against independent verified sources rather than any single vendor's
  own marketing claims.

## [0.8.1]

- Fixed: `single task run`'s failure summary ignored stderr entirely, so
  any agent CLI that wrote its real error to stderr (codex, opencode, and
  most others) surfaced as the useless `exit code Some(1), no output` —
  the actual error sat unread in the task artifact file. `summarize()` now
  falls back to stderr when stdout has nothing useful, and prefers the
  last `Error`/`ERROR`-marked line over whichever line happens to come
  first, since CLIs commonly print startup/banner noise before their real
  failure.

## [0.8.0]

- Added: `single provider add` gains a repeatable `--model <id>:<name>`
  flag, and `single provider sync` can now write a custom/local provider
  into opencode's own `opencode.jsonc` `provider` key (via
  `@ai-sdk/openai-compatible`) — previously, syncing a provider only
  injected its API key as an env var into an agent's process, which got
  the credential to opencode but never told opencode's model picker the
  provider existed. `provider_sync::sync`'s API-key value is never
  written into opencode's config — only an `{env:VAR}` reference, matching
  how the actual secret already reaches opencode's process.

## [0.7.0]

- Added: rate-limit detection (`single_core::ratelimit::looks_like_rate_limit`)
  now runs unconditionally on every task completion, not just when
  `--allow-fallback` is set, and is surfaced as `TaskRecord.rate_limited`.
- Added: `HomeRequirement` (whether an agent needs `real_home: true`,
  breaks under it, works either way, or is unverified) and
  `max_concurrency` are now queryable per agent via `agent_list`/`agent
  inspect`, closing a gap where this was previously tribal knowledge.
- Added: `CapabilityFlags.non_interactive_run` flags agents (currently
  only `codebuff`) with no non-interactive run mode at all, checkable
  before dispatching instead of discovering it via a failed run.
- Added: a per-agent concurrency guard serializes agents with a
  `max_concurrency` limit (currently `opencode`, whose own session
  SQLite lock can't handle two simultaneous instances) instead of
  letting a second concurrent run crash.
- Added: `single worktree diff`/`merge` CLI commands and
  `worktree_merge_preview`/`worktree_merge_apply` MCP tools give an
  assisted (never automatic) way to merge a worktree-isolated task's
  branch back — branches still aren't auto-merged, this just removes
  the manual `git` juggling once you've decided to.
- Fixed: the concurrency guard above originally held its slot through
  `maybe_fail_over`'s fallback path, which could self-deadlock if a
  fallback chain repeated the same concurrency-limited agent — the slot
  is now released as soon as the agent's subprocess actually finishes.

## [0.6.0]

Backfilled 2026-09-06 — this section was missing (the `0.7.0` and
`0.8.0` work landed above it without one). Contents reconstructed from
the `0.5.0..0.6.0` commit range.

- Added: `singlecli-mcp` — an MCP server exposing SingleCLI's own
  commands as tools (`task_run`, `orchestrate_run`,
  `orchestrate_parallel_run`, `orchestrate_graph_run`, plus agent,
  memory, and provider tools), synced alongside the MCP registry/gateway.
- Added: `single-lsp` — a dynamic LSP proxy that spawns the real language
  server for an open file's extension on first use, with a generated
  Claude Code plugin manifest driven by the LSP registry (filename-keyed
  presets that Claude Code's manifest schema rejects are skipped).
- Added: `--real-home` on `provider sync`, `plugin sync`,
  `install-integrations`, and `uninstall-integrations` — write into the
  actual `$HOME` instead of an isolated copy.
- Added: task lifecycle hooks (`single task-hook add/list/remove/test`).
- Added: `single web` pattern-library browsing (`single web patterns
  list|search`) via the new `single-web` crate.
- Fixed: `singlecli-mcp` now canonicalizes `cwd` client-side before
  sending requests.
- Fixed: `plugin sync` backs up `claude`'s `settings.json` before it
  writes into a real home.

## [0.5.0]

- Added: `task run --allow-fallback` fails over to another agent/account
  when a run hits a detected rate limit — `single fallback set
  <agent[:account]>...` saves the ordered chain to try. Detection honors
  an account already marked `rate_limited` (`single account set-status`)
  and, best-effort, generic rate-limit signals in a failed run's output.
  A follow-up task is created and linked, never a silent retry.
- Fixed: the TUI blocked on every tab's data before drawing anything —
  on a cold daemon this could take several seconds with a blank
  terminal. `App::new()` now draws the tab shell immediately with an
  animated loading spinner while the first fetch runs in the background;
  every later `[r]` refresh is non-blocking too.
- Fixed: `Status` and `AgentList` each independently re-discovered every
  registered agent when fired together (as the TUI's startup fetch
  does), doubling the subprocess spawns needed on a cold cache — now
  single-flighted per agent.
- Fixed: MCP, LSP, Plugins, Tools, Providers, Accounts, and the Tasks
  tab's workspace list had no column headers, unlike Agents/Tasks —
  every list in the TUI is a proper table now.

## [0.4.0]

- Added: tasks are now grouped by workspace (a stable identity — git
  remote URL, or root commit hash with no remote — that survives the
  project directory moving) instead of one flat list; the TUI's Tasks tab
  drills from a workspace list into that workspace's tasks. New `single
  workspace list`.
- Added: `single mcp enable-all` / `single lsp enable-all` bulk-enable
  every registered server, skipping only MCP servers still missing a
  required secret.
- Added: MCP gateway mode is now backed by real idle eviction — a lazily
  spawned server unused for 10 minutes is dropped instead of staying
  alive for the whole agent session. The TUI's MCP tab shows gateway
  status and toggles it with `[g]`.
- Fixed: every scrollable list/table in the TUI (Agents, Tasks, MCP, LSP,
  Plugins, Tools, Providers, Accounts) silently truncated past the
  visible height with no way to scroll further; they now show a proper
  scroll window and position indicator.
- Fixed: the daemon re-ran full agent discovery (`which`/`--version` per
  agent) and re-parsed the entire MCP/LSP/plugin/provider registries on
  every single request instead of once per process, making `single`
  slow to open — both are now cached/parallelized.

## [0.3.3]

- Fixed: encrypted backup restore rejected path traversal.

## [0.3.2]

- Fixed: malformed version tags rejected during update comparison.

## [0.3.0]

- Added: dependency-graph task orchestration and orchestrator modes.
- Fixed: OpenCode and Grok tasks silently no-op'd without approval flags.
- Fixed: Codex tasks ran in a read-only sandbox and could never write files.
- Fixed: `--background` with no daemon running now warns instead of
  silently no-op'ing.
- Fixed: OpenCode LSP sync omitted a now-required `extensions` field.

## [0.1.29]

- Added: named account capture/switch for Cursor, Copilot, Grok, Codebuff.

## [0.1.28]

- Added: Providers tab shows only configured providers, not every preset.

## [0.1.27]

- Fixed: `docker exec -e KEY=value` leaked secret values into process
  argv.

## [0.1.26]

- Added: inject stored provider keys into agent runs as environment
  variables.

## [0.1.25]

- Fixed: `SecretStore::list()` found nothing even when secrets were
  genuinely stored.

## [0.1.24]

- Fixed: the Usage tab's fetch could block the whole TUI event loop.

## [0.1.23]

- Added: Usage and Backup TUI pages.

## [0.1.22]

- Fixed: dash-prefixed prompts broke non-interactive runs for nearly
  every agent.

## [0.1.21]

- Fixed: gateway-mode MCP entries accumulated instead of replacing;
  `cursor-agent` needed `--trust`.

## [0.1.20]

- Fixed: honest account-capture error message, dropped a dead-end retry
  hint.

## [0.1.19]

- Added: daemon restart command, real adapters for 8 more agents, an
  honest `agy` auth hint.

## [0.1.18]

- Fixed: preset visibility; expanded tool/agent/provider catalogs.

## [0.1.17]

- Added: real parallel agent execution, knowledge-graph shared context,
  live agent-to-agent messaging.

## [0.1.16]

- Added: 50+ MCP presets, 100+ LSP presets, a plugin marketplace catalog,
  `doctor` coverage for semantic search.

## [0.1.15]

- Fixed: removed a real-home fallback from auth detection and account
  capture.

## [0.1.14]

- Fixed: propagate the real config dir into HOME-swapped child processes.

## [0.1.13]

- Added: real mid-run permission interception.

## [0.1.12]

- Added: permission enforcement and learned preferences.

## [0.1.11]

- Added: opt-in Docker execution backend.

## [0.1.10]

- Added: `single-mcp` dynamic gateway.

## [0.1.9]

- Added: document ingestion (OCR) and semantic memory search.

## [0.1.8]

- Added: memory context injection, agent notes, more presets.

## [0.1.7]

- Fixed: self-update `ETXTBSY` on cross-device installs.

## [0.1.6]

- Fixed: account capture / auth detection.

## [0.1.5]

- Maintenance release.

## [0.1.4]

- Added: `--real-home` opt-out for system-configuration tasks.
- Added: live task output in the TUI, with per-agent detail for
  `orchestrate`.

## [0.1.3]

- Added: Cursor CLI, Aider, and Goose as built-in agents; GitHub Copilot
  CLI, Kiro CLI, and Sourcegraph Cody as three more (Windsurf investigated
  and left out — see `docs/install-methods.md`).
- Added: `single agent login` for real interactive auth into isolated
  homes; every agent run isolated to a SingleCLI-managed home.
- Added: MCP/LSP preset catalogs, full TUI config surface (MCP/LSP/
  Plugins/Tools tabs), plugin management, multi-account isolation, in-TUI
  task creation.
- Added: LSP sync into OpenCode, skills synced into Claude Code, expanded
  tool registry.
- Docs: plugins, multi-account concurrency, LSP/skills sync, presets.

## [0.1.2]

- Fixed: Linux release binaries built against musl, not glibc (a glibc
  build failed to start on a real Debian 12 test machine).
- Added: self-update (`single update`) and a nightly release channel.
- Added: multi-agent orchestration (sequential relay).
- Added: "learn from errors" — task failures automatically become memory
  entries.
- Added: provider presets (including NVIDIA) and in-TUI provider
  configuration.
- Added: MCP/LSP/tool registries seeded with real defaults instead of a
  near-empty list.
- Added: TUI rewritten as a tabbed control center with in-app agent
  install.
- Added: cross-platform release workflow and curl installer.
- Added: pluggable Redis working-memory and Qdrant vector-store backends.
- Added: SQLite-backed knowledge graph memory.
- Added: LLM provider registry with real config sync (Phase 6, partial).
- Added: declarative custom agent adapters (Phase 5).
- Added: multi-account credential switching (`single account ...`).
- Added: Phase 4 single-agent task execution with worktree isolation.
- Added: Phase 3 memory subsystem and project context resolver.
- Added: Phase 2 shared-capability registries (MCP CRUD, LSP, tools,
  secrets, permissions, skills).

## [0.1.0]

Initial public release.

- Added: agent registry for Claude Code, Codex, OpenCode, Antigravity
  (`agy`), and Perplexity (`pplx`), with real detection/version/capability
  observation.
- Added: unified MCP registry synced into each agent's native config
  format, with backups.
- Added: vendor-verified bootstrap installers (`single setup`).
- Added: headless runtime daemon over a Unix socket with a CLI and TUI
  client.
- Added: `single` CLI and TUI dashboard.
- Docs: architecture, ADR, and install-methods documentation; README and
  MIT license.

[Unreleased]: https://github.com/naviNBRuas/SingleCLI/compare/v0.3.3...HEAD
[0.3.3]: https://github.com/naviNBRuas/SingleCLI/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/naviNBRuas/SingleCLI/compare/v0.3.0...v0.3.2
[0.3.0]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.29...v0.3.0
[0.1.29]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.28...v0.1.29
[0.1.28]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.27...v0.1.28
[0.1.27]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.26...v0.1.27
[0.1.26]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.25...v0.1.26
[0.1.25]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.24...v0.1.25
[0.1.24]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.23...v0.1.24
[0.1.23]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.22...v0.1.23
[0.1.22]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.21...v0.1.22
[0.1.21]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.20...v0.1.21
[0.1.20]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.19...v0.1.20
[0.1.19]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.18...v0.1.19
[0.1.18]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.17...v0.1.18
[0.1.17]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.16...v0.1.17
[0.1.16]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.13...v0.1.14
[0.1.13]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.12...v0.1.13
[0.1.12]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.11...v0.1.12
[0.1.11]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/naviNBRuas/SingleCLI/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/naviNBRuas/SingleCLI/releases/tag/v0.1.0
