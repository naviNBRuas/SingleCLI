# Usage page and Backup page — design

## Status

Proposed — awaiting user review before an implementation plan is written.

## Context

Two new TUI tabs, requested together as "next version" work:

1. **Usage** — spend/usage across all connected agents and providers, itemized, with a total.
2. **Backup** — export/import SingleCLI's entire setup (config, auths, credentials) to move to another machine.

Both are greenfield: no usage/cost tracking exists anywhere in the codebase today (`RunOutcome`/`TaskRecord` have no token/cost field), and no backup/export/import command exists (`backup_before_write` in `single-agent-sdk` is a per-file pre-overwrite `.bak` copy, unrelated to a full-system backup).

Existing patterns both features build on:
- **Tab pattern** (`single-tui/src/app.rs`, `ui.rs`): `Tab` enum variant → `title()` arm → `current_len()` arm → an `App` field populated in `refresh()` via `Request`/`ResponseData` → a `draw_x` fn in `ui.rs`. All 10 existing tabs (Agents, Tasks, Mcp, Lsp, Plugins, Tools, Providers, Accounts, Memory, Help) follow this exactly.
- **Registry pattern** (`single_core::registry::AgentDefinition`, `single-agent-sdk::adapters::for_agent`): a static list of entries, each honestly flagged `unverified`/capability-limited rather than guessed. The billing-provider registry (below) follows the same discipline.
- **Secret storage**: `single_core::secrets::SecretStore` trait, one real backend today (`secret-tool`/libsecret/gnome-keyring — no file fallback). Both features depend on this.
- **Per-file backup-before-write**: every config writer in `single-agent-sdk` already does this before overwriting a file. Backup-import reuses it.

## Key constraint that shaped this design

Provider billing APIs report spend **per API key**, not per local CLI tool. Most of the agents already set up this session (claude, codex, cursor, copilot, kiro, cody) authenticate via native OAuth tied to a subscription/account — there is no public API for "how much has my Claude Code subscription used this month." Real `$` tracking is only possible for **BYOK agents** (agents configured with a raw, user-issued API key: aider, goose, grok, opencode-zen, crush, and any agent pointed at a provider via `single provider sync`).

Decision (confirmed with user): OAuth agents still appear on the Usage page, with local-only stats (run count, duration, last-run — sourced from existing `TaskRecord` data, no new schema needed), clearly labeled as having no billing API available. BYOK agents show real `$`, contingent on the user setting up one API key per agent per provider (see Multi-key model below) so spend can actually be attributed rather than shown as one undifferentiated provider total.

---

## Part 1: Usage page

### 1.1 Multi-key provider model

Today, `single provider` stores exactly one secret per provider name (`ProviderSpec { name, env_var_name, secret_name, base_url }`, `single-protocol/src/lib.rs:833`), synced to a list of agents via `Request::ProviderSync { name, agents, dry_run }`. This is a shared-key model: one Anthropic key gets pushed into every agent that syncs against `anthropic`.

For per-agent `$` attribution to mean anything, a provider needs to support **multiple labeled keys**, each optionally tied to one agent:

```rust
pub struct ProviderKeySpec {
    pub provider: String,       // "anthropic"
    pub label: String,          // "claude", "aider" — user-chosen, defaults to "default"
    pub agent: Option<String>,  // which agent this key is for, if any
    pub env_var_name: String,   // inherited from the provider preset
    pub secret_name: String,    // composite: "{provider}:{label}", stored via SecretStore
}
```

- `single provider set-key <name> <value>` keeps working unchanged — it's sugar for `add-key <name> --label default <value>`.
- New: `single provider add-key <name> --label <label> [--agent <agent>] <value>`.
- New: `single provider list-keys <name>` — lists labels/agent-tags for a provider (never the secret value).
- `single provider sync` gains an optional `--label <label>` to sync a *specific* key to a specific agent, instead of the same shared key to every agent in `--agents`.

This is additive to the existing `ProviderSpec`/`ProviderKeySpec` split — `ProviderSpec` stays as the provider *preset* (name, env var, base URL); `ProviderKeySpec` is the new per-key layer underneath it. Existing single-key users see no behavior change.

### 1.2 Billing provider registry

New `single_core::billing` module, structurally parallel to `registry::AgentDefinition`:

