# Custom/local providers + OmniRoute integration — design

## Status

Proposed — awaiting user's go-ahead to implement. Do NOT start
implementation until explicitly told; do NOT deploy OmniRoute until
explicitly told.

## Context

Two requests, which turn out to be one feature:

1. Let a user add local/custom LLM providers "like normal providers,"
   with a `custom` type.
2. Deploy [OmniRoute](https://github.com/diegosouzapw/OmniRoute) (a
   real, MIT-licensed, self-hosted AI gateway — one OpenAI-compatible
   endpoint in front of 90+ free-tier providers, confirmed via its own
   docs) on this machine — not inside `SingleCLI` or `The-Company` —
   and register it into SingleCLI to use its pooled free-tier tokens.

Investigation into SingleCLI's current provider system found:

- **`ProviderSpec` (`single-protocol/src/lib.rs:1330`) is already
  fully generic**: `{ name, env_var_name, secret_name, base_url }`, no
  vendor enum. `single provider add <name> --env-var <VAR> --base-url
  <url>` (`single-cli/src/main.rs:849`) already registers **any**
  provider, local or remote — this already works today, no code
  needed.
- **Env-var injection into a spawned agent's process already works
  universally.** `resolve_env_for_agent`
  (`single-core/src/provider_keys.rs:105`) injects a provider's env var
  into any agent's subprocess environment at spawn time, once a labeled
  key is bound via `provider add-key <name> --agent <agent> <value>`.
  This is agent-agnostic by construction — it doesn't touch any
  agent's own config file.
- **The actual gap**: nothing tells an agent's own model picker that a
  newly-added provider exists. Confirmed via
  `single-agent-sdk/src/provider_sync.rs`: file-based config sync
  (`provider sync`) only writes `claude`'s `settings.json` and
  `codex`'s `auth.json` — every other agent, including **opencode**
  (the one agent that worked reliably across two full sessions), is
  explicitly `unsupported`, because its provider/model config isn't a
  documented single-file surface `provider_sync.rs` currently touches.
  Getting an env var into opencode's process (already works) is not
  the same as opencode knowing there's a model to pick (does not work
  today).
- **opencode's real custom-provider config format**, confirmed via
  opencode's own docs (not guessed):
  ```jsonc
  {
    "provider": {
      "<provider-id>": {
        "npm": "@ai-sdk/openai-compatible",
        "name": "<Display Name>",
        "options": {
          "baseURL": "<base_url>",
          "apiKey": "{env:<ENV_VAR_NAME>}"
        },
        "models": {
          "<model-id>": { "name": "<Model Display Name>" }
        }
      }
    }
  }
  ```
  `formats/opencode.rs` already has this exact pattern for two other
  keys (`mcp`, `lsp`) — read-merge-write into `opencode.jsonc`,
  backed up before writing (`backup_before_write`).

So: no bespoke OmniRoute integration is needed. Once opencode can learn
about a custom provider, OmniRoute is just another entry through the
same path.

## Design

### 1. `ProviderSpec` gains a `models` field

```rust
// single-protocol/src/lib.rs
pub struct ProviderSpec {
    pub name: String,
    pub env_var_name: String,
    pub secret_name: String,
    pub base_url: Option<String>,
    pub models: Vec<ModelSpec>,   // NEW, defaults to empty (existing presets unaffected)
}

pub struct ModelSpec {
    pub id: String,     // e.g. "auto"
    pub name: String,   // e.g. "Auto (best available)"
}
```

CLI: `single provider add <name> --env-var <VAR> --base-url <url>
--model <id>:<name> [--model <id>:<name> ...]` (repeatable flag).
Existing presets (openai/anthropic/opencode-zen/nvidia) get an empty
`models` list by default — harmless, since the new writer below only
acts when the list is non-empty.

### 2. New opencode provider writer

`single-agent-sdk/src/formats/opencode.rs` gains a function alongside
its existing `mcp`/`lsp` writers:

