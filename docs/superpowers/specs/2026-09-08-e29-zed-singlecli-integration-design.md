# E29 — Zed + SingleCLI deep integration: design

Status: approved. Builds on E27 (`single acp`, Coordinator) and E28
(free-provider pool, `single-pool` agent, auto-continue, self-heal).

## Goals

1. Live redaction of secrets in every prompt reaching an LLM/agent through
   SingleCLI, regardless of entry point (Zed ACP, direct CLI, OpenAI-compat
   server).
2. `single-pool` as the default agent for Zed ACP sessions, overridable.
3. A Zed-visible status surface for tasks/goals and provider auth/exhaustion
   state, within what Zed's extension API actually supports today.
4. Cross-session/cross-prompt dedup: don't start a duplicate goal for an
   ask that's already in flight; queue distinct asks.
5. Bring existing Zed agent-mode config in this repo/dotfiles in line with
   the E28 architecture (single-pool, auto-continue, self-heal).

## Non-goals

- LLM-based secondary redaction classifier — heuristic-only for v1,
  documented as a v2 follow-up (§Deferred).
- A true Zed status-bar/taskbar icon — confirmed unavailable in Zed's
  current extension API (see §Research: Zed capabilities). Not attempted.
- Push-updated UI — Zed's ACP surfaces no notification channel richer than
  plain-text session chunks; the status surface is pull/refresh-on-invoke.
- Changing `CoordinatorConfig.prefer_pool`'s global default or its existing
  call sites — untouched; ACP's default-agent behavior is layered on top
  via the existing `GoalSubmit.agent` field instead.

## Research: Zed capabilities (locked findings)

- No status-bar/taskbar/icon API ships today. RFC #53403 (status-item
  record: text/icon/tooltip/on-click via `add-item`) is unshipped/in
  discussion. Discussion #59656 (AI-credit/quota status display) is open,
  unresolved. Confirmed not possible in the current released API.
- Extensions can: language support (LSPs), themes, snippets, **slash
  commands** (`extension.toml`-registered, return `SlashCommandOutput`
  with `text` + optional `SlashCommandOutputSections` rendered as creases
  in the Agent Panel), and **context servers** (MCP), gated by a
  capability system (`process:exec`, `npm:install`, `download_file`).
- No general sidebar/dock panel API for extensions.
- ACP: `session/update` streams `agent_message_chunk`/`agent_thought_chunk`
  plain text only — no richer status channel.
- Conclusion / chosen fallback: a `/single-status` slash command added to
  `single acp`'s `commands()` list, returning structured
  `SlashCommandOutputSections` text rendered in the Agent Panel. Not a
  persistent indicator — refreshed each time the user invokes it.

Sources: github.com/zed-industries/zed discussions #53403, #59656;
zed.dev/docs/extensions/{capabilities,slash-commands}; docs.rs/zed_extension_api.

## Decisions locked

- **Redaction bias**: over-redact. Heuristic pass only for v1 (known
  vendor prefixes — `sk-`, `ghp_`, `AKIA`, JWT shape, etc. — plus generic
  high-entropy string detection and `KEY=`/`PASSWORD=`-shaped assignment
  patterns), modeled on gitleaks/trufflehog's public pattern sets. A false
  positive costs the user an "un-redact" click; a miss costs a leaked key.
- **Alias store**: new TTL'd table riding `single_core::secrets`'
  encryption primitive (not a literal extension of its existing
  get/set/list/delete API — new schema), `session_id`-scoped, 2-4h TTL,
  survives a daemon restart mid-session.
- **Default agent for ACP**: not a global `prefer_pool` flip.
  `GoalSubmit.agent: Option<String>` (already exists, currently always
  `None` from `acp.rs`) gets populated with `Some("single-pool")` by
  `acp.rs` unless the session/request specifies otherwise.
- **Dedup**: literal/fuzzy text-overlap check against `goal::active()`'s
  `Goal.text`, not semantic/LLM-based, for v1.

## Architecture

### Redaction

One new module, `single_core::redact`:

```
pub struct PendingAlias {
    pub alias: String,       // "{{REDACTED_1}}" etc, unique per session
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ciphertext: Vec<u8>,  // encrypted real value, via secrets' cipher
}

pub fn scan_and_replace(text: &str, session_id: &str, store: &RedactStore)
    -> Result<(String, Vec<PendingAlias>)>;

pub fn resolve(text: &str, session_id: &str, store: &RedactStore)
    -> Result<String>; // substitutes aliases back to plaintext
```

Three call sites, all pre-`GoalSubmit`/pre-dispatch:

- `crates/single-cli/src/acp.rs:127` — right after `prompt_text(&p)`,
  before `run_turn` is invoked (covers slash-command args too).
- `crates/single-cli/src/serve_openai.rs:273` — right after
  `flatten_messages()`.
- `crates/single-cli/src/main.rs` (~2767, ~2823) — the two `task run`
  goal-submission sites.