```rust
pub struct BillingProviderDefinition {
    pub provider: String,             // matches ProviderSpec::name
    pub verified: bool,               // has this endpoint actually been called successfully, or is it doc-sourced only
    pub admin_key_env_hint: String,   // e.g. "ANTHROPIC_ADMIN_KEY" — documents what kind of key is needed, distinct from the inference key
    pub notes: Option<String>,
}

pub trait BillingProvider {
    fn provider(&self) -> &str;
    fn fetch_usage(&self, admin_key: &str, since: DateTime<Utc>) -> Result<Vec<UsageRecord>>;
}

pub struct UsageRecord {
    pub provider: String,
    pub key_label: Option<String>,   // matched against ProviderKeySpec::label where the billing API exposes per-key breakdown
    pub model: Option<String>,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
```

v1 scope: **anthropic, openai, xai, openrouter**. Each gets implemented and marked `verified: true` only after an actual successful call against that provider's real usage endpoint during implementation — matching how `registry.rs`'s existing entries document "confirmed via direct execution" vs. doc-sourced. If a provider's usage API turns out not to support per-key breakdown (plausible — this needs real verification, not assumption), `UsageRecord::key_label` is `None` and the Usage page shows it as an undifferentiated provider total instead of guessing an attribution.

The **admin/billing key** (org-scoped, read-only usage access) is a separate credential from any `ProviderKeySpec` inference key. Stored via the same `SecretStore` under a `billing:<provider>` namespace. `single provider set-billing-key <provider> <value>` (new command).

### 1.3 Protocol and aggregation

```rust
Request::UsageShow { provider: Option<String> },
Request::UsageRefresh,   // force a live re-fetch, bypassing cache
```

```rust
pub struct UsageSummary {
    pub provider_usage: Vec<ProviderUsageRow>,   // one row per (provider, key_label)
    pub agent_local_stats: Vec<AgentLocalStats>, // OAuth / no-billing-data agents
    pub total_usd: f64,
    pub last_refreshed: String,
}
pub struct AgentLocalStats {
    pub agent: String,
    pub run_count: u64,
    pub avg_duration_ms: u64,
    pub last_run_at: Option<String>,
}
```

`agent_local_stats` groups the existing `TaskRecord` table by `agent` — `run_count` from `COUNT(*)`, `avg_duration_ms` from `updated_at - created_at`, `last_run_at` from `MAX(updated_at)`. No schema change needed.

`provider_usage` is a short-TTL cache (a few minutes) over live `BillingProvider::fetch_usage` calls, refreshed on TUI tab entry or `single usage refresh` — avoids hammering billing APIs on every TUI redraw.

### 1.4 Surface

- **TUI**: `Tab::Usage`. Two tables: "Provider spend" (provider, key label / agent tag, `$` this period, tokens, a totals row) and "Connected agents" (agent, runs, avg duration, last run) for agents with no billing data. A refresh hint if the cache is stale.
- **CLI**: `single usage show [--provider <name>] [--json]`, `single usage refresh`.

---

## Part 2: Backup page

### 2.1 What gets captured

A full walk of `SingleDirs` root (`~/.config/single/` or `$SINGLE_CONFIG_DIR`), per user decision — **everything**, not just config:

- `config.toml`, `profiles/`, `agents/`
- Flat registries: `mcp.toml`, `mcp_gateway.toml`, `lsp.toml`, `tools.toml`, `docker.toml`, `hooks.toml`, `permissions.toml`, `providers.toml`, `plugins.toml`
- `skills/`
- `accounts/` (captured claude/codex/agy credential snapshots)
- `homes/<agent>/**` — every agent's isolated `$HOME`, i.e. every real OAuth credential from `single agent login`
- `state/single.db` (full task/memory/notes/knowledge-graph history), `state/artifacts/`, `state/documents/`
- **Every keychain secret**: `SecretStore::list()` enumerated, each value fetched via `SecretStore::get()` and written into the archive's manifest — this is the one category of data that isn't already a file, since `single secret set`/`single provider (add-)key`/the new billing keys all live in the OS keychain, not on disk.

Directories that don't exist yet (most of `SingleDirs` besides `profiles/agents/state/logs` are created lazily on first use — see `SingleDirs::ensure_created`) are simply skipped, not errored on.

