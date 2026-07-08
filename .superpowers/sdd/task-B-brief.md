# Task B: Session backend — cost calculation, session commands, idle sweep

Source: PLAN.md, "Ordered task breakdown" items 11, 12, 15, plus §5 "Cost calculation" and the relevant part of §6 "AI summary generation" (only the session-end-detection constant, not summarization itself — that's a later task).

## Current state (read before writing anything)

- `src-tauri/src/db/queries.rs` already has every DB-side primitive this task needs, unused so far (dead-code warnings on `cargo build` confirm this):
  - `list_sessions(conn) -> Vec<Session>`, `get_session_detail(conn, session_id) -> Option<(Session, Vec<FileChanged>)>` — read-side queries, already correct, just not exposed as Tauri commands yet.
  - `sessions_to_finalize(conn, idle_threshold_secs, now) -> Vec<String>` — returns session ids where `status='active' AND (now - last_activity_at) > idle_threshold_secs`.
  - `finalize_session(conn, session_id)` — sets `status='ended'`, `ended_at = last_activity_at`, `duration_seconds = last_activity_at - COALESCE(started_at, last_activity_at)`.
  - `update_cost(conn, session_id, cost_usd)` — absolute `UPDATE sessions SET cost_usd = ?`.
  - `all_session_token_totals(conn) -> Vec<SessionTokenTotals>` — every session's `{id, model, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens}`, for recomputing cost from accumulated totals without re-parsing logs.
- `src-tauri/src/parser/session_builder.rs`'s `ingest_record()` calls `queries::upsert_session(...)` to accumulate token deltas per assistant record, but never touches `cost_usd` — it stays `0` forever right now.
- `src-tauri/src/commands.rs` only exposes `list_projects`. `src-tauri/src/lib.rs`'s `.invoke_handler(tauri::generate_handler![commands::list_projects])` only registers that one command.
- `src-tauri/src/lib.rs`'s `setup()` opens the DB, manages it as Tauri state, and calls `watcher::start(app.handle().clone())`. No background tokio task exists yet — `tokio` is a dependency (`rt-multi-thread`, `time`, `fs`, `macros` features) but nothing spawns a runtime/task today.
- No `cost/` module exists yet. No `resources/` directory or `pricing.json` exists yet.
- `src-tauri/Cargo.toml` already has `tokio = { version = "1", features = ["rt-multi-thread", "time", "fs", "macros"] }` — you do not need to add a dependency for the sweep's `tokio::time::interval`. Tauri's own async runtime (Tauri 2 bundles a tokio runtime internally for its own async commands) means you can spawn a task with `tauri::async_runtime::spawn` — check what's idiomatic for Tauri 2's `app.handle()` context rather than manually starting a second tokio runtime. Look at how `watcher::start` is invoked from `setup()` for the pattern this codebase already uses for background work (it uses `std::thread::spawn` for the watcher's blocking loop, since `notify`'s channel-based API is synchronous — the idle sweep is different: it's an async interval loop, so `tauri::async_runtime::spawn(async move { ... })` is the natural fit, not a raw OS thread).

## What to build

### 1. `src-tauri/resources/pricing.json` (new)

Static, bundled pricing table. Exact content (verbatim — these are the real per-million-token USD rates in the plan):

```json
{
  "schema_version": 1,
  "rates_per_million_tokens": {
    "claude-opus-4-8":           { "input": 15.0, "output": 75.0, "cache_write": 18.75, "cache_read": 1.5 },
    "claude-sonnet-5":           { "input": 3.0,  "output": 15.0, "cache_write": 3.75,  "cache_read": 0.3 },
    "claude-haiku-4-5-20251001": { "input": 1.0,  "output": 5.0,  "cache_write": 1.25,  "cache_read": 0.1 },
    "_default":                  { "input": 3.0,  "output": 15.0, "cache_write": 3.75,  "cache_read": 0.3 }
  }
}
```

### 2. `src-tauri/src/cost/mod.rs` and `src-tauri/src/cost/pricing.rs` (new module, add `mod cost;` to `lib.rs`)

- Load `pricing.json` via `include_str!("../../resources/pricing.json")` (bundled at compile time — this is a static resource, not something read from disk at runtime, per the plan: "static, bundled, agent[can't ]fetch"). Parse once (e.g. `OnceLock`), never re-read from disk.
- A lookup function, e.g. `pub fn cost_usd(model: Option<&str>, prompt_tokens: i64, completion_tokens: i64, cache_read_tokens: i64, cache_creation_tokens: i64) -> f64`.
- **Lookup order** (exact wording from PLAN.md §5 — implement all four branches):
  1. Exact match on the model string in `rates_per_million_tokens`.
  2. Longest-prefix match (handles versioned/dated suffixes on a known model family — e.g. a future `claude-opus-4-8-20260115` should match the `claude-opus-4-8` entry via prefix, not fall through).
  3. Sentinel check: if the model string starts with `<` (e.g. the real `<synthetic>` value seen in logs), treat as **non-billable** — return `0.0` — do **not** fall through to `_default`.
  4. Otherwise, `_default`, logging a one-time warning per unique unrecognized model string (mirror the existing `log_unknown_type_once` pattern in `src-tauri/src/parser/claude_jsonl.rs` — same rate-limiting idea, just for model strings instead of record types).
  - `model: None` (no model seen yet for a session) should also resolve to `_default` without panicking — do not treat `None` as an error case.
- Formula: `(input_tokens/1e6)*input_rate + (output_tokens/1e6)*output_rate + (cache_creation_input_tokens/1e6)*cache_write_rate + (cache_read_input_tokens/1e6)*cache_read_rate`. Note the DB/Rust field names are `prompt_tokens`/`completion_tokens`/`cache_creation_tokens`/`cache_read_tokens` (see `Session`/`SessionTokenTotals` in `queries.rs`) — these map onto `input_tokens`/`output_tokens`/`cache_write`/`cache_read` rates respectively; don't let the naming mismatch cause a mixup.
- Never panic on an unrecognized model string, missing pricing entry, or malformed `pricing.json` (it's bundled and controlled by us, but write it defensively anyway — this file's whole reason to exist per the plan is "cost accuracy... resilient to unknown model values").
- Write unit tests here (in a `#[cfg(test)] mod tests` block, following the pattern already established in `src-tauri/src/parser/claude_jsonl.rs`'s test module) covering: exact match, `_default` fallback with an unrecognized model, the `<synthetic>`-style sentinel resolving to `0.0` (not `_default`), and `model: None`. These three specific cases are also explicitly required by PLAN.md's own "Verification" section ("`<synthetic>`, an unknown model string, ... resolve without panicking, hitting the sentinel/`_default`/exact-match branches respectively") — make sure your tests actually exercise all three named branches, not just a couple of them.

### 3. Wire cost into ingestion (`src-tauri/src/parser/session_builder.rs`)

After `queries::upsert_session(...)` runs inside `ingest_record()` (the call already there, which accumulates token deltas), recompute and persist `cost_usd` for that session using the **current accumulated totals**, not the delta — i.e. read the session's now-updated token totals back out (there's no existing "get one session's totals" query; either add a small one to `queries.rs` alongside `all_session_token_totals`, e.g. `session_token_totals(conn, session_id) -> Option<SessionTokenTotals>`, or reuse/adapt what's there — your call, keep it consistent with existing query style) and call `cost::pricing::cost_usd(...)`, then `queries::update_cost(conn, session_id, cost)`. This "recompute from accumulated totals" approach (rather than delta-summing a cost column) is deliberate: it's what makes the existing `all_session_token_totals` + `update_cost` pair in `queries.rs` reusable later for recomputing all costs after a pricing-table edit without re-parsing logs — don't invent a separate delta-based cost accumulation path that would drift from that.

### 4. Startup cost backfill (`src-tauri/src/lib.rs` `setup()`)

Once per app start, after opening the DB: call `queries::all_session_token_totals(conn)`, compute `cost::pricing::cost_usd(...)` for each, and `queries::update_cost(...)` for any that differ from the stored value (or just unconditionally — your call on whether an equality check is worth the complexity given this runs once at startup over what's realistically a small number of rows). This exists to backfill `cost_usd` for sessions that were ingested before this task existed (their `cost_usd` is currently stuck at `0` in the DB) — the plan calls this out as "backfill cost on ingest."

### 5. Tauri commands (`src-tauri/src/commands.rs` + `src-tauri/src/lib.rs`)

Add `#[tauri::command] list_sessions` and `#[tauri::command] get_session_detail(session_id: String)` wrapping the existing `queries::list_sessions`/`queries::get_session_detail`, following the exact pattern `list_projects` already uses (lock the `Db` state's mutex, map errors to `String`). Register both in `lib.rs`'s `.invoke_handler(tauri::generate_handler![...])` alongside `list_projects`.

For `get_session_detail`, decide a sensible return shape for the frontend (e.g. a small struct wrapping `{ session: Session, files_changed: Vec<FileChanged> }`, or a tuple — `Session`/`FileChanged` already derive `Serialize`) and return `Result<Option<YourShape>, String>` (the query already returns `Option` for "no such session").

Do **not** add frontend TypeScript for these yet (no `listSessions`/`getSessionDetail` in `src/lib/tauri.ts`, no `Session`-shaped changes needed in `src/lib/types.ts` beyond what's already there) — the Sessions view that will consume these commands is a separate, later task. Just get the backend surface correct and tested.

### 6. Idle-session sweep (`src-tauri/src/lib.rs` `setup()`, or a new small module if you prefer — your call, but don't over-split for what's a ~15-line loop)

- Constant: `120` seconds idle threshold (not user-configurable yet, per the plan — "constant, not yet user-configurable").
- Sweep interval: ~20 seconds (`tokio::time::interval` or equivalent — see the async-runtime note above).
- Each tick: `queries::sessions_to_finalize(conn, 120, now)` (where `now` is the current unix timestamp — `chrono::Utc::now().timestamp()`, matching how timestamps are stored elsewhere in this codebase), then `queries::finalize_session(conn, id)` for each. If any were finalized, emit the same `"data-changed"` Tauri event the watcher already emits on ingest (see `src-tauri/src/watcher/mod.rs`'s `process_file` for the exact emit pattern/payload shape — reuse that shape, e.g. `{"entity": "session", "kind": "updated"}"`, so the frontend's existing `useDataChangedEvents` hook picks it up without changes).
- Needs access to the `Db` state and the `AppHandle` (for `.emit(...)`) inside the spawned task — `app.handle().clone()` is already how `watcher::start` gets this; follow the same pattern.

## Explicitly out of scope for this task

- No AI summary pipeline, no tag classification — those are separate later tasks (PLAN.md items 16/17) that build on `finalize_session` existing, but summarization/tagging logic itself is not this task.
- No frontend changes at all (no Sessions view, no `tauri.ts` additions, no `types.ts` changes).
- No changes to `upsert_session`'s token-accumulation logic itself (already correct/tested) — only add cost calculation alongside it.

## Verification

- `cargo test` (from `src-tauri/`) must pass, including your new pricing unit tests, and must not break any of the 20 existing tests in `parser/claude_jsonl.rs`, `parser/session_builder.rs`, `watcher/tail.rs`.
- `cargo build` (or `cargo check`) must succeed with no new warnings beyond what's unavoidable (the existing dead-code warnings on `list_sessions`/`get_session_detail`/`update_cost`/`all_session_token_totals`/etc. should now disappear since you're using them — if any of those warnings persist after this task, something didn't get wired up).
- Consider adding a `session_builder.rs` test (alongside the existing ones there, using the same `in_memory_db()` + fixture-ingestion helpers already in that file's `#[cfg(test)]` module) confirming that after ingesting `tests/fixtures/session_basic.jsonl`, the session's `cost_usd` is a nonzero value consistent with `claude-opus-4-8`'s rates in `pricing.json` and the fixture's known token totals (148 input / 700 output / 21800 cache_read / 290 cache_creation — these exact numbers are asserted already in `session_builder.rs`'s `ingesting_fixture_accumulates_token_totals_and_model_on_the_session` test, so you can compute the expected cost by hand: `(148/1e6)*15.0 + (700/1e6)*75.0 + (290/1e6)*18.75 + (21800/1e6)*1.5`).
- The idle-sweep and startup-backfill logic (things that involve `AppHandle`/live timing) don't have a clean unit-test seam in this codebase yet — a manual reasoning check (re-read your own code) is acceptable for those two pieces specifically; don't invent a Tauri-mocking test harness for this task.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point, including task boundaries. Leave all your changes in the working tree, uncommitted. Do not run `git commit` (staging with `git add` is fine and encouraged if it helps you track your own progress, but do not commit).

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-B-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
