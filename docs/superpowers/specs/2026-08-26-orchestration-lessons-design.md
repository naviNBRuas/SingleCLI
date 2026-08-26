# Orchestration operational lessons — design

## Status

Proposed — approved approach, awaiting user's go-ahead to implement. Do
NOT start implementation until explicitly told; this spec exists so the
plan is ready the moment that happens.

## Context

Two full sessions (2026-08-25 overnight, 2026-08-26 daytime) of using
SingleCLI as a real multi-agent orchestrator — dispatching work across
~29 repos via `singlecli-mcp`'s `task_run`/`orchestrate_*` tools —
surfaced six operational issues. A deep-dive investigation (with
file:line citations) established which are genuine gaps vs. already
solved vs. not actually SingleCLI's problem:

| # | Issue | Verdict |
|---|---|---|
| 1 | Rate-limit exhaustion (copilot/cursor/grok/gemini all hit quota within minutes) returns a generic `exit_code: 1, no output` — no signal to the caller | **Real gap** |
| 2 | opencode can't run two instances in parallel (its own SQLite session lock) | Not a SingleCLI bug — worth a registry guard anyway |
| 3 | Which agents need `real_home:true` vs. break under it is tribal knowledge from trial and error | **Real gap** |
| 4 | `orchestrate_parallel_run`/`orchestrate_graph_run` isolate work into git worktrees; merging back is fully manual | **Real gap** (by design — fix should assist, not override, the "human decides" principle) |
| 5 | codebuff is listed `"authenticated": "authenticated"` in `agent_list` but has no non-interactive run mode at all — only discoverable by a failed `task_run` call | **Real gap** |
| 6 | `single-mcp`/`single-lsp`'s dynamic MCP/LSP gateway tools were never used across either session | **Not a gap** — they're for a calling agent's own session to register as its MCP/LSP servers, not something an orchestrator dispatches *to* via `task_run`. No task all session needed a third-party MCP tool or LSP lookup. |

This spec covers #1, #3, #4, #5, plus a registry-level concurrency guard
for #2 (agreed useful even though it's not a SingleCLI-caused bug). #6
needs no code change.

## Approach

Extend the existing `AgentDefinition` (`single-core::registry`),
`CapabilityFlags` and `TaskRecord` (`single-protocol`) structs in place,
and enforce at the existing `task.rs`/`orchestrate.rs` call sites —
rather than a standalone "agent health" subsystem (duplicates data
`registry.rs` already owns) or patching only at the `singlecli-mcp`
boundary (duplicates `ratelimit.rs`'s existing detection logic and
wouldn't help plain `single-cli` callers). This is the natural
continuation of patterns already in the codebase — `CapabilityFlags`
exists for exactly this kind of per-agent metadata already.

All changes are additive (`Option<>`/bool fields with defaults) — no
existing tool schema breaks.

---

## 1. Data model

```rust
// single-core/src/registry.rs — AgentDefinition gains:
pub home_requirement: HomeRequirement,   // NEW
pub max_concurrency: Option<u32>,        // NEW — None = unlimited

pub enum HomeRequirement {
    IsolatedOnly,   // breaks under real_home:true
    RealRequired,   // real_home:true is the only way it authenticates
    Either,         // works either way
    Unverified,     // default — nobody's confirmed either way yet
}
```

```rust
// single-protocol/src/lib.rs — CapabilityFlags gains:
pub non_interactive_run: bool,   // NEW — false for codebuff-style agents (default true)

// TaskRecord gains:
pub rate_limited: bool,          // NEW — true if looks_like_rate_limit matched this run's output
```

`HomeRequirement` deliberately keeps the project's existing "only claim
what's verified" ethos (matches `ratelimit.rs`'s own doc comment) —
`Unverified` is the honest default for the ~18 of 22 agents nobody has
empirically confirmed either way, rather than guessing.

**Populate at registration** (`registry.rs`), from what's already
empirically established across both sessions:
- `RealRequired`: `copilot` (structural — `agent_home.rs`'s
  `strip_embedded_credential_fields` always strips its keyring pointer
  from the isolated home, so isolated auth is impossible by design),
  `opencode` (empirical, both sessions)
- `IsolatedOnly`: `cursor`, `grok`, `codex` (empirical, 2026-08-25 session)
- `Either`: `claude` (the native harness)
- `Unverified`: everything else

`max_concurrency`: `Some(1)` for `opencode`, `None` for everything else.

