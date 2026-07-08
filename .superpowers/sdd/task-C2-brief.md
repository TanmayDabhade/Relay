# Task C2: AI summary pipeline

Source: PLAN.md, "Ordered task breakdown" item 17, plus §6 "AI summary generation" (the summarization-specific parts — session-end detection and the 120s/20s sweep constants were already built in a prior task; this task is the trigger-dedup, prompt-construction, and API-call pieces on top of it).

## Current state (read before writing anything)

- `src-tauri/src/lib.rs`'s `spawn_idle_sweep` (built across two prior tasks) already runs a `tokio::time::interval` every `SWEEP_INTERVAL_SECS` (20s), each tick: finalizing idle sessions (`queries::finalize_session`, marking `status='ended'`), then classifying tags for any `status='ended' AND tags IS NULL` session (`queries::sessions_needing_tags` → `parser::transcript::extract_excerpts` → `tags::classify` → `queries::update_tags`), then emitting one combined `"data-changed"` event if anything changed. **You are extending this same tick with a third step — do not add a second `tokio::time::interval`.**
- `parser::transcript::extract_excerpts(raw_log_path: &str) -> anyhow::Result<TranscriptExcerpts>` (built in the immediately-prior task, Task C1) already exists and returns `TranscriptExcerpts { first_user_text: Option<String>, last_assistant_text: Option<String> }` — re-reads the session's raw `.jsonl` file, gives you the first user prompt and the last assistant turn that actually had text content (correctly skipping trailing tool-only turns). **Reuse this directly** — do not write a second file-parsing routine.
- `queries.rs` already has `sessions_needing_summary(conn) -> Vec<String>` (sessions where `status='ended' AND summary IS NULL`) and `update_summary(conn, session_id, summary: &str)` — both unused so far (dead-code warnings). You're wiring these up.
- `src-tauri/src/db/queries.rs` has `session_raw_log_path` (added in Task C1 — a single-row lookup, `WHERE id = ?1`, not a full scan) if you need a session's `raw_log_path` outside of `get_session_detail`. `get_session_detail(conn, session_id) -> Option<(Session, Vec<FileChanged>)>` gives you the session's `files_changed` rows if you need the list of distinct file paths touched.
- `src-tauri/src/cost/pricing.rs` loads `resources/pricing.json` once via `OnceLock` and exposes a private rates table keyed by model string (`claude-opus-4-8`, `claude-sonnet-5`, `claude-haiku-4-5-20251001`, `_default`). PLAN.md's §6 says the summarization model string should come "from pricing.json (single source of truth)" — i.e. don't hardcode `"claude-haiku-4-5-20251001"` a second time in this task's code. Add a small public accessor to `cost/pricing.rs`, e.g. `pub fn haiku_model_id() -> Option<&'static str>` that finds the one key in the loaded table containing `"haiku"` (a `.keys().find(|k| k.contains("haiku"))`-style lookup over the already-loaded table — no new JSON parsing). If no haiku entry is found (shouldn't happen given the bundled `pricing.json`, but the function should still not panic), return `None` and have the caller treat that the same as "no API key" — skip summarization, log once, keep going.
- `reqwest` is already a dependency with `json` + `rustls-tls` features (no `blocking` feature) — use it async, inside the same `tauri::async_runtime::spawn` pattern the idle sweep and (per-session) summary tasks will use.
- Existing patterns: `cost/pricing.rs`'s `log_unknown_model_once` / `claude_jsonl.rs`'s `log_unknown_type_once` (both `OnceLock<Mutex<HashSet<String>>>` rate-limited-once-per-value logging) — reuse this idea for "summarization disabled, no API key" (log once at startup or on first sweep tick that would have needed it, not every 20s forever).

## What to build

### 1. API key resolution (`src-tauri/src/summarize/mod.rs`, new module — add `mod summarize;` to `lib.rs`)

At app startup (in `setup()`, once), resolve the Anthropic API key in this exact order:
1. `std::env::var("ANTHROPIC_API_KEY").ok()`, treating an empty string the same as unset.
2. Otherwise, read `app_data_dir()/config.json` (same `app_data_dir()` the DB already uses — see how `lib.rs`'s `setup()` calls `app.path().app_data_dir()?`), parse it as `{"api_key": "..."}` (a minimal, permissive shape — if the file doesn't exist, isn't valid JSON, or lacks `api_key`, treat as "no key," don't error the whole app startup over it).
3. If neither yields a key: summarization is disabled for this app run — the sweep's tag/finalize steps and everything else must continue working normally, just skip the summary step (with a once-logged note, not silence, so it's discoverable in logs why summaries never appear).

**Never log the key value itself** — not in a debug format, not in an error message that might echo the input, nowhere. Store the resolved key as Tauri-managed state (e.g. `app.manage(summarize::ApiKeyState(Option<String>))`), read once at startup, never re-read from env/disk afterward (matches PLAN.md's "read once at startup" and the existing `cost::pricing` "load once via OnceLock" precedent, though this one varies per-app-run so it should be Tauri state, not a `OnceLock` — since a future Phase 4 settings UI will want to update it without a restart, per the plan's forward note "write the resolution code to check both so Phase 4 just adds a writer").

### 2. In-flight dedup set (`src-tauri/src/summarize/mod.rs`)

A `Mutex<HashSet<String>>` of session ids currently being summarized, managed as Tauri state (e.g. `app.manage(summarize::InFlight(Mutex::new(HashSet::new())))`). Purpose: the sweep tick fires every 20s; a slow/hanging API call must not cause the *same* session to get a second concurrent summarization task spawned on the next tick before the first one finishes. Insert the session id when a task is spawned for it; remove it when that task finishes (success or failure) — a task must always clean up after itself, including on early-return error paths (a `struct`-with-`Drop` guard, or careful manual removal on every exit path — your call, but verify every code path removes the id, a leaked id would permanently block that session from ever getting summarized again this app run).

### 3. Prompt construction (`src-tauri/src/summarize/prompts.rs`, new file)

Given a session id, build the final prompt string:
- `parser::transcript::extract_excerpts(raw_log_path)` for `first_user_text`/`last_assistant_text`.
- The distinct list of file paths touched (from `files_changed`, via `get_session_detail` or a leaner query — your call, but don't fetch things you don't need).
- **Truncate `first_user_text` and `last_assistant_text` independently to ~400 tokens each.** There's no tokenizer dependency in this codebase and none should be added for this — approximate with a character budget using the common ~4 characters/token heuristic (~1600 characters), truncating at a valid UTF-8 char boundary (not a raw byte index — Rust will panic on an invalid boundary; use `char_indices()` or `.chars().take(n)` rather than naive byte slicing) and appending a truncation marker (e.g. `"…"`) when truncation actually occurred. Document this as an approximation in a comment — it's deliberately not exact token counting.
- Compose a final prompt along these lines (exact wording is your call, but it must include all three pieces PLAN.md names): the user's request, the assistant's final response, a bullet list of distinct file paths touched, and an instruction to produce **one sentence, ≤15 words, focused on what changed** (not a general chat response, not multiple sentences, not a restatement of the instruction itself).
- If `first_user_text` is `None` (e.g. unreadable log, or a session with no captured user text) treat this as "nothing to summarize" — the caller should skip the API call entirely rather than sending a near-empty prompt (log once at debug/info level, leave `summary` `NULL`, do not retry-loop forever on it the way the tags step had to guard against — for summaries it's fine to just leave it `NULL` permanently rather than writing a placeholder, since `sessions_needing_summary`'s `WHERE summary IS NULL` will keep it eligible for retry indefinitely, and the in-flight-set + one-spawn-per-tick design already prevents that from being a busy-loop; unlike tags there's no equivalent to `"[]"` sentinel value that makes sense to write here).

### 4. The API call (`src-tauri/src/summarize/mod.rs`)

`reqwest::Client` POST to `https://api.anthropic.com/v1/messages` with headers `x-api-key: <resolved key>`, `anthropic-version: 2023-06-01`, `content-type: application/json`, and a JSON body:
```json
{
  "model": "<from cost::pricing::haiku_model_id()>",
  "max_tokens": 60,
  "messages": [{"role": "user", "content": "<constructed prompt>"}]
}
```
(`max_tokens: 60` is a reasonable bound for "one sentence, ≤15 words" with tokenization overhead — adjust if you have a good reason, just keep it small/bounded, not the default-unbounded shape.)

