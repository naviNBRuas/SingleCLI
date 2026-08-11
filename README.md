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
single                  # launch the TUI dashboard
```

Every list/inspect command supports `--json` for scripting.

## What's implemented (Phase 1)

- A real agent registry for `claude`, `codex`, `opencode`, `agy`, and
  `perplexity` (`pplx`) — detection, versions, and capabilities are
  observed from real config files and `--version` output, not invented.
- A unified MCP registry (`~/.config/single/mcp.toml`) synced into each
  agent's native config format, with a backup taken before every write.
- Vendor-verified bootstrap installers (`single setup`) for all five —
  sources cited in `docs/install-methods.md`.
- A headless runtime daemon (`single-runtimed`) over a Unix socket, a CLI
  client, and a TUI dashboard that talks to the daemon over that same
  socket.
- Profile switching and config precedence (global → profile → project).

## What's not (yet)

Agent orchestration/task graphs, shared memory, LSP management, skills,
plugins, a secrets vault, sandboxing, and process lifecycle management
(actually running/streaming an agent session through SingleCLI) are later
phases. See `docs/architecture.md`'s "Not in Phase 1" section.

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