`non_interactive_run: false` for `codebuff` (its adapter falls through
to `run_prompt`'s default-trait `bail!` in
`single-agent-sdk/src/adapter.rs:89` — the flag makes that fact
queryable instead of discoverable only via a failed call). `true` for
every other adapter, since they each override `run_prompt`.

## 2. Rate-limit surfacing (issue #1)

Today, `looks_like_rate_limit()` only runs inside `maybe_fail_over`
(`task.rs:990`), itself gated behind `opts.allow_fallback` (defaults
`false` in both the runtime and `singlecli-mcp`'s arg parsing). None of
today's dispatches set it, so the detector never ran on any of the
four quota failures hit today.

Change: run `looks_like_rate_limit()` unconditionally on every
non-zero-exit task completion, regardless of `allow_fallback`. Set
`TaskRecord.rate_limited = true` when it matches. When `opts.account`
is `Some`, also flip `AccountStatus::RateLimited` as the existing
fallback path already does; when `None` (today's default/unnamed-account
calls), skip the account-level update — can't attribute it to a named
account — but still surface the per-task field. That field alone is
the valuable part for an orchestrator deciding whether to retry an
agent or move on.

`singlecli-mcp`'s `task_run` tool result already returns the full
`TaskRecord` shape, so this is "free" once the field exists on the
struct — no separate wiring needed in `server.rs`.

## 3. `real_home` correctness surfaced (issue #3)

`agent_list` (CLI) and its MCP equivalent gain the new
`home_requirement` field on each agent's entry — same place
`capabilities` already appears in the JSON. A caller can now check
`home_requirement` before deciding whether to pass `real_home: true`,
instead of relying on trial-and-error learned across sessions.

Explicitly **out of scope for this pass**: changing
`is_authenticated`'s behavior (today it only ever checks the isolated
home, per `account.rs:214`'s own test name). That's a real related gap
but a deeper behavior change — flagged as a follow-up, not bundled in
here, to keep this change bounded.

## 4. Concurrency guard (issue #2)

An in-process semaphore keyed by agent name, acquired before spawn and
released on completion, inside `task.rs`'s single spawn entrypoint —
applies uniformly whether the caller is `single-cli`, `task_run`, or
`orchestrate_parallel_run`/`orchestrate_graph_run`, since all three
funnel through the same function. When `max_concurrency` is set and
already at capacity, the new call **queues** (blocks until a slot
frees) rather than failing — turns opencode's own SQLite-lock crash
into a clean serialized wait.

## 5. Structured "not invocable" flag (issue #5)

`agent_list`'s `capabilities` object gains `non_interactive_run` (see
data model above). A caller checks this before dispatching instead of
discovering incompatibility via a failed `task_run` call. The
underlying fact already exists in two places today (the trait-default
`bail!` and a prose note in the registry) — this unifies them into one
queryable field, replacing neither.

## 6. Assisted worktree merge-back (issue #4)

`docs/architecture.md:743` is explicit: *"Branches are never
auto-merged; that stays a human decision."* This fix assists that
decision rather than overriding it.

New functions in `single-core/src/worktree.rs`:
- `diff(repo: &Path, branch: &str) -> Result<String>` — `git diff
  main...branch`
- `merge(repo: &Path, branch: &str) -> Result<MergeResult>` — performs
  the merge

New CLI commands: `single worktree diff <name>`, `single worktree merge
<name>`.

New `singlecli-mcp` tools, kept as **two separate tools** rather than
one tool with a `confirm` flag: `worktree_merge_preview` (returns the
diff) and `worktree_merge_apply` (performs the merge). Two deliberate
tool calls — look, then decide — mirrors the human-decision principle
in an agent-caller context better than a boolean an orchestrator could
set blindly on the first call. `orchestrate_parallel_run`'s and
`orchestrate_graph_run`'s tool descriptions get updated to point at
these as the recommended way to close out a worktree-isolated run.

## Testing

Follow existing per-module unit-test convention (`ratelimit.rs` and
`agent_home.rs` already have tests in this style). New tests needed:
`worktree.rs`'s `diff`/`merge`, the concurrency semaphore (two
simulated concurrent spawns against a `max_concurrency: Some(1)` agent
serialize rather than race), and `looks_like_rate_limit` now firing
unconditionally (existing tests still pass unchanged — no signal
change to the detector itself, only to when it's called).

## Rollout

All changes additive — no existing tool schema breaks. Workspace is at
`0.6.0` (pre-1.0); this is a minor feature addition → `0.7.0` per this
project's own SemVer convention.

## Open questions for user review

None — all decisions in this spec were either directly confirmed or
are conservative defaults (e.g. scoping out `is_authenticated`
changes) called out explicitly above.
