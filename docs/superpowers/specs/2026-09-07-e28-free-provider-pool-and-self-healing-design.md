# E28 — Free-provider pool, adaptive routing, and self-healing autonomy

**Status:** design spec, draft for review · **Date:** 2026-09-07
**Target:** SingleCLI v0.11 · **Repo:** `naviNBRuas/SingleCLI`
**Builds on:** E27 (v0.10.0 — Coordinator subsystem, `single acp`, `single serve --openai`,
per-task token accounting, `single loop`). This spec assumes that foundation.
**Reference (studied, not vendored):** `github.com/tashfeenahmed/freellmapi` —
its provider catalog (`shared/types.ts` `Platform` union, `server/src/providers/`)
and routing/quota engine (`docs/en/architecture/01–06`, `docs/en/providers/02`)
are the design source for Parts A–B. **No code, no runtime dependency is taken
from it** — this is a clean-room native-Rust port of the ideas.

---

## 1. Goal

1. **Absorb the free-LLM-provider landscape.** Register the ~40 free-tier
   providers freellmapi supports as first-class SingleCLI providers, each
   with its real rate-limit metadata, base URL, auth quirks, and free
   roster — so the pool has genuine breadth.
2. **A native pooled agent — `single-pool`.** A first-class SingleCLI agent
   that calls provider HTTP APIs directly (OpenAI-compatible + a few native
   wires) through an adaptive routing/quota engine, usable as
   `single task run --agent single-pool` and as any coordinator node's
   agent. It is the Rust equivalent of freellmapi's router, minus the HTTP
   proxy surface (E27's `single serve --openai` already fills that).
3. **Never lose work to exhaustion — "auto-continue".** When every eligible
   provider/agent is rate-limited or out of daily quota, a goal does not
   fail. It enters `waiting_on_capacity`, and the coordinator resumes it
   automatically as cooldowns expire and daily windows reset — feeding the
   resumed node a context-handoff message so quality does not degrade
   across the pause or across a mid-task model switch.
4. **Self-heal and self-fix.** The daemon detects and repairs its own
   broken state (stale sockets, zombie rows, poisoned locks, corrupt
   config, dead agent binaries, DB corruption), the coordinator corrects
   its own failing goals and routing config, and it self-installs / repairs
   missing agent CLIs and their auth when a node needs one that is not
   ready.
5. **Self-healing, self-resuming sessions.** A coordinator/ACP session
   survives a daemon crash or restart: in-flight goals are re-planned or
   re-dispatched from where they stopped, not marked failed; `session/load`
   replays and rebinds transparently.

## 2. Non-goals

- **No HTTP proxy re-implementation.** freellmapi's `/v1/chat/completions`
  surface, dashboard, media endpoints, Anthropic/Gemini translation layers,
  60-language UI — out of scope. `single serve --openai` (E27) is the
  HTTP surface; `single-pool` is an internal agent.
- **No Node.js / npm dependency.** Native Rust only. No second daemon.
- **No live signed-catalog feed** (this iteration). Provider metadata is a
  vendored static snapshot in the Rust source, bumped by hand on release.
  A signed-feed loader is a documented follow-up.
- **No paid/frontier models.** Free tiers only, matching freellmapi's
  no-card criterion.
- **No new LSPs.** freellmapi ships none; nothing to import.
- **`n>1`, `/v1/moderations`, multi-tenant billing** — out.

## 3. Decisions locked (2026-09-07)

| # | Decision |
|---|---|
| D1 | **New native `single-pool` agent + all ~40 providers as `ProviderSpec` presets.** The presets also flow into existing CLI agents' configs via `install-integrations` (using the E27 `single-<name>` aliasing where a models.dev collision exists). |
| D2 | **Native Rust only.** No freellmapi runtime, no Node. |
| D3 | **Vendored static provider snapshot**, manual bumps. A `providers/free_pool.rs` (or a bundled `free-pool.toml`) holds the table; `single provider sync-pool` reconciles it into the registry. |
| D4 | **Self-fix scope = infra + coordinator self-correction + agent self-install/repair.** The broadest option: daemon/session/config repair, autonomous routing/coordinator config correction, and auto-install/auth-repair of missing agents. Every autonomous mutation is logged as a `self_heal_events` row and is individually disable-able. |
| D5 | Everything sits **on top of the E27 coordinator** — new goal states, new tick behaviour, new tables, no rewrite. `single task run` / `orchestrate-*` / existing agents keep working unchanged. |
| D6 | Version bump to **0.11.0** (new agent, new subsystem, new subcommands — minor, pre-1.0). Single `## [0.11.0]` CHANGELOG block. Additive tables only. |

---

## 4. Architecture

```
                         ┌─────────────────────────────────────────────┐
                         │            single-runtimed (daemon)         │
   single task run ─────▶│                                             │
   single goal submit ──▶│   Coordinator (E27) + new states:           │
   single acp ──────────▶│     waiting_on_capacity, re-plan-on-restart │
                         │                                             │
                         │   ┌───────────────── pool engine (E28) ──┐  │
   task::run(agent=      │   │  routing::bandit  — Thompson scoring  │  │
     "single-pool") ────▶│──▶│  quota::ledger    — RPM/RPD/TPM/TPD   │  │
                         │   │  quota::cooldown  — ladder + probe    │  │
                         │   │  quota::pools     — provider-wide     │  │
                         │   │  degrade::state   — healthy-ratio SM  │  │
                         │   │  handoff::inject  — context handoff   │  │
                         │   └───────────┬──────────────────────────┘  │
                         │               │ dispatch (agent=single-pool)│
                         │   pool::client — HTTP to a chosen           │
                         │     (provider, model, key); OpenAI-compat   │
                         │     + native wires (google/cohere/…)        │
                         │                                             │
                         │   self_heal::pass  — infra + coordinator +  │
                         │     agent self-install (periodic + on-demand)│
                         └──────────────────┬──────────────────────────┘
                                            ▼
          Google · Groq · Cerebras · OpenRouter · NVIDIA NIM · … 35 more
          (provider HTTP APIs; keys in ~/.config/single, encrypted)
```

**Crate placement:**
- `single-core/src/free_pool.rs` — the vendored provider catalog + metadata types (pure data + parsers).
- `single-core/src/pool_keys.rs` — per-provider API-key storage (reuse `provider_keys.rs` / `secret` machinery; keys encrypted at rest as today).
- `single-runtime/src/pool/` — new module tree:
  `mod.rs` · `client.rs` (HTTP dispatch, OpenAI-compat + native wires) ·
  `ledger.rs` (4-D windows, leases, SQLite) · `cooldown.rs` (ladder, probe job) ·
  `pools.rs` (provider-wide pool keys) · `bandit.rs` (Beta posteriors, scoring, strategy weights) ·
  `degrade.rs` (state machine) · `handoff.rs` (context-handoff message builder).
- `single-runtime/src/pool_agent.rs` — the `single-pool` adapter: implements the
  agent-run contract by calling `pool::` instead of shelling a CLI.
- `single-runtime/src/self_heal.rs` — the self-heal pass + `self_heal_events`.
- `single-runtime/src/coordinator/` — extended (states, restart re-plan, capacity wait).
- `single-cli/src/main.rs` — new subcommands.

---

## 5. Part A — Provider catalog (~40 free providers)

### 5.1 Metadata type — `free_pool.rs`

```rust
pub struct FreeProvider {
    pub id: &'static str,              // "groq", "openrouter", "sea-lion", …
    pub display: &'static str,
    pub base_url: &'static str,        // "" for native-wire providers
    pub wire: Wire,                    // OpenAiCompat | Gemini | Cohere | Cloudflare | Zhipu | AiHorde | ...
    pub auth: Auth,                    // Bearer | XApiKey | Compound("account_id:token") | Keyless(sentinel) | Header(name)
    pub signup_url: &'static str,
    pub limits: Limits,               // { rpm, rpd, tpm, tpd: Option<u64> }  — None = unknown → 10-min cooldown ceiling
    pub pool: Option<PoolShape>,      // provider-wide pool: Free | Project | Account | CreditPool { rpm } | DailyTokens { n }
    pub timeout: Duration,            // per-provider (google 60s, nvidia 180s, ollama/custom 120s, aihorde 120s, else 30s)
    pub quirks: Quirks,               // { force_single_tool_call, no_tools, no_stream, browser_ua, min_max_tokens,
                                      //   validate_url: Option<&str>, validate_cache: Option<Duration>, region_wall, real_name_auth }
    pub free_note: &'static str,      // human note ("$0.10/mo router credit", "5M tok/day account-wide", …)
}
```

### 5.2 The vendored table (snapshot from freellmapi 2026-09, verified against `providers/index.ts`)

| id | base_url (or native) | auth | key limits / pool | quirks |
|---|---|---|---|---|
| `google` | native Gemini | Bearer | per-project pool | 60s timeout |
| `groq` | api.groq.com/openai/v1 | Bearer | account pool; some models rpd 1000 / tpm 8000 | — |
| `cerebras` | api.cerebras.ai/v1 | Bearer | — | — |
| `nvidia` | integrate.api.nvidia.com/v1 | Bearer | credit pool ~40 rpm account-wide | `force_single_tool_call`, 180s |
| `mistral` | api.mistral.ai/v1 | Bearer | — | — |
| `openrouter` | openrouter.ai/api/v1 | Bearer | `:free` pool 1000/day (50/day if <10 credits) | `HTTP-Referer`+`X-Title` headers, `:free` suffix |
| `github-models` | models.github.ai/inference | Bearer | — | `<publisher>/<model>` ids |
| `cohere` | native (OpenAI-compat endpoint) | Bearer | — | — |
| `cloudflare` | native (Workers AI) | Compound `account_id:token` | — | — |
| `zhipu` | native (z.ai / bigmodel.cn) | Bearer | — | domestic→global host re-probe, 60s |
| `ollama-cloud` | ollama.com/v1 | Bearer | — | 120s, reasoning in `message.reasoning` |
| `kilo` | api.kilo.ai/api/gateway/v1 | Keyless | 200 req/hr per IP | `validate_url=/api/gateway/models`, prompts logged for training |
| `pollinations` | native | Bearer | — | validate `/account/key` (public `/v1/models` lies) |
| `llm7` | api.llm7.io/v1 | Bearer (anon works for basic) | 100 req/hr | — |
| `huggingface` | router.huggingface.co/v1 | Bearer | $0.10/mo router credit | — |
| `opencode-zen` | opencode.ai/zen/v1 | Bearer | trial-only promo roster | — |
| `ovh` | oai.endpoints.kepler.ai.cloud.ovh.net/v1 | Keyless | 2 req/min per IP per model | — |
| `agnes` | apihub.agnes-ai.com/v1 | Bearer | ~30 concurrent → 429 | 60s |
| `reka` | api.reka.ai/v1 | Bearer | recurring monthly credit, no card | — |
| `siliconflow` | api.siliconflow.com/v1 | Bearer | free generative-media (FLUX.1-schnell, CosyVoice2) | media only |
| `routeway` | api.routeway.ai/v1 | Bearer | ~5 rpm observed (doc says 20/200) | `browser_ua` (Cloudflare 1010) |
| `bazaarlink` | bazaarlink.ai/api/v1 | Bearer | `auto:free` route only | — |
| `ainative` | api.ainative.studio/api/v1 | Bearer | ~10M tok/mo (unverified) | — |
| `aion` | api.aionlabs.ai/v1 | Bearer | no-card | 30-day age gate |
| `requesty` | router.requesty.ai/v1 | Bearer | shared free pool | — |
| `navyai` | api.navy/v1 | Bearer | 150K tok/day, 20 rpm | needs explicit `User-Agent` |
| `nara` | router.bynara.id/v1 | Bearer | shared free pool | Telegram channel verification |
| `sea-lion` | api.sea-lion.ai/v1 | Bearer | 10 rpm recurring | Google sign-in, no card, no region wall |
| `orcarouter` | api.orcarouter.ai/v1 | Bearer | shared `$0` pool, unpublished limits | 429 is a clean quota signal |
| `unorouter` | api.unorouter.com/v1 | Bearer | account-wide per-minute `:free` pool | `:free` suffix |
| `xkiro` | api.xkiro.com/v1 | Bearer or `x-api-key` | 5M tok/day account-wide | validate `/v1/usage` |
| `radeon` | developer.amd.com.cn/radeon/api/v1 | Bearer | rotating public roster, header-reported limits | `no` parallel tools, 10-min gen window |
| `bai` | api.b.ai/v1 | Bearer | limited-time 0-credit promo | — |
| `anyapi` | api.anyapi.ai/v1 | Bearer | 100K tok/day, no published rpm | — |
| `electronhub` | native (OpenAI-compat subclass) | Bearer | shared weekly credit pool | validate `/v1/user/me` |
| `experiential` | native (OpenAI-compat subclass) | Bearer | shared monthly credit pool | authed `/v1/models` |
| `sail` | native (Responses API, background poll) | Bearer + payment method | $5/mo credit then pay-go | flex-only models |
| `modelscope` | native | Bearer (`ms-` prefix) | magic-grain quota | Alibaba China + real-name; validate cache 24h |
| `qianfan` | qianfan.baidubce.com/v2 | Bearer | ERNIE-Speed/Lite/Tiny free | Chinese real-name auth |
| `volcengine` | ark.cn-beijing.volces.com/api/v3 | Bearer | 2M tok/day/model + 500K new-user | real-name auth |
| `longcat` | api.longcat.chat/openai/v1 | Bearer | daily free quota | email signup works outside China |
| `xfyun` | spark-api-open.xf-yun.com/v1 | Bearer (console APIPassword) | Lite model free | no published ceiling |
| `aihorde` | native (queue proxy) | Keyless (`0000000000`) | kudos | `min_max_tokens=16`, array `stop`, `no_tools`, `no_stream`, 120s |
| `custom` | user-supplied | Bearer | — | 120s (llama.cpp/vLLM/LM Studio) |

**Region-walled / real-name providers** (`google`-sign-in `sea-lion` excepted):
`modelscope`, `qianfan`, `volcengine`, `xfyun` are `region_wall + real_name_auth`
— registered but flagged so `single doctor` and the pool health check surface
"needs a China account" instead of looping cooldowns.

### 5.3 CLI

- `single provider list-free` — the vendored catalog with limits + signup URLs.
- `single provider add-free <id> [--key <k>]` — register one, prompt for the key
  (stored encrypted), validate it (free probe path per §5.1 `validate_url`).
- `single provider sync-pool` — reconcile the whole vendored table into
  `providers.toml` as `single-<id>` presets (E27 aliasing), disabled until keyed.
- `single provider key-status` — per-provider: keyed?, last validation, current
  cooldown, today's RPD/TPD headroom.

---

## 6. Part B — the pool engine (`single-runtime/src/pool/`)

Native Rust port of freellmapi's `services/{router,scoring,ratelimit,provider-quota,cooldown-probe,degradation}.ts`.

### 6.1 `ledger.rs` — 4-D quota accounting

- New table `pool_usage(platform, model, key_id, kind ['request'|'tokens'], amount, at_ms)`.
- Sliding windows: **RPM/TPM** = 60 s rolling; **RPD/TPD** = to next **UTC midnight**
  (providers reset at midnight, not 24 h rolling).
- **Admission check** before dispatch: `recorded + in_flight + estimate < limit`
  for every window that has a limit. Unknown limit → skip that window (but the
  cooldown ceiling is 10 min for guesses — §6.2).
- **In-flight leases**: `acquire_lease(platform, model, key, est_tokens) -> LeaseId`,
  released in a guard (idempotent, 2-min max age backstop). Leases count against
  both minute and day windows so N concurrent dispatches can't all read the
  counter as unspent. In-memory `Vec<Lease>` + a `pool_leases` table for
  cross-restart visibility.
- **Token estimate**: `chars/4` (reuse E27's `parse_or_estimate_tokens` seam);
  refined post-response from real usage where the provider reports it.
- Degraded-DB fallback: windows sum from memory when the SQLite write fails
  (pruned on push, bounded).

### 6.2 `cooldown.rs` — the ladder + probe job

- New table `pool_cooldowns(platform, model, key_id, until_ms, provenance, hits, hits_window_start_ms)`.
- Ladder on a 429/5xx: **transient 90 s** → escalation **2 m → 10 m → 1 h → 1 day**
  over a rolling 24 h hit window; a successful request clears the hit counter.
- **402 payment-required → 1 day**; **403 tier-gate → 1 day**; **401 auth → benched
  until next health cycle (~5 m)**; **local/transport error → 5 s, never enters the
  ladder**.
- **Provenance** — `Heuristic` (a guess; capped at 10 min; probe-recoverable) vs
  `Authoritative` (explicit `Retry-After` or a stated daily-quota reset — never
  shortened, never probed) vs `Credit` / `Tier` (never probed — a passing key
  probe proves nothing about balance).
- **Back-off parser** (`pool/backoff.rs`): the `Retry-After` header (delta-seconds
  or HTTP-date) wins; else walk the error body depth-capped (≤6) for
  `retryDelay` / `retry_after` / `retryAfterSeconds` shapes (Gemini's
  `error.details[].RetryInfo.retryDelay="17s"`); else anchored prose
  ("try again in 30 seconds", "retry after 2m"). All delays clamped to 24 h.
  **Only the number is kept — never the body.**
- **Probe job** (started from `single-runtimed`, every 60 s): re-validates
  `Heuristic`-cooldown keys once half the bench elapsed and >60 s remains; a
  failed probe never extends the bench, only pushes the next probe out
  (2 m doubling, cap 15 m); budgeted per pass (default 3); the probe unit is
  the **key**, not the model. `SINGLE_POOL_COOLDOWN_PROBE_DISABLED=1` kills it.
- **Operator ceiling**: `single pool cooldown-ceiling <10m|1h|6h>` caps the
  ladder and 402/403 benches (provider-stated times are never shortened);
  `single pool cooldown-clear [--key <id>]` lifts benches + score penalties.

### 6.3 `pools.rs` — provider-wide pools

- `infer_pool_key(platform, model) -> String` → `openrouter::free`,
  `google::project`, `nvidia::credit-pool`, `groq::account`, `xkiro::free`,
  `navyai::daily-tokens`, … (per-platform shape from `FreeProvider.pool`).
- Aggregators with one shared free pool (`routeway`, `bazaarlink`, `unorouter`,
  `orcarouter`, `xkiro`, `anyapi`, `navyai`, `nara`, `sea-lion`, `aion`,
  `requesty`) each get one aggregate gate — counted by summing per-model windows
  for the same `platform + key_id` — so an `(N models × RPD)` fan-out can't earn
  surprise 429s.
- `is_shared_pool(platform)` → skip `least-remaining` key selection for these
  (every key reports the same number).

### 6.4 `bandit.rs` — adaptive scoring (Thompson sampling)

- New table `pool_outcomes(platform, model, key_id, ok BOOL, latency_ms, tokens, at_ms)` (7-day retention).
- **Reliability** = Thompson sample of `Beta(α, β)` where
  `α = decay_weighted_successes + 1`, `β = decay_weighted_failures + 1`,
  7-day window, **2-day half-life** (`weight = 0.5^(age_days/2)`).
  Timeouts count as failures for reliability but feed **speed** (wall-clock,
  0 tokens).
- **Speed** = normalized inverse of measured latency + TTFB.
- **Intelligence** = the model's catalog `intelligence_rank` / `size_label`
  (vendored, from `free_pool.rs`), normalized per provider.
- `base = w_rel·reliability + w_speed·speed + w_intel·intelligence` (convex,
  weights sum to 1); `effective = base × headroom_factor × ratelimit_factor`
  (guardrail multipliers ∈ [floor, 1] — headroom shrinks as a window fills).
- **Strategies** (`routing.toml` `[pool]` `strategy`): `balanced` (0.5/0.25/0.25,
  default) · `smartest` · `fastest` · `reliable` · `custom {w_rel,w_speed,w_intel}`
  · `priority` (chain order dominates). `single pool strategy <name>` / a per-goal
  `--pool-strategy`.
- **Key selection** (separate from model ranking): `auto` | `least-remaining`
  (skipped for shared pools).
- `single-pool` slots into the E27 coordinator routing: when a node's agent is
  `single-pool`, `bandit::pick(kind, effort, strategy) -> (provider, model, key)`
  replaces `routing::select_agent`.

### 6.5 `degrade.rs` — degraded-mode state machine

- `healthy_ratio = providers_with_≥1_usable_key / enabled_providers`
  (`unknown` key status counts as healthy).
- `normal → degraded` when ratio < `0.5` for `entry_grace` (60 s);
  `degraded → normal` when ratio ≥ `0.5` for `exit_grace` (120 s, hysteresis);
  only evaluated with ≥ `min_providers` (3) enabled.
- In `degraded`: **bandit exploration off** — stick to scored order of the
  survivors. `single pool status` / the E27 `single coordinator status` show
  `pool: degraded since <ts>`.
- Env-tunable: `SINGLE_POOL_DEGRADED_{HEALTHY_RATIO,MIN_PROVIDERS,ENTRY_GRACE_MS,EXIT_GRACE_MS}`.

### 6.6 `client.rs` — HTTP dispatch

- Reuse `reqwest` (already a workspace dep; blocking client, as `task::run` is sync).
- **OpenAI-compat** path for 33 providers: `POST {base_url}/chat/completions`,
  `Authorization: Bearer`, SSE or non-stream, `<think>` extraction, truncation
  detection, first-byte budget.
- **Native wires**: `google` (Gemini `generateContent`), `cohere`, `cloudflare`
  (compound cred), `zhipu` (host re-probe), `aihorde` (queue submit + poll),
  `sail` (Responses background poll), `modelscope`/`pollinations`/`electronhub`/
  `experiential` (custom validate probe). Each a small `impl PoolWire`.
- **Quirks applied per `FreeProvider.quirks`**: `browser_ua` header,
  `force_single_tool_call`, `no_tools` (strip + 422 if the request needs them),
  `min_max_tokens`, per-provider `timeout`.
- **Retry budget + hedge**: default 45 s wall-clock (`SINGLE_POOL_RETRY_BUDGET_MS`),
  checked before each retry; attempt 0 and the first failover always run; a
  budget expiry mid-attempt aborts the in-flight `reqwest` via a cancel token
  (`HedgeAbort` — not a health signal, no cooldown), disarmed on first byte.
- **Tool-call rescue**: models that emit tool calls as prose → parsed into real
  `tool_calls` (port `lib/tool-call-rescue.ts`); tool requests only route to
  `supports_tools` models.

### 6.7 `handoff.rs` — context handoff

- In-memory session store: `session_key -> { messages_summary, last_provider_model }`,
  TTL 3 h.
- `session_key` = an explicit id from the caller (coordinator node id, ACP
  session id) else SHA-1 of the first user message.
- On a **model switch** for a session (and only then — not first request, not
  same-model continuation, not if a handoff message is already present): prepend
  one compact `system` message:
  ```
  SingleCLI context handoff:
  You are taking over an ongoing task from another model (<old> → <new>).
  Continue using the context already in this request. Do not restart, re-ask
  answered setup questions, or discard prior tool results. The user's latest
  message is the highest-priority instruction.
  Recent summary: <rolling summary>
  ```
- Storage is memory-only; nothing written to disk or logs.

---

## 7. Part C — the `single-pool` agent

- A registry entry `single-pool` (`AgentDefinition`) with `adapter = "pool"`,
  `home_requirement = None` (no isolated `$HOME` — it makes HTTP calls, runs no
  binary), `max_concurrency = None` (the ledger caps it, not a slot).
- `single-runtime/src/pool_agent.rs` implements the same run contract
  `task::execute` expects: given a prompt + cwd + timeout, it
  1. builds messages (system preamble from cwd context, then the prompt),
  2. `bandit::pick` → `(provider, model, key)`, applies `handoff::inject`,
  3. `ledger::acquire_lease`, `client::dispatch`,
  4. on success → `ledger::record`, `bandit::record_ok`, write the artifact
     (same `task_artifact_path` as any task; real `usage` when reported),
  5. on 429/5xx → `cooldown::bench` (+ back-off parse), `bandit::record_fail`,
     pick the next `(provider, model, key)` within the retry budget,
  6. on **all eligible exhausted** → return a typed
     `PoolOutcome::Exhausted { earliest_recovery_ms }` (not a plain failure) —
     this is what the coordinator's auto-continue consumes (§8).
- `single task run --agent single-pool "…"` works standalone (a one-shot
  fallback-loop, no coordinator).
- Coordinator: `routing.toml` `[kind.*]` lists can name `single-pool`; a node
  routed to it goes through the engine instead of a CLI. `coordinator.toml`
  `prefer_pool = true` makes `single-pool` the default for every kind unless a
  kind's list overrides.

---

## 8. Part D — auto-continue on exhaustion

**New goal status `waiting_on_capacity`** (between `running` and `blocked`).

- `settle_finished_node` / `tick`: when a node's dispatch returns
  `PoolOutcome::Exhausted` (or a CLI agent's fallback chain is fully
  rate-limited), the node goes back to `Pending` with an
  `earliest_retry_at_ms` stamp, and the goal → `waiting_on_capacity` with a
  `capacity_reason` (which providers/pools, when the earliest window frees).
- The scheduler tick **skips** a node whose `earliest_retry_at_ms` is in the
  future (so it doesn't spin), and **re-admits** it once the time passes —
  driven by real cooldown/`ledger` state, not a fixed sleep.
- A **resume budget** (`coordinator.toml` `max_capacity_waits_per_goal`,
  default 20, and `max_capacity_wait_minutes`, default 720 / 12 h): past it the
  goal finally goes `blocked` with "waited 12 h for capacity, still exhausted"
  — raised by `single goal amend <id> capacity-budget=N`.
- **No quality compromise**: on resume, the node's prompt is rebuilt with
  `handoff::inject` (so a different provider/model picks up cleanly) and the
  dependency outputs are re-attached. The `careful`/loop machinery (E27 item 4)
  composes with this — a loop iteration that hits exhaustion waits, then
  continues its DONE loop.
- Events: `capacity_wait` (body = reason + ETA), `capacity_resumed`.
  `single coordinator status` shows `waiting: goal_x — nvidia+groq pools spent,
  resumes ~14:03Z`. `single acp` translates `capacity_wait` to an
  `agent_thought_chunk` ("all providers rate-limited; holding, resumes ~14:03Z")
  so the Zed panel shows a live hold rather than a stall.
- Daemon restart during a wait: the `waiting_on_capacity` goal + its node
  stamps are durable (rows), so the tick timer just picks it back up (ties into
  Part F).

---

## 9. Part E — self-heal / self-fix

`single-runtime/src/self_heal.rs`. A **pass** runs: on daemon start, every
`self_heal_interval_secs` (default 300), and on `single doctor --fix`. Every
autonomous change writes a `self_heal_events(at, category, action, detail, ok)`
row and is gated by a per-category toggle in `~/.config/single/self_heal.toml`
(all `true` by default). `single self-heal log` / `single self-heal disable <category>`.

### 9.1 Infra self-repair (category `infra`)

- **Stale socket**: `runtime.sock` exists but no live `single-runtimed` PID → remove it.
- **Zombie rows**: non-terminal `tasks` / `graph_nodes` with a dead/absent
  backing PID → reconcile (extends E27's `reconcile_orphaned_tasks` +
  `coordinator::scheduler::reconcile` — run them from the pass, not just startup).
- **Poisoned locks**: N/A in the daemon (no test `Mutex`), but the pass
  `catch_unwind`s each sub-step so one failure can't wedge the rest.
- **Corrupt config**: every `*.toml` under `~/.config/single/` is parse-checked;
  a broken one is restored from the newest `*.bak-*` sibling (SingleCLI already
  writes timestamped backups) and the event logged; if no backup, the file is
  moved aside and regenerated from defaults.
- **DB integrity**: `PRAGMA integrity_check` on `state/single.db`; on failure,
  restore from `FREEAPI_DB_BACKUP`-style newest backup (add a periodic
  `single.db.bak-*` writer, `db_backup_interval_secs` default 3600) or, last
  resort, rebuild the schema and re-seed (losing history, logged loudly).
- **Dead agent binaries**: re-run `$PATH` augmentation (E27's `augmented_path`)
  and `cached_discover`; if an agent that was detected is now missing, mark it
  and (if `agent` self-install is enabled, §9.3) queue a reinstall.
- **Cooldown recovery**: the §6.2 probe job is part of this category.

### 9.2 Coordinator self-correction (category `coordinator`)

- A goal that has been `blocked` for > `blocked_reeval_minutes` (default 30) with
  a `capacity`/`supervisor` reason is **re-evaluated**: if the blocking condition
  cleared (pools recovered, an agent came back), it's moved to `running` and
  re-ticked — bounded by `max_auto_reevals_per_goal` (default 3).
- A goal failing repeatedly on the **same node with the same agent** → the pass
  edits that node's routing (next agent in the kind's list, or `single-pool`),
  logs a `coordinator` event, re-ticks. Bounded, and never touches a goal a
  human `amend`ed in the last hour.
- **`routing.toml` drift**: if a routing list names an agent that has been
  undetected for > 24 h, the pass comments it out (writes the file, backup
  first) and logs it; re-adds on `single provider sync-pool` / next detection.
- **`coordinator.toml` sanity**: absurd values (`max_parallel = 0`,
  `max_goal_minutes < 1`) are reset to defaults with a logged event.
- Hard rule: the pass never *deletes* a goal, never force-kills a running node,
  never edits a config a human touched inside the last hour.

### 9.3 Agent self-install / repair (category `agent`)

- When a coordinator node needs agent `X` and `X` is not
  detected-and-authed, the pass (if enabled) runs SingleCLI's existing
  `bootstrap::run_one` / `single setup --yes` path for `X`, then re-probes.
- **Auth repair**: if `X` is installed but `has_live_login` is false, the pass
  does *not* attempt an interactive login (can't, headless) — it emits a
  `self_heal_events` row + a coordinator `blocked` reason "agent X needs
  `single agent login X`" and routes the node to an alternative. `keyring`-auth
  agents (codex/cursor, E27) get their `cli_reports_logged_in()` re-checked.
- **`single-pool` provider keys**: a provider whose key fails validation for >
  `provider_key_grace_hours` (default 24) is auto-disabled in the pool (logged),
  and `single provider key-status` flags it for re-keying.
- Every install is timeout-bounded, `-j` respectful, and one-agent-at-a-time
  (reuse the E27 doctor 4-permit gate). `SINGLE_SELF_HEAL_AGENT_INSTALL=0` off.
- **Safety**: agent installs shell package managers — so this category is the
  one most likely to want `false` on a shared box; `self_heal.toml` documents
  that and `single doctor` prints whether it's on.

---

## 10. Part F — self-healing / self-resuming sessions

E27 gave durable `sessions` / `goals` / `graph_nodes` / `coordinator_events`
and `session/load` replay. This part makes an **interrupted goal resume**, not
just survive as a row.

- **On daemon start** (`server.rs` startup block, after `reconcile`):
  `coordinator::resume_interrupted()` —
  - each `graph_nodes` row left `running` whose task is terminal/gone →
    already handled (E27 reconcile → `failed`/`done`);
  - **new**: a goal in `running` / `planning` / `waiting_on_capacity` with no
    live node and not all-terminal is **re-ticked** — the scheduler recomputes
    the ready-set and re-dispatches `pending` nodes. A node that had a partial
    artifact keeps it and is fed back via `handoff::inject`.
  - a goal in `planning` with an empty graph → `plan_goal` is re-run (the
    planner call was interrupted).
  - `resume_interrupted` writes a `session_resumed` event per goal touched.
- **ACP**: `single acp`'s `session/load` (E27) already rebinds by `sess_` id and
  replays events. Addition: after replay, if the session has a
  `running`/`waiting_on_capacity` goal, the bridge **re-attaches its event
  long-poll** so the Zed panel keeps streaming a goal that was in flight when
  the panel (or the daemon) restarted — no new prompt needed.
- **Crash vs clean stop**: a clean `single daemon stop` marks its goals
  `paused` (new terminal-ish state that `resume_interrupted` treats as
  "re-tick"); a crash leaves them `running` and the PID check + re-tick covers
  it. Either way the user sees continuity.
- `single goal resume <id>` — manual trigger of the same path for a goal that
  ended up `blocked`/`failed` but is recoverable.

---

## 11. Part G — new agent adapters

freellmapi wires 17 agents; SingleCLI already covers most. **New adapters +
detection + `configure` (native config write) for:**

| Agent | Config file | Wire | Notes |
|---|---|---|---|
| `cline` | `~/.config/Code/User/settings.json` | OpenAI Chat | VS Code extension settings block |
| `continue` | `~/.continue/config.json` | OpenAI Chat | ghost-text autocomplete model too |
| `roo` | `~/.config/roo/config.json` | OpenAI Chat | — |
| `mimo` | `~/.config/mimocode/config.json` | OpenAI Chat | OpenCode-derived; `provider` map + `{env:…}` key syntax |
| `atomcode` | `~/.atomcode/config.toml` | OpenAI Chat | Rust CLI; `[providers.freellmapi]`-style TOML, `default_provider` |
| `deepseek-harness` (`dsh`) | `~/.dsh/settings.yaml` + `~/.dsh/.env` | OpenAI Completions | routes under `llm-pi-ai.providers`; key in `.env` mode 0600 |

Each follows SingleCLI's existing `AgentAdapter` pattern (E27 `configure_mcp`
precedent): structural merge, timestamped backup, `0600`. The
`single-<name>`-aliased pool providers (Part A) and `single-pool` itself become
selectable models in each. `single install-integrations` learns these targets.
`goose`, `aider`, `qwen-code`, `crush`, `kilocode` already exist — just ensure
the pool providers sync into them.

---

## 12. Data model (all additive, `CREATE TABLE IF NOT EXISTS`)

| Table | Purpose |
|---|---|
| `pool_provider_keys` | `(platform, key_id, secret_ref, added_at, last_validated_at, valid BOOL, disabled BOOL)` — API keys, secret via existing encrypted store |
| `pool_usage` | `(platform, model, key_id, kind, amount, at_ms)` — RPM/RPD/TPM/TPD events, 1-day retention |
| `pool_leases` | `(lease_id, platform, model, key_id, est_tokens, acquired_at_ms)` — in-flight, cross-restart |
| `pool_cooldowns` | `(platform, model, key_id, until_ms, provenance, hits, hits_window_start_ms)` |
| `pool_outcomes` | `(platform, model, key_id, ok, latency_ms, tokens, at_ms)` — bandit input, 7-day retention |
| `self_heal_events` | `(at, category, action, detail, ok)` |
| `goals` (E27) | + `capacity_reason TEXT`, `earliest_retry_at_ms INTEGER`, `capacity_waits INTEGER DEFAULT 0` (via `add_column_if_missing`) |
| `graph_nodes` (E27) | + `earliest_retry_at_ms INTEGER` |

New `GoalStatus` variants: `WaitingOnCapacity`, `Paused` (serde `waiting_on_capacity` / `paused`).

## 13. Config files (all optional, defaults written on first use)

- `~/.config/single/free-pool.toml` — *generated* from the vendored table by
  `sync-pool`; the operator edits `enabled`/per-provider overrides here.
- `~/.config/single/routing.toml` (E27) — `[pool]` block: `strategy`,
  `key_selection`, `custom` weights.
- `~/.config/single/coordinator.toml` (E27) — `prefer_pool`,
  `max_capacity_waits_per_goal`, `max_capacity_wait_minutes`.
- `~/.config/single/pool.toml` — engine tunables (`retry_budget_ms`,
  `cooldown_ceiling`, degraded-mode params) — or reuse env vars only; decide in
  the plan.
- `~/.config/single/self_heal.toml` — `[categories] infra/coordinator/agent = bool`,
  intervals, `db_backup_interval_secs`, grace windows.

## 14. Testing

| Layer | Test |
|---|---|
| `free_pool` | the vendored table parses; every `base_url` is a valid URL or `""`; every `id` unique; `wire`/`auth`/`quirks` well-formed. Pure. |
| `ledger` | admission math (recorded + in-flight + estimate vs limit) across all 4 windows; UTC-midnight rollover; lease acquire/release idempotency; degraded-DB memory fallback. Pure + in-mem SQLite. |
| `cooldown` | ladder escalation over a scripted hit sequence; provenance (heuristic capped at 10 m, authoritative untouched); `backoff.rs` parses `Retry-After` header, Gemini `RetryInfo`, prose "try again in 30s", clamps to 24 h, keeps only the number. Pure. |
| `pools` | `infer_pool_key` per platform; aggregate gate = sum of per-model windows for same platform+key; `is_shared_pool` list. Pure. |
| `bandit` | Beta posterior + decay weighting (0.5^(age/2)); Thompson sample bounded [0,1]; strategy weights convex & sum to 1; `effective = base × headroom × ratelimit`; timeout counts as reliability-fail + speed-sample. Pure. |
| `degrade` | ratio state machine: enter after entry_grace below, exit after exit_grace above, no flap below min_providers, exploration flag flips. Pure. |
| `handoff` | injects exactly once on model switch; not on first request / same-model / already-present; session key = explicit id else SHA-1 of first user msg. Pure. |
| `client` | request shape per wire (URL, auth placement, quirk headers) against a mock HTTP server (`wiremock`-style or a hand-rolled `TcpListener`); SSE parse; hedge-abort is not a health signal. |
| pool_agent | `#[ignore]` integration: `single task run --agent single-pool "say PONG"` against 2–3 real keyed free providers → answers, records usage, benches on a forced 429. |
| auto-continue | scheduler test with a fake dispatcher returning `Exhausted { earliest_recovery_ms }` → goal `waiting_on_capacity`, node stamped, tick skips until the stamp passes, then re-admits; resume budget → `blocked`. Pure (fake dispatcher). |
| self_heal | tempdir: stale socket removed; corrupt `*.toml` restored from `.bak-*`; `integrity_check` fail → restore; a `self_heal_events` row per action; a disabled category is a no-op. |
| resume | tempdir daemon-start path: a `running` goal with a `pending` node and no live task → re-ticked + `session_resumed` event; a `planning` goal with empty graph → `plan_goal` re-run (stub planner). |
| adapters (Part G) | each new generator: structural merge preserves unrelated keys, timestamped backup, `0600`, snapshot of the written block. |

Real-network calls stay `#[ignore]`. Everything else deterministic.

## 15. Migration & compatibility

- `single task run` / `orchestrate-*` / `single goal|session|acp|loop|serve`
  (E27) unchanged. `single-pool` is opt-in per agent/route.
- New tables additive; new `GoalStatus` variants parse-tolerant
  (`GoalStatus::parse` already falls back).
- No provider key is ever required — an unkeyed pool provider is simply
  skipped by admission.
- Version → **0.11.0**; `## [0.11.0]` CHANGELOG; tag `v0.11.0` on release.
- Sole-author commits, `type: description`, no trailers (git-commit-standards).

## 16. Build order (for the implementation plan)

1. **Part A** — `free_pool.rs` vendored table + types + `provider list-free` /
   `add-free` / `sync-pool` / `key-status`. Pure data + CLI. Lands first, no risk.
2. **Part B core** — `ledger.rs` + `cooldown.rs` + `backoff.rs` + `pools.rs`
   (all pure / in-mem SQLite, exhaustively unit-tested). Then `bandit.rs`,
   `degrade.rs`, `handoff.rs`.
3. **Part B `client.rs`** — OpenAI-compat wire first (covers 33 providers),
   then the native wires (`google`, `cohere`, `cloudflare`, `zhipu`, `aihorde`,
   `sail`, `modelscope`, `pollinations`, `electronhub`, `experiential`).
   Retry budget + hedge + tool-call rescue.
4. **Part C** — `single-pool` agent registry entry + `pool_agent.rs`;
   `single task run --agent single-pool` end to end.
5. **Part C coordinator wiring** — `bandit::pick` as a routing target;
   `prefer_pool`.
6. **Part D** — `waiting_on_capacity` state, node stamps, tick skip/re-admit,
   resume budget, events, ACP translation.
7. **Part F** — `resume_interrupted()` on daemon start; `paused` state on clean
   stop; `single goal resume`; ACP re-attach.
8. **Part E** — `self_heal.rs` pass: `infra` first, then `coordinator`, then
   `agent` (behind its own toggle); `single doctor --fix`, `single self-heal
   log|disable`.
9. **Part G** — the 6 new agent adapters + `install-integrations` targets.
10. Version bump, CHANGELOG, deploy, live verification (keyed pool run;
    forced-429 cooldown; degraded-mode trip; auto-continue hold+resume;
    daemon-restart goal resume; `doctor --fix` on a deliberately-corrupted
    config).

Each part is independently testable and shippable; 1–3 are the bulk.

## 17. Open questions for the plan author

- **`pool.toml` vs env-only** for engine tunables — §13. Lean: a small
  `pool.toml` for the operator-facing knobs (strategy is already in
  `routing.toml`), env vars for the rest.
- **Bandit `community_*` priors** — freellmapi seeds Beta priors from a
  community feed. We have no feed (D3). Start with `Beta(1,1)` uniform;
  document the seam.
- **Media models** (`siliconflow` FLUX/CosyVoice, image/video/audio) — Part A
  registers the providers but the plan should decide whether `single-pool`
  does non-text at all this iteration (recommend: text-only v1, media a
  follow-up).
- **`sail` payment-method + `modelscope`/`qianfan`/`volcengine`/`xfyun`
  region+real-name** — register but default-disabled with a clear
  `key-status` reason; the plan confirms they're not in the default pool.
