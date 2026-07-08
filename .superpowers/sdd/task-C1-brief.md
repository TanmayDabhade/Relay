# Task C1: Transcript text extraction + tag auto-classification

Source: PLAN.md, "Ordered task breakdown" item 16, plus the relevant slice of §6 "AI summary generation" (only the text-extraction mechanics, which tag classification and the later summary pipeline both need — not the summarization API call itself, that's Task C2). SPEC.md line 104 names the exact tag taxonomy: `feature`, `bugfix`, `refactor`, `test`, `docs`, `infra`.

## Current state (read before writing anything)

- `src-tauri/src/parser/claude_jsonl.rs`'s `ParsedRecord` currently captures `record_type`, `cwd`, `git_branch`, `session_id`, `timestamp`, `model`, `usage`, `tool_uses` — but **not the raw text content** of `user` or `assistant` messages. Tag classification needs "the first user prompt's text"; the later summary pipeline (Task C2, not yours) also needs this plus "the last assistant message's text-only content blocks." Neither exists today — this is the gap you're closing.
- Real `user` records have `message: { role: "user", content: "<plain string>" }` (see `tests/fixtures/session_basic.jsonl`'s user record, or `src-tauri/src/parser/claude_jsonl.rs`'s own doc comments referencing real log inspection). Real `assistant` records have `message.content` as an **array** of blocks with `type` in `{"text", "thinking", "tool_use"}` (see the fixture's assistant records — e.g. the first one has both a `{"type":"text","text":"I'll create the main function."}` block and a `{"type":"tool_use", ...}` block).
- `queries.rs` already has `sessions_needing_tags(conn) -> Vec<String>` (sessions where `status='ended' AND tags IS NULL`) and `update_tags(conn, session_id, tags_json: &str)` — both unused so far (dead-code warnings). You'll wire these up.
- `sessions` table has `raw_log_path` (the absolute path to the session's `.jsonl` file) but does **not** store prompt/response text anywhere — by design, per this codebase's existing pattern (`cost_usd` is likewise recomputed from `raw_log_path`-adjacent stored token totals rather than persisted redundantly). Extracting first-user-text means re-reading that file, not adding new DB columns.
- The idle-sweep (`src-tauri/src/lib.rs`'s `spawn_idle_sweep`, built in a prior task) already runs every 20s, finalizing sessions whose `last_activity_at` is >120s old via `queries::finalize_session`. You will extend this same sweep tick to also compute and store tags — do not add a second `tokio::time::interval`.
- Existing test patterns to follow: `src-tauri/src/parser/claude_jsonl.rs`'s `#[cfg(test)] mod tests` (fixture-based, using `include_str!("../../tests/fixtures/session_basic.jsonl")`), `src-tauri/src/cost/pricing.rs`'s pure-function unit tests (no I/O, no DB).

## What to build

### 1. Extend `ParsedRecord` with a `text` field (`src-tauri/src/parser/claude_jsonl.rs`)

Add `pub text: Option<String>` to `ParsedRecord`. Populate it in `parse_line`:
- For `user` records: if `message.content` is a plain JSON string, that string. If `message.content` is present but *not* a string (e.g. an array — real logs sometimes have this for attachment/multi-part messages, per this file's existing defensive style), leave `text: None` rather than attempting to handle that shape — out of scope, and the existing "best-effort, `Option`-based" philosophy in this file means an unhandled shape degrades gracefully, it doesn't need full coverage.
- For `assistant` records: concatenate every `content[]` item where `"type" == "text"` (skip `"thinking"` and `"tool_use"` blocks — explicitly required, matches PLAN.md's "last assistant message's text-only content blocks (skip thinking/tool_use...)"), joined with a single space or newline (your call, document which). `None` if there are no text blocks (e.g. a tool-only turn).
- For `system` records: `None` always (system records never carry prompt/response text).

Add tests for this to the existing `#[cfg(test)] mod tests` block in this file (same fixture-based style as what's already there): confirm the fixture's user record's `text` matches its known content string, confirm an assistant record's `text` correctly skips a `tool_use` block and captures only the `text` block's content, confirm a `system` record's `text` is `None`.

### 2. New file `src-tauri/src/parser/transcript.rs` (add to `src-tauri/src/parser/mod.rs`: `pub mod transcript;` and re-export what's needed)

A single aggregation function that re-reads a session's raw `.jsonl` file and extracts what tag classification (this task) and summarization (Task C2, later) both need:

```rust
pub struct TranscriptExcerpts {
    pub first_user_text: Option<String>,
    pub last_assistant_text: Option<String>,
}

pub fn extract_excerpts(raw_log_path: &str) -> anyhow::Result<TranscriptExcerpts>
```

Implementation: read the file at `raw_log_path` (`std::fs::read_to_string` — the session has already been fully ingested by the time this runs, i.e. after `finalize_session`, so a plain synchronous full read is fine, no need for the watcher's incremental byte-offset tailing machinery here), split into lines, run each through the now-`text`-aware `parse_line`, and take the **first** `user`-type record's `text` (skip records where `text` is `None`) and the **last** `assistant`-type record's `text` (again skipping `None`s — a tool-only assistant turn shouldn't overwrite a real text answer from an earlier turn... actually re-read PLAN.md's literal wording: "last assistant message's text-only content blocks" — interpret this as: iterate assistant records in file order, and track the most recent one that has non-`None` text; a trailing tool-only assistant turn with no text block should not blank out the last *textful* one). Handle a missing/unreadable file gracefully (`Err` propagated via `anyhow`, caller decides what to do — don't panic).

This function does real file I/O, so test it against `tests/fixtures/session_basic.jsonl` directly (open the real fixture path via `include_str!`-free `std::fs::read_to_string` of the fixture, or write the fixture content to a temp file first — look at how `src-tauri/src/watcher/tail.rs`'s existing test module builds/cleans up temp files with its `TempFile` helper and reuse that pattern if it's easy to share, or write a small local equivalent — your call, don't over-engineer a shared abstraction for two files). Assert `first_user_text` matches the fixture's known user message ("Add a hello world main function and fix the greeting in foo()."), and `last_assistant_text` matches the fixture's last assistant record's text-only content — check the fixture file yourself (`tests/fixtures/session_basic.jsonl`) to get the exact expected strings rather than guessing.

### 3. New file `src-tauri/src/tags.rs` (add `mod tags;` to `src-tauri/src/lib.rs`)

Pure keyword-heuristic classifier, no I/O:

```rust
pub fn classify(prompt_text: &str) -> Vec<String>
```

Categories (exact taxonomy from SPEC.md line 104 — use these exact strings, lowercase, since they're what the frontend will eventually render as tag pills): `"feature"`, `"bugfix"`, `"refactor"`, `"test"`, `"docs"`, `"infra"`.

Design the keyword lists yourself (case-insensitive matching against `prompt_text`) — there's no canonical list in the plan to transcribe verbatim, use your judgment for reasonable signal words per category, e.g. (illustrative, not exhaustive — pick what you think is sensible):
- `bugfix`: words like "fix", "bug", "broken", "error", "crash", "fails"
- `feature`: "add", "implement", "create", "build", "new"
- `refactor`: "refactor", "cleanup", "simplify", "reorganize", "restructure"
- `test`: "test", "spec", "coverage"
- `docs`: "document", "readme", "docs", "comment"
- `infra`: "deploy", "ci", "docker", "config", "pipeline", "migration"

A prompt can match zero, one, or multiple categories (return all that match — don't force exactly one). `prompt_text: ""` or a string matching nothing returns an empty `Vec`. Never panic on empty/unusual input (e.g. very long strings, non-ASCII).

Write unit tests covering: a prompt that matches exactly one category, a prompt matching multiple categories, a prompt matching none (empty result), case-insensitivity (e.g. "FIX the bug" still matches `bugfix`), and an empty string input.

### 4. Wire into the idle sweep (`src-tauri/src/lib.rs`)

In the existing `spawn_idle_sweep` async loop, after the finalize step (or at the end of each tick — your call on ordering, but it must run *after* `finalize_session` calls in that tick, since tags only apply to `'ended'` sessions per `sessions_needing_tags`'s existing `WHERE status='ended'` clause), add: call `queries::sessions_needing_tags(&conn)`, and for each session id, look up its `raw_log_path` (there's no existing "get raw_log_path by id" query — you likely need a small one, or fetch it as part of a slightly different query; your call on the cleanest way, but don't do a wasteful `list_sessions()` full-table scan just to find one field for one id), call `parser::transcript::extract_excerpts(raw_log_path)`, then `tags::classify(excerpts.first_user_text.unwrap_or_default())`, serialize the resulting `Vec<String>` to a JSON array string (`serde_json::to_string`), and `queries::update_tags(&conn, &session_id, &tags_json)`. This should also naturally backfill tags for any session that was finalized before this feature existed (same "backfill via the existing needs-X query" pattern as the cost backfill and the sweep's own finalize step) — no separate startup pass needed since `sessions_needing_tags` already only returns rows still missing tags, and the sweep re-runs every 20s.

If `extract_excerpts` fails (unreadable file, e.g. deleted or moved) or returns no `first_user_text`, still call `update_tags` with an empty JSON array (`"[]"`) rather than leaving `tags` `NULL` forever — otherwise a session with an unreadable log would be retried every single sweep tick indefinitely. Log a warning in that case, don't let it stop the rest of the tick's tag processing for other sessions.

If any tags were actually written this tick, emit the same `"data-changed"` event shape the sweep's finalize step already emits (reuse/extend the existing `any_finalized` emit — you can fold "any tags written" into the same `if` condition rather than emitting twice per tick, your call on the cleanest way to combine them without over-complicating the loop).

## Explicitly out of scope for this task

- No AI summarization, no reqwest/API calls, no API key resolution — that's Task C2, which will build on your `parser::transcript::extract_excerpts` (specifically its `last_assistant_text` field, which this task doesn't otherwise use).
- No frontend changes — `SessionsView`/`SessionDetailModal` (built in a prior task) already render `session.tags` as `Pill`s if present and handle `null` gracefully; nothing there needs to change for tags to start showing up.
- No changes to the tag *taxonomy* beyond the six SPEC.md categories — don't invent additional categories.

## Verification

- `cargo test` (from `src-tauri/`) must pass, including your new tests for `ParsedRecord.text`, `transcript::extract_excerpts`, and `tags::classify`, and must not break any of the 27 existing tests.
- `cargo build` must succeed. The `sessions_needing_tags`/`update_tags` dead-code warnings should now be gone (confirms they're actually wired up).
- The sweep-integration piece (calling `sessions_needing_tags` → `extract_excerpts` → `tags::classify` → `update_tags` inside `spawn_idle_sweep`) doesn't have a clean unit-test seam in this codebase yet (same situation as the idle-sweep/cost-backfill logic in the prior task) — a manual reasoning check (re-read your own code) is acceptable for that specific piece; don't invent a Tauri-mocking test harness for it.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-C1-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
