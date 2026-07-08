# Task B Report: Session backend — cost calculation, session commands, idle sweep

Status: **DONE**

## What I implemented

1. **`src-tauri/resources/pricing.json`** (new) — the exact static pricing table verbatim from the brief (`claude-opus-4-8`, `claude-sonnet-5`, `claude-haiku-4-5-20251001`, `_default`).

2. **`src-tauri/src/cost/mod.rs`** + **`src-tauri/src/cost/pricing.rs`** (new module, `mod cost;` added to `lib.rs`):
   - `pricing.json` loaded once via `include_str!` + `OnceLock`, never re-read from disk.
   - `pub fn cost_usd(model: Option<&str>, prompt_tokens: i64, completion_tokens: i64, cache_read_tokens: i64, cache_creation_tokens: i64) -> f64` implementing all four lookup-order branches exactly as specified:
     1. Exact match on `rates_per_million_tokens`.
     2. Longest-prefix match (`max_by_key(|(key, _)| key.len())` over keys where `model.starts_with(key)`, excluding `_default`).
     3. Sentinel: `model.starts_with('<')` → `0.0` (implemented as zero rates, so it flows through the same formula rather than being a special-cased early return) — does **not** fall through to `_default`.
     4. Otherwise `_default`, with a one-time warning per unique unrecognized model string via a `log_unknown_model_once` helper mirroring `claude_jsonl.rs`'s `log_unknown_type_once` pattern (`OnceLock<Mutex<HashSet<String>>>`).
   - `model: None` resolves directly to `_default` rates, bypassing the exact/prefix/sentinel branches entirely (per the brief: "should also resolve to `_default` without panicking — do not treat `None` as an error case").
   - Malformed/missing `pricing.json` is handled defensively: if `serde_json::from_str` fails, falls back to a hardcoded `_default`-only table (`FALLBACK_DEFAULT_RATES`) rather than panicking; `default_rates()` also `.unwrap_or(&FALLBACK_DEFAULT_RATES)` in case `_default` itself is missing from a malformed file.
   - Field-name mapping verified: `prompt_tokens→input`, `completion_tokens→output`, `cache_creation_tokens→cache_write`, `cache_read_tokens→cache_read` — matches the brief's explicit warning about the naming mismatch.

3. **Wired cost into ingestion** (`src-tauri/src/parser/session_builder.rs`): after the existing `queries::upsert_session(...)` call inside `ingest_record()`, added a read-back of the session's now-updated accumulated totals via a new `queries::session_token_totals(conn, session_id)` query (added to `queries.rs`, single-session variant of `all_session_token_totals`, same style/columns), computed cost via `pricing::cost_usd(...)`, and persisted via the existing `queries::update_cost(...)`. This recomputes from accumulated totals, not a delta-summed cost column, as required — reuses the exact same `all_session_token_totals` + `update_cost` pair that the startup backfill (below) also uses, so there's one cost-computation code path, not two that could drift.

