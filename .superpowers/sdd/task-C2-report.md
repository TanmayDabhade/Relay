# Task C2: AI summary pipeline — report

Status: **DONE**

## What was implemented

1. **API key resolution** (`src-tauri/src/summarize/mod.rs`, `resolve_api_key`)
   - Order: `ANTHROPIC_API_KEY` env var (empty string treated as unset) → `app_data_dir()/config.json`'s `"api_key"` field (permissive: missing file / invalid JSON / missing or empty field all resolve to `None`, never an error) → `None`.
   - Resolved once in `setup()`, stored as Tauri-managed state `summarize::ApiKeyState(pub Option<String>)`. Never re-read from env/disk afterward. Never logged anywhere (grepped for every `api_key` reference — confirmed no `log::`/`{:?}` touches the value itself).
   - If no key resolves, `setup()` calls `summarize::log_summarization_disabled_once(...)` once.

2. **In-flight dedup set** — `summarize::InFlight(pub Mutex<HashSet<String>>)`, managed as Tauri state. Ids are inserted synchronously in `lib.rs::spawn_summary_tasks` (while the tick's DB lock is held, no `.await` involved) right before each task is spawned. Removal is via an RAII guard, `summarize::InFlightGuard`, constructed as the very first statement inside each spawned task's async block and held for the block's full lifetime — Rust runs its `Drop` on every exit path (normal completion, every early `return` on a skip/error branch, or an unexpected panic-unwind), so there is no manual per-branch bookkeeping to get wrong.

3. **Prompt construction** (`src-tauri/src/summarize/prompts.rs`, new file)
   - `truncate_for_prompt(text: &str) -> String`: character-budget truncation (1600 chars ≈ 400 tokens at ~4 chars/token, documented as a deliberate approximation — no tokenizer dependency added). Cuts via `chars()`/`take()`, never a raw byte index, so it can't panic on a multi-byte UTF-8 boundary. Appends `'…'` only when truncation actually happened.
   - `build_prompt(&PromptInputs) -> String`: composes the final prompt from `first_user_text`, `last_assistant_text` (or a placeholder if `None`), and a bullet list of distinct file paths (or a placeholder if empty), plus an instruction for one sentence, ≤15 words, focused on what changed.
   - `prompt_for_session(conn, session_id) -> anyhow::Result<Option<String>>`: the DB/filesystem glue — looks up `raw_log_path` via `queries::session_raw_log_path`, calls `parser::extract_excerpts` (reused directly, no reimplementation), returns `Ok(None)` if there's no `raw_log_path` row or no `first_user_text` (nothing worth summarizing — caller skips the API call and leaves `summary` `NULL` permanently retryable, per the brief), otherwise pulls distinct `file_paths` from `queries::get_session_detail` and calls `build_prompt`.

4. **The API call** (`summarize::call_anthropic_api`) — `reqwest::Client` POST to `https://api.anthropic.com/v1/messages` with `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json` headers and `{"model", "max_tokens": 60, "messages": [...]}` body. `model` comes from the new `cost::pricing::haiku_model_id()` accessor (see below) — not hardcoded a second time. On non-200 status: `Err` with just the status code (never the response body, to keep the error surface minimal). On 200: parses `content[0].text`, trims, returns it. Malformed/unexpected shape → `Err` via `.ok_or_else`. No retry/backoff logic anywhere in this function.

5. **`cost::pricing::haiku_model_id()`** (new, in `src-tauri/src/cost/pricing.rs`) — `pricing_table().keys().find(|k| k.contains("haiku")).map(|k| k.as_str())`. No new JSON parsing; reuses the existing `OnceLock`-loaded table. Returns `None` (never panics) if no such key exists.

6. **Sweep wiring** (`src-tauri/src/lib.rs`, `spawn_summary_tasks`, called from `spawn_idle_sweep`'s tick right after the tag-classification step, before `drop(conn)`) — reads `ApiKeyState`; if `None`, returns (already logged once at startup). Reads `cost::pricing::haiku_model_id()`; if `None`, logs once and returns. Queries `sessions_needing_summary(&conn)`, filters out ids already in `InFlight`, inserts the remainder into `InFlight`, then for each spawns a `tauri::async_runtime::spawn` task that: builds the prompt under a freshly-reacquired, block-scoped DB lock (dropped before returning from that block); calls `call_anthropic_api(...).await` with the lock **not** held; on success reacquires the lock only to call `update_summary`, then emits `"data-changed"` (same event name/payload shape the tick's own finalize/tag emit uses — issued from the task itself since these tasks routinely outlive the synchronous tick that spawned them, so they cannot be folded into that tick's single emit literally); on any failure, logs a warning and leaves `summary` `NULL` (next tick retries automatically, since `sessions_needing_summary`'s `WHERE summary IS NULL` still matches and the id has been removed from `InFlight` by then).

No second `tokio::time::interval` was added — this all rides the existing sweep's interval, as required.

## TDD evidence (RED/GREEN)

### `cost::pricing::haiku_model_id()`
- **RED**: temporarily replaced the implementation with `None // TEMP: RED check` and ran the new test alone:
  ```
  thread 'cost::pricing::tests::haiku_model_id_finds_the_bundled_haiku_entry' panicked:
  assertion `left == right` failed
    left: None
   right: Some("claude-haiku-4-5-20251001")
  test result: FAILED. 0 passed; 1 failed
  ```
- **GREEN**: restored the real implementation, reran — `test cost::pricing::tests::haiku_model_id_finds_the_bundled_haiku_entry ... ok`.

### Truncation logic (`summarize::prompts::truncate_for_prompt` / `build_prompt`)
Written test-first as a full suite (9 tests) covering: short text passes through unchanged; text at exactly the 1600-char budget is unchanged; text over budget is truncated to 1600 chars + 1 marker char; multi-byte UTF-8 input (`"🦀".repeat(2000)`) truncates without panicking and still lands on a char boundary; empty string stays empty; `build_prompt` includes user text, assistant text, and the file-path bullet list (all three required pieces); `build_prompt` handles `None` assistant text and an empty file list without panicking; `build_prompt` truncates long inputs before composing (the untruncated 5000-char string does not appear verbatim in the final prompt); `prompt_for_session` returns `Ok(None)` for an unknown session id against a freshly-migrated in-memory DB. All 9 pass. (I did not do a separate strip-and-reimplement RED cycle for every one of these individually — for `haiku_model_id` I ran a literal RED→GREEN cycle as the representative demonstration since the brief calls it out by name; the prompts tests were written before I ran `cargo test` for the first time on this module, i.e. they never observed a passing state against a stub, but they were written to fail without the real logic — e.g. the multibyte/truncation-marker/exact-budget assertions are specific enough that a naive or missing implementation would fail them.)

## Verification run

```
cd src-tauri && cargo test
```
Result: **51 passed; 0 failed; 0 ignored** (41 pre-existing + 10 new: 1 `haiku_model_id` test + 9 `summarize::prompts` tests). Only warning: a pre-existing, unrelated `unused import: IngestOutcome` in `src/parser/mod.rs` (not touched by this task, present before this work started).

```
cd src-tauri && cargo build
```
Result: **Finished `dev` profile** successfully. Same single pre-existing warning, nothing else. Confirmed via `grep -i "sessions_needing_summary\|update_summary\|dead_code\|never used"` against the build output that those two dead-code warnings are gone.

No test makes a real network call to `api.anthropic.com` — `call_anthropic_api` itself has no automated test; verified by manual re-read (see Self-review below).

## Files changed

- `src-tauri/src/lib.rs` — added `mod summarize;`, API-key/in-flight-set/HTTP-client state management in `setup()`, and the new `spawn_summary_tasks` function wired into `spawn_idle_sweep`'s existing tick.
- `src-tauri/src/cost/pricing.rs` — added `pub fn haiku_model_id() -> Option<&'static str>` and its test.
- `src-tauri/src/summarize/mod.rs` — new file: `ApiKeyState`, `InFlight`, `HttpClient`, `InFlightGuard`, `resolve_api_key`, `log_summarization_disabled_once`, `call_anthropic_api`.
- `src-tauri/src/summarize/prompts.rs` — new file: `truncate_for_prompt`, `PromptInputs`, `build_prompt`, `prompt_for_session`, and their tests.

No frontend changes, no `Cargo.toml`/`Cargo.lock` changes (no new dependencies — confirmed via `git diff --stat` showing no changes to either file), no Settings UI, no tokenizer dependency.

## Self-review findings

- **Lock discipline (manual reasoning check, per the brief's allowance for this piece)**: In `spawn_summary_tasks`'s per-session spawned task, the DB lock is acquired inside a bare `{ ... }` block for `prompt_for_session` and dropped at that block's closing brace — strictly before the `.await` on `call_anthropic_api`. The lock is reacquired only after that `.await` resolves, again inside a bare block, to call `update_summary`, and dropped again before the `emit` call. At no point does a `MutexGuard<Connection>` cross an `.await` point. I traced every line between the two `.await`-adjacent blocks and confirmed no `conn`/`db` binding is held live across the `summarize::call_anthropic_api(...).await` call.
- **In-flight-set cleanup on every exit path**: `InFlightGuard` is constructed as the first statement in the async block and relies on Rust's unconditional `Drop`-on-scope-exit — I traced all four exit points (the two early `return`s on prompt-build failure/nothing-to-summarize, the `Ok`/`Err` arms of the API-call match) and confirmed the guard is in scope and will drop in every one of them, plus the implicit panic-unwind case.
- **Emit semantics deviation from a literal reading of the brief**: the brief says to "fold into the same emit-gating pattern the finalize/tag steps already use — one combined emit per tick." Because summarization tasks are spawned and complete asynchronously, often well after the tick that spawned them has already finished its own synchronous work and issued its own emit, a literal single combined-emit-per-tick is not achievable — the summary write may land seconds after the tick's own emit fired. I instead had each task emit its own `"data-changed"` event (same event name and payload shape as the tick's combined emit) immediately after a successful `update_summary`. This preserves the frontend contract (any `"data-changed"` event triggers a refetch) without inventing a new/fourth event type, which I believe satisfies the intent of "not a fourth separate emission path" even though it isn't literally batched into the tick's single emit. Flagging this as the one place I made a judgment call beyond the brief's literal wording.
- Confirmed no test makes a network call — grepped for `reqwest`/`call_anthropic_api` usage in `#[cfg(test)]` blocks, found none.
- Confirmed the API key is never logged — grepped every occurrence of `api_key` across `summarize/mod.rs` and `lib.rs`; none appear inside a `log::`/format-macro call.
- Confirmed no retry/backoff logic was added anywhere in the new code — failures simply return, leaving `summary` `NULL` for the next tick.

## Issues / deviations / concerns

- The one deviation noted above (per-task emit instead of a literal single per-tick combined emit) is the only concern. I believe it's the correct interpretation given the async nature of the work, but flagging it explicitly since the brief's wording could be read more literally.
- Did not commit anything, per instructions. Ran `git add` on the changed/new files only, for tracking.
