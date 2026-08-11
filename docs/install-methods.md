# Verified install methods for agent CLIs

This records exactly how `single setup` installs each agent CLI, and where
each command was verified. Per the project's own rule ("do not fabricate
APIs, flags, configuration formats, or capabilities"), every command below
was fetched directly from the vendor's own current documentation or GitHub
README — not taken on trust from a research summary — before being wired
into `crates/single-core/src/registry.rs`.

| Agent | Command | Install command | Source |
|---|---|---|---|
| Claude Code | `claude` | `curl -fsSL https://claude.ai/install.sh \| bash` | https://code.claude.com/docs/en/setup |
| Codex CLI | `codex` | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` | https://github.com/openai/codex/blob/main/README.md |
| OpenCode | `opencode` | `curl -fsSL https://opencode.ai/install \| bash` | https://opencode.ai/docs/ |
| Antigravity CLI | `agy` | `curl -fsSL https://antigravity.google/cli/install.sh \| bash` | https://antigravity.google/docs/cli/install |
| Perplexity | `pplx` | `curl -fsSL https://github.com/perplexityai/perplexity-cli/releases/latest/download/install.sh \| sh` | https://docs.perplexity.ai/docs/cli/overview |

## Important caveat: Perplexity does not have a coding-agent CLI

The task that seeded this project assumed a "Perplexity CLI" comparable to
Claude Code, Codex, OpenCode, and Antigravity — an interactive coding agent.
As of this writing, no such product exists. Perplexity's only official CLI
is `pplx`, a thin client for the **Perplexity Search API**: it does web
search and content-snippet extraction and returns structured JSON. It has
no chat/session model, no tool-use loop, and nothing resembling an
interactive coding agent — it's the kind of tool a coding agent *calls*,
not one itself.

The registry still installs it (the user explicitly asked for every agent
to be installable through SingleCLI, and `pplx` is the closest real
product), but `AgentDefinition.notes` on the `perplexity` entry flags this
plainly, `capabilities.sessions` is `false`, and `single doctor` /
`single agent inspect perplexity` surface the caveat rather than presenting
it as equivalent to the other four. If Perplexity ships an actual coding
agent in the future, this entry should be revisited.

## Confirmed-installed reference points (used to build the registry)

On the machine this was built on, `claude`, `codex`, `opencode`, and `agy`
were already installed. Their real, observed installation methods (as
opposed to the fresh-install bootstrap commands above) are recorded in
`registry.rs`'s `InstallMethod` field for each agent:

- **Claude Code**: native installer, binary under `~/.local/share/claude/versions/<ver>`, symlinked from `~/.local/bin/claude`. `installMethod: "native"` is also recorded directly in `~/.claude.json`.
- **Codex**: its own standalone package manager under `~/.codex/packages/standalone/`, symlinked from `~/.local/bin/codex`.
- **OpenCode**: a standalone binary at `~/.opencode/bin/opencode`.
- **Antigravity (`agy`)**: a standalone binary at `~/.local/bin/agy`, with its own `agy install` subcommand for PATH/shell-alias setup (confirmed via `agy install --help`, which shows the same `--skip-aliases`/`--skip-path` flags documented on antigravity.google's install page — good corroborating evidence the fetched page is genuine).

None of the four were installed via a shared/generic package manager (no
npm-global entries for any of them) — this is why `single setup` runs each
vendor's own install script rather than assuming `npm install -g` works
universally.

## Version-check caveat

Codex CLI's and Antigravity's official docs don't explicitly document a
`--version` flag the way Claude Code and OpenCode do. `codex --version` and
`agy --version` were confirmed to work directly on the reference machine
(see the Phase 1 investigation in this repo's git history), so
`single-agent-sdk::discover` uses `<command> --version` uniformly — but
treat this as machine-observed behavior, not vendor-documented contract,
if either CLI changes its `--version` output format in the future.