```rust
pub fn write_provider(path: &Path, provider: &ProviderSpec, dry_run: bool) -> Result<ProviderSyncResult>
```

Read-merge-write into `opencode.jsonc`'s `provider.<name>` key, in the
confirmed format above, backed up before writing (same
`backup_before_write` convention every other writer in this file
uses). No-op (returns `unsupported`, matching the existing pattern in
`provider_sync.rs`) if `provider.models` is empty — a provider with no
declared models has nothing useful to add to opencode's picker.

### 3. Wire it into `provider sync`

`single-agent-sdk/src/provider_sync.rs`'s `sync()` match gains an arm:

```rust
"opencode" => sync_opencode_provider(home, provider, dry_run),
```

taking the full `ProviderSpec` (not just `env_var_name`/`value` like
the other arms) since it needs `base_url` and `models` too — the
signature of `sync()` needs to accept the whole spec rather than just
the resolved env var name/value; check call sites in
`handlers.rs:965-1004` and `:1085` to pass `provider` through instead
of destructuring it early. This is the one real signature change in
this design — everything else is additive.

### 4. The "custom type" question

Today, a preset-derived provider and a manually-added one are
structurally identical once in the registry — `AddPreset` just
pre-fills known values before calling the same `add`. **Decision made
without you being here to confirm** (flagging for override): don't add
an artificial `kind: Preset | Custom` enum distinguishing them — it
would be a label with no behavioral difference, and `presets()`'s
fixed list already answers "is this built-in" for anything that needs
to check. If you actually want the visible distinction in `provider
list` output, say so and it's a small addition (`kind` field,
defaulted `Custom`, set `Preset` in `sync_missing_presets`).

### 5. OmniRoute deployment

Per its own docs: `npm install -g omniroute` (Node, no Docker needed —
confirmed lightweight enough for a systemd user service), listens on
`localhost:20128/v1`, OpenAI-compatible, ignores the API key value by
default. Deliberately **not** inside `nbr-vault` or `The-Company` (per
instruction) — lives wherever global npm packages land
(`~/.local/share/npm`-equivalent or `npm config get prefix`), service
managed via a new `~/.config/systemd/user/omniroute.service` unit
(`ExecStart=omniroute serve` or equivalent — confirm exact serve
command from `omniroute --help` at implementation time, not guessed
here), enabled so it survives reboot.

Registration is then pure usage of the mechanism above, zero
OmniRoute-specific code:

```
single provider add omniroute \
  --env-var OMNIROUTE_API_KEY \
  --base-url http://localhost:20128/v1 \
  --model auto:"Auto (best available)"
single provider set-key omniroute placeholder   # OmniRoute ignores the value
single provider add-key omniroute --agent opencode placeholder
single provider sync omniroute --agents opencode --yes
```

After this, opencode's model picker shows `omniroute/auto` backed by
OmniRoute's full free-tier pool, no separate integration surface.

## Testing

New tests: `formats/opencode.rs::write_provider` (merges correctly
into an existing `opencode.jsonc`, doesn't clobber other `provider`
entries, backs up first — same test shape as the existing `mcp`/`lsp`
writer tests in that file). `provider_sync.rs`'s new `"opencode"` arm
gets the same `unsupported`-when-empty-models test as the pattern
elsewhere in that file.

## Rollout

Additive except the one `sync()` signature change noted in §3 (internal
function, not a public tool schema — no external caller breaks).
Workspace `0.6.0` → `0.7.0` alongside the orchestration-lessons change
if both ship together, or its own `0.7.0`/`0.8.0` if shipped separately
— sequencing is the user's call, not decided here.

## Open questions for user review

1. Do you want the `kind: Preset | Custom` label added to `provider
   list` output, or is the current behavior (structurally identical,
   distinguishable only via the fixed `presets()` list) fine? (§4)
2. Confirm OmniRoute's exact long-running serve command and any
   env/config choices (does the free zero-config `auto` model meet the
   bar, or do you want specific provider API keys registered inside
   OmniRoute itself for higher-quality free tiers?) before the systemd
   unit is written.