4. **Startup cost backfill** (`src-tauri/src/lib.rs`, new `backfill_session_costs(conn: &rusqlite::Connection)`): called once in `setup()`, right after `db::open` and before `app.manage(...)`, using the still-owned `&Connection` (avoids re-locking a `Mutex` for a one-shot startup pass). Iterates `queries::all_session_token_totals`, computes cost per session, calls `queries::update_cost` unconditionally (no equality check — brief explicitly allowed skipping that given it's a small, one-time pass).

5. **Tauri commands** (`src-tauri/src/commands.rs`):
   - `list_sessions(db: State<'_, Db>) -> Result<Vec<queries::Session>, String>` — same lock/map_err pattern as `list_projects`.
   - `get_session_detail(db: State<'_, Db>, session_id: String) -> Result<Option<SessionDetail>, String>` where `SessionDetail { session: queries::Session, files_changed: Vec<queries::FileChanged> }` is a new `#[derive(Serialize)]` struct wrapping the query's `(Session, Vec<FileChanged>)` tuple into a named shape for the frontend.
   - Both registered in `lib.rs`'s `tauri::generate_handler![commands::list_projects, commands::list_sessions, commands::get_session_detail]`.
   - No frontend TS changes made (per "explicitly out of scope").

6. **Idle-session sweep** (`src-tauri/src/lib.rs`, new `spawn_idle_sweep(app_handle: tauri::AppHandle)`):
   - Constants: `IDLE_THRESHOLD_SECS: i64 = 120`, `SWEEP_INTERVAL_SECS: u64 = 20`.
   - `tauri::async_runtime::spawn(async move { ... })` with a `tokio::time::interval` loop (matches the brief's guidance — async interval loop, not a raw OS thread like the watcher's blocking `notify` loop).
   - Each tick: locks `Db` state, computes `now = chrono::Utc::now().timestamp()`, calls `queries::sessions_to_finalize(&conn, IDLE_THRESHOLD_SECS, now)`, then `queries::finalize_session(&conn, id)` for each returned id. Drops the lock, then — if any were finalized — emits `"data-changed"` with `{"entity": "session", "kind": "updated"}`, matching `watcher::mod.rs`'s `process_file` emit pattern so `useDataChangedEvents` picks it up unchanged.
   - Called from `setup()` via `spawn_idle_sweep(app.handle().clone())`, right after `watcher::start(app.handle().clone())` — same `AppHandle` acquisition pattern as the watcher.

## TDD evidence

### Pricing module (`src/cost/pricing.rs`)
- **RED**: Wrote the full `#[cfg(test)] mod tests` block (6 tests: exact match, longest-prefix match, sentinel non-billable, unrecognized→`_default`, `None`→`_default`, zero-tokens) against a stub `cost_usd` that always returned `0.0`. Ran `cargo test --lib cost::` — 3 of 6 failed for the expected reason (`expected 0.0928575, got 0`, `left: 0.0 right: 3.0` ×2); the other 3 trivially passed against the stub (sentinel/zero-token/prefix-vs-exact-both-zero comparisons happened to hold at `0.0`, which is fine — those three encode "sentinel and known models are non-billable/comparable," not "the real rate table is wired up").
- **GREEN**: Replaced the stub with the full lookup implementation (pricing table load, exact/prefix/sentinel/default lookup order, formula). Reran `cargo test --lib cost::` — all 6 passed.

### session_builder cost integration test
- **RED**: Added `ingesting_fixture_computes_nonzero_cost_consistent_with_opus_pricing` to `session_builder.rs`'s existing test module (same `in_memory_db()` + `ingest_fixture()` helpers as neighboring tests), asserting `cost_usd` on the fixture-ingested session is nonzero and equals the hand-computed `claude-opus-4-8` cost from the brief's formula. Ran it before wiring cost into `ingest_record()` — failed with `expected nonzero cost, got 0` (cost_usd stayed at its DB default of `0`).
- **GREEN**: Added the `session_token_totals` read-back + `cost::pricing::cost_usd` + `queries::update_cost` call inside `ingest_record()`. Reran — passed, along with all 5 other pre-existing `session_builder` tests (confirming no regression to token accumulation, file-changed recording, or replay-safety).

## Verification run and results

```
cd src-tauri && cargo test
```
- **27/27 tests passing** (20 pre-existing + 6 new `cost::pricing` tests + 1 new `session_builder` cost test), 0 failed, 0 ignored.
- Test names confirmed present: `cost::pricing::tests::{exact_match_uses_the_named_models_rates, longest_prefix_match_resolves_a_dated_suffix_to_the_known_family_rate, sentinel_model_is_non_billable_not_default, unrecognized_model_string_falls_back_to_default_rates, none_model_resolves_to_default_without_panicking, zero_tokens_yields_zero_cost_regardless_of_model}` and `parser::session_builder::tests::ingesting_fixture_computes_nonzero_cost_consistent_with_opus_pricing`.
- All pre-existing tests in `parser/claude_jsonl.rs` (13), `parser/session_builder.rs` (5 pre-existing + 1 new = 6), `watcher/tail.rs` (3) still pass unchanged.

```
cd src-tauri && cargo build
```
- Succeeds. Remaining warnings (unchanged in nature/count of *relevant* items — see below) are all pre-existing and out of this task's scope:
  - `unused import: IngestOutcome` (pre-existing, `parser/mod.rs`, predates this task)
  - `sessions_needing_summary`, `update_summary`, `sessions_needing_tags`, `update_tags` never used — these belong to the tagging/summarization tasks (explicitly out of scope here)
  - `field 'record_type' is never read` (pre-existing, `parser/claude_jsonl.rs`, predates this task)
- **Confirmed the four warnings the brief calls out as an acceptance signal are gone**: `list_sessions`, `get_session_detail`, `update_cost`, `all_session_token_totals` no longer appear anywhere in `cargo build` output — grepped the full output for each name to confirm.

## Files changed

- `src-tauri/resources/pricing.json` (new)
- `src-tauri/src/cost/mod.rs` (new)
- `src-tauri/src/cost/pricing.rs` (new)
- `src-tauri/src/commands.rs` (modified — added `list_sessions`, `get_session_detail`, `SessionDetail`)
- `src-tauri/src/db/queries.rs` (modified — added `session_token_totals`)
- `src-tauri/src/lib.rs` (modified — `mod cost;`, startup backfill, idle sweep, command registration)
- `src-tauri/src/parser/session_builder.rs` (modified — cost recompute wired into `ingest_record`, new test)

All of the above were `git add`-ed for tracking (not committed), per instructions. Note: `git status` also shows pre-existing uncommitted modifications to `src-tauri/src/parser/claude_jsonl.rs`, `src-tauri/src/watcher/tail.rs`, and an untracked `src-tauri/tests/` directory — these predate this session (Phase 1 work left uncommitted, per the brief's "current state") and I did not touch or stage them.

## Self-review findings

- Checked all four pricing lookup-order branches are implemented and independently tested (exact, prefix, sentinel, default) plus the `None` case — all present.
- Checked the delta-accumulation token path (`upsert_session`, untouched) and the recompute-from-totals cost path (`session_token_totals` → `cost_usd` → `update_cost`) don't drift: both the ingest-time call site (`session_builder.rs`) and the startup-backfill call site (`lib.rs`) call `cost::pricing::cost_usd` with parameters in the same order (`model, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens`), and both source their totals from the same `SessionTokenTotals` shape (`session_token_totals` / `all_session_token_totals`) — one code path, not two.
- Confirmed no scope creep: no tag classification, no AI summarization, no frontend TS changes (`src/lib/tauri.ts` and `src/lib/types.ts` untouched — verified via `git status`).
- Manually re-read `spawn_idle_sweep` and `backfill_session_costs` per the brief's allowance that these two pieces (AppHandle/live-timing) don't have a clean unit-test seam in this codebase — reasoning check: sweep locks `Db`, queries, finalizes, drops the lock *before* emitting (avoids holding the mutex across the `emit` call or an await point), matches `watcher::process_file`'s drop-then-emit ordering.
- File sizes stayed proportionate to the brief's descriptions (pricing.rs ~180 lines including tests; lib.rs's setup logic ~15-20 lines for the sweep as suggested) — no restructuring beyond what the brief asked for.

## Issues / deviations / concerns

None. Implementation matches the brief as written. One minor judgment call, called out in-line: the sentinel case (`0.0`) is implemented by resolving to a `ZERO_RATES` struct that flows through the normal formula rather than an early `return 0.0`, purely to keep one code path for the final cost computation — behaviorally identical to a hard-coded `0.0` return (verified by the `sentinel_model_is_non_billable_not_default` test using large token counts across all four token types, all of which zero out).
