# SingleCLI

A unified control plane for heterogeneous AI coding-agent CLIs — Claude
Code, Codex, OpenCode, Antigravity (`agy`), Perplexity's `pplx`, and any
new agent CLI you describe in a TOML file. Configure MCP servers,
provider keys, and accounts once in SingleCLI; every supported agent gets
the same configuration synced into its own native format.

> **Status: Phases 1-4 done, Phases 5-6 partial, plus two rounds of
> growth work (plugins, multi-account concurrency, LSP/skills sync,
> preset catalogs, a fuller TUI).** See "What's implemented" below and
> `docs/architecture.md` for the full picture, including what's
> deliberately *not* here yet.

## Why

Every one of these CLIs maintains its own independent config for the same
underlying capabilities — MCP servers, provider API keys, login
credentials — each in a different file/format. SingleCLI keeps one
unified registry for each and syncs it out to whichever CLIs are
installed, installs the CLIs themselves on a machine that has none of
them yet, and lets you add a brand-new agent CLI to the whole system by
writing one TOML file, no recompilation required.

## Install

**Linux and macOS (x86_64 and arm64):**

```bash
curl -fsSL https://raw.githubusercontent.com/naviNBRuas/SingleCLI/main/install.sh | sh
```

Downloads the prebuilt `single` and `single-runtimed` binaries for your
platform from the latest [release](https://github.com/naviNBRuas/SingleCLI/releases)
to `~/.local/bin` (override with `SINGLE_INSTALL_DIR`). See
[`install.sh`](install.sh) — it's a plain shell script, read it before
piping it into `sh` if you want to know exactly what it does.

**From source** (any platform with a Rust toolchain):

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
single setup --yes       # install missing agent CLIs + sync config
single install-integrations --yes   # sync MCP config into every agent, with backups
single task run "add a .gitignore" --agent claude   # delegate a prompt to a real agent, synchronously
single orchestrate "add tests for the parser" --agents claude,codex --worktree   # relay across multiple agents

single account capture claude work      # snapshot the currently logged-in Claude account
single account use claude personal      # switch to a different captured account

single provider presets                             # OpenAI, Anthropic, OpenCode Zen, NVIDIA
single provider add-preset nvidia
single provider set-key nvidia nvapi-...
single provider sync nvidia --agents claude --yes

single mcp presets                                  # brave-search, slack, puppeteer, postgres
single mcp add-preset brave-search
single lsp presets                                  # rust-analyzer, pyright, typescript, gopls, dockerfile, clangd, bash, yaml, terraform, json
single lsp add-preset clangd

single plugin add my-plugin my-plugin@official       # target used verbatim by claude/codex/agy
single plugin sync my-plugin --agents claude --yes   # runs `claude plugin install my-plugin@official`

single account capture claude work --label work@example.com   # snapshot the currently logged-in Claude account
single account use claude personal                             # switch to a different captured account
single account set-status claude work rate_limited             # track usability (manual — no agent exposes a quota API)
single task run "..." --agent claude --account work             # run against an isolated $HOME so multiple
                                                                  # accounts of the same agent run concurrently

single skill install my-skill ./my-skill-dir
single skill sync-claude my-skill       # copies it into ~/.claude/skills/my-skill/

single memory graph create-entity SingleCLI project
single memory graph show                # dump the shared knowledge graph

single                  # launch the TUI: Agents/Tasks/MCP/LSP/Plugins/Tools/Providers/Accounts/Memory tabs,
                        # [i] installs an agent interactively, [n] creates a task, [a] quick-adds
                        # into MCP/LSP/Plugins/Tools, [d]/[e]/[s] remove/toggle/sync the selection

single update --check                       # is a newer stable build available?
single update --yes                         # replace the running binaries in place
single update --channel nightly --yes       # track main instead of tagged releases
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
- **Phase 5** — declarative custom agents: describe a brand-new CLI agent
  in `~/.config/single/agents/<name>.toml` (command, install command,
  prompt mode, MCP format) and it gets real detection, MCP sync, and task
  execution identically to the five built-in agents — no Rust required.
- **Phase 6 (partial)** — a provider registry (OpenAI, Anthropic, OpenCode
  Zen, ...) with keys held in the OS keychain, synced into the agents with
  a verified config slot for them (Claude Code's `env` settings, Codex's
  `OPENAI_API_KEY`).
- **Auth** — `single account capture/use/list/remove`: snapshot an agent's
  current login state as a named profile and switch between them later —
  e.g. two separate Claude Code accounts — with automatic backups and
  without ever printing token contents.
- **Memory upgrades** — a shared SQLite knowledge graph (entities,
  observations, typed relations — `single memory graph ...`), plus
  optional Redis working memory (`single memory cache ...`,
  `SINGLE_REDIS_URL`) and Qdrant vector storage/search for RAG
  (`single memory vector ...`, `SINGLE_QDRANT_URL`) — both built and
  tested against real local instances. Task failures are automatically
  recorded as searchable memory ("learn from errors").
- **Multi-agent orchestration** — `single orchestrate "<goal>" --agents
  a,b,c [--worktree]` runs several agents in sequence on one goal, sharing
  one git worktree and handing each agent the previous one's real
  captured output. A sequential relay, not live parallel chat — see
  `docs/architecture.md` for the honest scope.
- **Provider presets** — OpenAI, Anthropic, OpenCode Zen, and NVIDIA,
  configurable from the TUI's Providers tab (`[a]`, masked key entry
  straight to the OS keychain) or `single provider add-preset <name>`.
