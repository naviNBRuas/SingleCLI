# SingleCLI

A unified control plane for heterogeneous AI coding-agent CLIs — Claude
Code, Codex, OpenCode, Antigravity (`agy`), and Perplexity's `pplx`.
Configure MCP servers once in SingleCLI; every supported agent gets the
same configuration synced into its own native config format.

> **Status: Phase 1 (foundation).** See "What's implemented" below and
> `docs/architecture.md` for the full picture, including what's
> deliberately *not* here yet.

## Why

Every one of these CLIs maintains its own independent config for the same
underlying capability (MCP servers, in Phase 1's case) — `~/.claude.json`,
`~/.codex/config.toml`, `~/.config/opencode/opencode.jsonc`, each in a
different format, each requiring the same server list to be entered by
hand. SingleCLI keeps one unified registry and syncs it out to whichever
of these CLIs are installed, and can install the CLIs themselves on a
machine that has none of them yet.

## Install

Build from source (no published binary yet — Phase 1):

```bash
cargo build --release --workspace
```

Binaries land in `target/release/`: `single` (the CLI/TUI) and
`single-runtimed` (the headless runtime daemon). Put both on `$PATH`.

## Quickstart

```bash
single doctor          # what's installed, what SingleCLI can manage
single agent list      # the agent registry, live detection status
single mcp list         # the unified MCP registry
single setup            # plan: install missing agent CLIs (dry run by default)
single setup --yes       # actually run vendor install scripts + sync config
single install-integrations --yes   # sync MCP config into every agent, with backups
single task run "add a .gitignore" --agent claude   # delegate a prompt to a real agent, synchronously
single task list
single                  # launch the TUI dashboard
```

Every list/inspect command supports `--json` for scripting.

## What's implemented

- **Phase 1** — a real agent registry for `claude`, `codex`, `opencode`,
  `agy`, and `perplexity` (`pplx`) with detection/versions/capabilities
  observed from real config files and `--version` output; a unified MCP
  registry synced into each agent's native config format with backups;
  vendor-verified bootstrap installers (`single setup`); a headless
  runtime daemon over a Unix socket with a CLI and TUI client; profiles
  and config precedence.
- **Phase 2** — CRUD for the MCP/LSP/tool registries, an OS-keychain
  secrets abstraction, a deny/ask/allow permission model (data only, not
  yet enforced), and a local skills directory.
- **Phase 3** — a SQLite-backed, scoped, provenance-tagged memory store,
  and a git/project context resolver.
- **Phase 4** — real single-agent task execution: `single task run`
  invokes an agent CLI's actual non-interactive mode (`claude -p`, `codex
  exec`, `opencode run`, `agy -p`), optionally isolated in a real git
  worktree, captures the output as an artifact, and records the result.

## What's not (yet)

A multi-agent task-graph orchestrator (parallel execution, agent-to-agent
communication, automatic agent selection), permission *enforcement* (the
model exists, nothing calls it), LSP syncing into agents, a plugin
marketplace, semantic memory search, workflows, and a provider abstraction
are later work. See `docs/architecture.md`'s "Not in Phase 1-4" section
for the full, honest list.

## Development

```bash
cargo build --workspace
cargo test --workspace
```

No API keys are required to build, test, or run `doctor`/`agent list`.
`single setup --yes` and `single install-integrations --yes` do touch real
files/run real installers — everything else is read-only or operates in a
temp directory during tests.

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — crate layout, data flow, known Phase 1 simplifications.
- [`docs/adr/0001-tech-stack.md`](docs/adr/0001-tech-stack.md) — why Rust/Unix-socket/SQLite, and what was considered instead.
- [`docs/install-methods.md`](docs/install-methods.md) — verified install commands and sources for every agent CLI.

## License

MIT — see [`LICENSE`](LICENSE).