On a `200` response: parse the response JSON, extract `content[0].text` (Anthropic's Messages API response shape — a top-level `content` array of blocks, each with a `type`/`text`), trim whitespace, call `queries::update_summary(conn, session_id, &text)`, and this session's contribution to the tick's combined `data-changed` emit (fold into the same emit-gating pattern the finalize/tag steps already use — one combined emit per tick if anything changed, not a fourth separate emission path).

On any failure (non-`200` status, network error, malformed/unexpected response JSON): log a warning (never logging the API key), leave `summary` as `NULL` — `sessions_needing_summary` will naturally pick it up again next tick since nothing was written. This is explicitly the plan's intended behavior ("retried next sweep — never blocks the rest of the pipeline"), not a bug to work around with retry/backoff logic of your own invention — don't add exponential backoff or a retry-limit counter, that's out of scope for this phase.

### 5. Wire into the sweep (`src-tauri/src/lib.rs`)

After the tag-classification step in the same tick: if an API key is available (from step 1's resolved state) and `cost::pricing::haiku_model_id()` returns `Some`, call `queries::sessions_needing_summary(&conn)`, filter out ids already in the in-flight set (step 2), and for each remaining id: insert into the in-flight set, then `tauri::async_runtime::spawn` a task that builds the prompt (step 3), makes the API call (step 4), updates the DB on success, and removes itself from the in-flight set when done. If no API key is available, skip this step entirely (log once, not every tick).

Because the API call is async and the DB `Connection` is behind a `Mutex` (not `Send`-safe to hold across an `.await`), **each spawned summarization task must acquire the DB lock only for the brief synchronous parts** (reading data needed for the prompt, and writing the result afterward) and release it before/during the `.await` on the network call — do not hold `db.0.lock()` across the `reqwest` call. Look at how `spawn_idle_sweep`'s existing tick already drops its lock before the (synchronous, but analogous in spirit) emit call, and follow the same discipline here, more strictly, since this one has a real `.await` point in the middle.

## Explicitly out of scope for this task

- No retry/backoff strategy beyond "next sweep tick tries again if `summary` is still `NULL`."
- No Settings UI for entering an API key (Phase 4, per the plan) — env var + `config.json` resolution only.
- No frontend changes — `SessionDetailModal` (built in a prior task) already renders `session.summary` if present and handles `null` gracefully.
- No tokenizer dependency — the ~400-token truncation is an explicit character-count approximation, not exact.

## Verification

- `cargo test` (from `src-tauri/`) must pass, including new tests for the prompt-construction/truncation logic (pure function, easily testable without network — e.g. confirm a >1600-character `first_user_text` gets truncated to roughly the right length with a truncation marker, confirm a short one passes through unchanged, confirm the constructed prompt contains all three required pieces) and for `cost::pricing::haiku_model_id()` (confirms it finds the right key from the real bundled `pricing.json`). Must not break any of the 41 existing tests.
- **Do not write a test that makes a real network call to the Anthropic API** — the API-call function itself (the actual `reqwest` POST) is not unit-testable without either a live key or a mocking setup this codebase doesn't have; a manual reasoning check (re-read your own code for the lock-discipline requirement above, and for the response-parsing/error-handling paths) is acceptable for that specific piece, same as the idle-sweep/tag-sweep integration pieces in prior tasks.
- `cargo build` must succeed. The `sessions_needing_summary`/`update_summary` dead-code warnings should now be gone.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-C2-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