- **Richer starter registries** — the default MCP/LSP/tool catalogs ship
  with real, commonly-used entries (fetch, sequential-thinking,
  rust-analyzer, pyright, docker, gh, ...) instead of a near-empty list,
  seeded from this project's own verified working configuration, plus
  preset catalogs (`single mcp/lsp presets`, `add-preset <name>`) to grow
  either registry without a code change.
- **Self-update** — `single update` checks GitHub Releases and replaces
  its own binaries in place, on a `stable` (tagged releases) or `nightly`
  (rebuilt on every push to `main`) channel.
- **Plugin management** — `single plugin add/remove/list/inspect/sync`:
  installs a named plugin via each agent's own real command (`claude
  plugin install`, `codex plugin add`, `opencode plugin <module>`, `agy
  plugin install`). Installation only — no marketplace browsing/discovery.
- **LSP sync into OpenCode** — `single install-integrations` now writes
  the LSP registry into `opencode.jsonc`'s real `lsp` key alongside MCP
  (the only agent with a confirmed native LSP config surface).
- **Skills synced into Claude Code** — `single skill sync-claude <name>`
  copies a locally-installed skill into Claude's real skill directory
  (`~/.claude/skills/<name>/`), backing up any existing same-named
  directory first.
- **Account labels, status, and concurrent multi-account execution** —
  captured account profiles can carry a human label and a manually-set
  status (`available`/`rate_limited`/`needs_topup`/`unknown` — never
  auto-detected, since no agent exposes a verified quota API). `single
  task run --account <name>` runs against a materialized, isolated
  `$HOME` for that account instead of swapping the live one in place, so
  multiple accounts of the same agent (two `claude`, three `codex`, ...)
  can run **concurrently** without clobbering each other.
- **TUI: full config surface + task creation** — LSP/Plugins/Tools tabs
  alongside the original Agents/Tasks/MCP/Providers/Accounts/Memory/Help;
  a quick-add flow for MCP/LSP/Plugins/Tools, remove/toggle/sync
  keybindings, and an in-TUI task-creation flow (description → workspace
  path → pick one or more agents).

## What's not (yet)

A parallel/live multi-agent task-graph (as opposed to the sequential
relay that exists), permission *enforcement* (the model exists, nothing
calls it), a real text-to-vector embeddings pipeline (Qdrant integration
stores/searches vectors you already have), LSP syncing into agents other
than OpenCode, a plugin marketplace/discovery layer (installing a named
plugin is real; browsing what's available is not), workflows, and full
model/provider abstraction (discovery, streaming, usage accounting) are
later work. See `docs/architecture.md`'s "Not in Phase 1-6" section for
the full, honest list.

## Development

```bash
cargo build --workspace
cargo test --workspace
```

No API keys are required to build, test, or run `doctor`/`agent list`.
`single setup --yes`, `single install-integrations --yes`, `single account
use`, and `single provider sync --yes` touch real files/run real
installers — everything else is read-only or operates in a temp directory
during tests. The Redis and Qdrant backends are optional (unset
`SINGLE_REDIS_URL`/`SINGLE_QDRANT_URL` and their commands just report "not
configured"); their tests skip cleanly, rather than fail, when no such
service is reachable — to actually exercise them locally:

```bash
docker run -d -p 6379:6379 redis:7-alpine
docker run -d -p 6333:6333 qdrant/qdrant
SINGLE_REDIS_URL=redis://127.0.0.1:6379 SINGLE_QDRANT_URL=http://127.0.0.1:6333 cargo test --workspace
```

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — crate layout, data flow, and every phase's honest scope/limitations.
- [`docs/adr/0001-tech-stack.md`](docs/adr/0001-tech-stack.md) — why Rust/Unix-socket/SQLite, and what was considered instead.
- [`docs/install-methods.md`](docs/install-methods.md) — verified install commands and sources for every agent CLI.

## License

MIT — see [`LICENSE`](LICENSE).