`resolve()` is called once, at the outbound HTTP boundary in
`single-runtime`'s `pool_agent.rs`/`client.rs` (E28's pool dispatch path),
substituting aliases back to plaintext only in the request body sent to
the provider. Goal text, event history, and anything logged or persisted
keeps the redacted form — this is the whole point.

Promotion: a new `single secret promote-alias <alias> <name>` CLI command
(and an ACP-reachable equivalent) moves a `PendingAlias`'s decrypted value
into a real `single_core::secrets` entry under `<name>`, then deletes the
alias row. Requires explicit user confirmation at the call site (ACP: a
confirmation round-trip in the agent's response; CLI: a y/n prompt) — the
daemon never auto-promotes.

### Default agent (ACP)

`acp.rs`'s `run_turn`, at the `GoalSubmit` call (~line 300): `agent: None`
becomes `agent: session.agent_override.clone().or(Some("single-pool".into()))`.
`session.agent_override` is a new optional field on the ACP session state,
settable via a `/agent <name>` slash command for the rest of that session,
or per-request via an (already-supported-elsewhere-in-ACP-conventions)
inline directive — scope this to session-level only for v1, per-request
override is a documented v2 follow-up if requested.

### Status slash command

`/single-status` registered in `single acp`'s `commands()`. Handler calls
existing `coordinator::status()`, `pool::status()`, and
`provider::key_status()` (all already implemented, per E28) and formats
into `SlashCommandOutputSections`: one section for active/queued goals,
one for per-provider auth/cooldown state, one rough usage-estimate line
computed from the ledger's already-tracked 4-D quota consumption. No new
backend logic — formatting/wiring only.

### Cross-session dedup

New `goal::find_overlapping(text: &str) -> Option<Goal>` in
`crates/single-runtime/src/coordinator/goal.rs`, called from the same
`GoalSubmit` handler path (all three entry points) before creating a new
goal. v1 overlap test: normalized (lowercased, whitespace-collapsed)
substring/Jaccard-token-overlap above a fixed threshold against
`active()`'s `Goal.text` values. On a hit: return the existing goal's
`id`/status instead of submitting a new one, with a short "already in
flight, goal <id> — <status>" response surfaced back through whichever
entry point asked (ACP text reply, CLI stdout, OpenAI-compat JSON). No
change to how distinct, non-overlapping goals queue — E28's scheduler and
`waiting_on_capacity` machinery already does that.

### Zed agent-mode config QoL pass

Audit whatever `.zed/settings.json` / agent-mode config exists in this
repo and the user's dotfiles; update default agent references to
`single-pool`, confirm auto-continue/self-heal-aware settings are
reflected (e.g. don't configure a mode that assumes synchronous
completion when goals can legitimately pause on `waiting_on_capacity`).

## Data model (additive only)

New table (name TBD at implementation, e.g. `redact_aliases`):
`session_id`, `alias` (PK together with session_id), `ciphertext`,
`created_at`, `expires_at`. Expiry swept lazily (checked on
read/resolve) plus an opportunistic sweep on daemon startup, matching
existing `secrets` module patterns — no new background timer thread.

No changes to `goals` table schema — `find_overlapping` is a read-time
query against existing columns.

## Testing

- Unit: `redact::scan_and_replace` against a corpus of real-shaped (not
  real) secrets — vendor-prefixed keys, JWTs, generic high-entropy
  strings, `KEY=`/`PASSWORD=` assignments — plus a negative corpus
  (UUIDs, git SHAs, normal prose) to bound false-positive rate
  qualitatively (documented, not gated on a numeric threshold for v1).
- Unit: `resolve()` round-trips a redacted string back to original given
  a live alias; expired alias resolves to a clear error, not silent
  passthrough of the alias token.
- Unit: `goal::find_overlapping` against synthetic active-goal sets:
  exact match, near-duplicate phrasing, and genuinely distinct asks.
- Integration (`#[ignore]`-gated where it needs the real daemon): ACP
  session sends a prompt containing a fake-shaped secret → confirm the
  goal record and event history contain only the alias, never the
  plaintext.
- Live verification (per task constraints): real daemon restart with a
  populated alias table (TTL survives restart), a real `/single-status`
  invocation from an actual Zed session if feasible, cross-session dedup
  by asking for the same thing twice from two different sessions.

## Build order

1. `single_core::redact` module + alias store schema + unit tests.
2. Wire `scan_and_replace` into the three entry points; wire `resolve`
   into the pool dispatch boundary.
3. `single secret promote-alias` command.
4. ACP default-agent (`agent_override` + `single-pool` default).
5. `goal::find_overlapping` + wiring into the three entry points.
6. `/single-status` slash command.
7. Zed agent-mode config QoL pass.
8. Live verification pass (real daemon, real Zed session where feasible).

## Deferred (documented, not built here)

- LLM-based second-pass redaction classifier.
- Per-request (not just per-session) ACP agent override.
- A real Zed status-bar icon, if/when RFC #53403 or discussion #59656
  ship in a future Zed release.
- Numeric false-positive-rate gating for the redaction heuristic (v1
  ships with qualitative corpus testing only).
