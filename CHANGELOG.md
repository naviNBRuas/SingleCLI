# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/) —
pre-1.0, so the minor version (`0.x.0`) carries new functionality and the
patch version (`0.0.x`) carries fixes, per [CONTRIBUTING.md](CONTRIBUTING.md).

## [Unreleased]

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