Explicitly **not** captured: `state/runtime.sock` (a live socket, meaningless once copied), log files under `logs/` (operational noise, not setup).

### 2.2 Archive format and encryption

- Pack into a tar (via the `tar` crate — new workspace dependency).
- Encrypt with a passphrase using the `age` crate's passphrase mode (new workspace dependency; native Rust, no external binary — deliberately *not* shelling out to a system `age`/`gpg` binary the way `secret-tool` is shelled out to, since this session already hit real pain from `secret-tool` being absent on this machine and a self-contained crate avoids repeating that failure mode for backup specifically).
- Output: a single `.age`-encrypted file, e.g. `singlecli-backup-2026-08-15.tar.age`.
- Passphrase is prompted interactively (via a hidden-input terminal read) for both export and import — **never** accepted as a CLI argument, to avoid it landing in shell history or `ps` output.

### 2.3 Restore

- Decrypt (passphrase prompt), unpack the tar.
- Every file gets written through the **same `backup_before_write` helper** every other config writer already uses — importing onto an existing setup backs up what's there first rather than silently overwriting. Importing onto a fresh machine (the primary use case) just populates cleanly, since there's nothing to collide with.
- Every captured secret gets re-inserted into the *target* machine's keychain via `SecretStore::set()`. If `secret-tool` isn't installed on the target, each failed secret is reported individually (`"failed to restore secret X: secret-tool not found"`) rather than aborting the whole import — matches how `single doctor`/`single setup` already report per-item rather than all-or-nothing.
- `single backup import <path>` requires `--yes` to actually write (dry-run by default, prints what would be restored) — same confirmation pattern as `Setup`/`UninstallIntegrations`.

### 2.4 Protocol and surface

```rust
Request::BackupExport { dest_path: String },          // passphrase handled out-of-band, see below
Request::BackupImport { src_path: String, dry_run: bool },
```

Passphrase handling needs care: unlike `SecretSet { value }` (an already-precedented plain-`String`-over-the-socket field for a value the caller explicitly wants stored), a backup passphrase is deliberately *not* meant to be persisted or logged anywhere, and the runtime's event log (`single-runtime`'s audit trail, referenced in `mcp.rs`'s docs re: `SecretGet`) must not capture it. Resolution: the passphrase is read directly by whichever process needs it (the CLI process for `single backup export/import`, or the TUI process for the Backup tab) and passed to the archive/encryption code **in-process**, not over the socket — `install_all`/`uninstall_all`-style requests that need real filesystem+keychain access on the machine running the command anyway, so this doesn't need daemon involvement the way registry CRUD does. This mirrors `client.rs`'s existing in-process-fallback path, just used deliberately here instead of only as a fallback.

- **TUI**: `Tab::Backup`. Shows last-backup info if tracked (a small marker file, not required for correctness), an export form (destination path + passphrase prompt), an import form (source path + passphrase prompt + a dry-run preview before requiring confirmation), and a results pane listing per-item success/fail after running.
- **CLI**: `single backup export <path>`, `single backup import <path> [--yes]`.

---

## Testing

- `provider_keys`: round-trip add-key/list-keys/sync with multiple labels per provider; existing single-key tests must keep passing unchanged.
- `billing`: unit tests per provider against a mocked HTTP response (not live API calls in CI) for parsing; each provider's real endpoint gets one manual "confirmed via direct execution" verification pass during implementation, documented in the registry entry the same way `registry.rs` already documents agent verification.
- `backup`: round-trip export→import into a temp `SingleDirs` root, asserting every file and every secret comes back byte-identical; a corrupted/wrong-passphrase import must fail cleanly, not partially apply.
- TUI: existing tab-rendering test patterns (if any) extended to the two new tabs; otherwise manual verification via `cargo run`, consistent with how other tabs were validated.

## Open risks (explicitly not resolved by this doc — flagged for implementation time)

- Whether anthropic/openai/xai/openrouter's usage APIs actually support per-key-label breakdown at all is **unverified** — this doc's data model assumes it's possible where available and degrades to a provider-level total where it isn't. Confirming this per-provider is implementation work, not something to guess here.
- `age` crate's exact passphrase-mode API surface hasn't been checked against this project's MSRV/dependency constraints yet — a quick compile-check during implementation, not expected to block the design.
