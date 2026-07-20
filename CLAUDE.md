# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Relay — a local-first desktop control plane for AI coding agents ("Datadog for AI coding agents"). It watches session log files that CLI coding agents write to disk (Claude Code's `~/.claude/projects/**/*.jsonl` is the primary, fully-supported source; Codex, Gemini CLI, and Cursor Agent are also tailed but their log formats are best-effort/unverified — see doc comments in `src-tauri/src/parser/`), parses them defensively, and shows a live dashboard of every project, session, and dollar spent. Everything stays on the user's machine: one process, one local SQLite database, no server, no telemetry.

Built with Tauri 2 (Rust backend) + React 19/TypeScript (frontend). macOS only for now (the terminal-attach feature shells out to Terminal.app + AppleScript).

The repo also contains `landing-page/`, a separate Next.js marketing site with its own `CLAUDE.md`/`AGENTS.md` — treat it as an independent project, not part of the Tauri app.

## Agent operating rules

- **Never run the dev server or a build yourself** — no `npm run tauri dev`, `npm run dev`, `npm run build`, `vite build`, `next dev`, `next build`, `cargo build`, `cargo run`, etc. This is strictly prohibited, no exceptions.
- Whenever you need to see a build/dev-server/runtime result — compiler output, console logs, a rendered screen, whether something actually works — **ask the user to run it and paste back the output**, instead of running it yourself. `cargo check` and `cargo test` (and their frontend equivalents, e.g. `tsc -b` for a typecheck) are fine to run yourself; those aren't the dev server or a build.
- Don't spend time writing spec/plan/progress docs or re-confirming completed decisions with the user unless asked. Move straight to implementation after a design is agreed on.
- Don't narrate or log individual tool calls into the session transcript — report results, not a play-by-play of what was invoked.

## Commands

Root app (Tauri + React), run from the repo root:

```bash
npm install
npm run tauri dev     # starts Vite + launches the native window, hot-reload on frontend
npm run build          # tsc -b && vite build (frontend only, no bundling into app)
npm run lint            # oxlint
```

Rust backend, run from `src-tauri/`:

```bash
cargo test                                  # full suite (parser, watcher, cost, tags, summarize, activity, db)
cargo test -p app <test_name>                # single test by name
cargo test --lib db::queries                 # tests in one module
cargo check                                  # fast type-check without building
```

Rust test coverage lives inline as `#[cfg(test)] mod tests` in: `commands.rs`, `tags.rs`, `watcher/tail.rs`, `cost/pricing.rs`, `activity.rs`, `db/queries.rs`, `summarize/prompts.rs`, and each `parser/*.rs` file. Fixtures for parser tests are under `src-tauri/tests/fixtures/`.

There is no JS/TS test runner configured — verification for frontend changes is `tsc -b` (typecheck only; do not run the full `npm run build` or `npm run tauri dev` yourself, see "Agent operating rules" above) plus asking the user to exercise it manually.

Landing page (`landing-page/`), run from that directory: `npm run dev` / `npm run build` / `npm run lint` (Next.js + ESLint). It has its own Supabase-backed waitlist API (`app/api/`, `lib/supabase.ts`) — see `landing-page/CLAUDE.md` (which just points to `AGENTS.md`) before editing it, and note its `AGENTS.md` warns this Next.js version has breaking API changes from training data.

## Architecture

### Data flow: filesystem → SQLite → UI

1. **`src-tauri/src/watcher/`** — a dedicated OS thread runs a debounced `notify` watcher (500ms) over each known agent's log root (`~/.claude/projects`, `~/.codex/sessions`, `~/.gemini/tmp`, `~/.cursor/logs`). On startup it backfills every pre-existing file, then tails live appends. `watcher/tail.rs` does incremental byte-offset reads so a growing log is never re-parsed from scratch. New agent sources are added by extending the `AGENT_SOURCES` table in `watcher/mod.rs` (root dir fn + extension + parse_line fn) — the debounce/backfill/dedup logic is agent-agnostic.
2. **`src-tauri/src/parser/`** — one module per agent's log format (`claude_jsonl.rs` is the only one verified against real output; `codex_jsonl.rs`, `gemini_log.rs`, `cursor_jsonl.rs` are best-effort). All parsing is `Option`-based and must never panic on malformed/unrecognized input — logs are undocumented internal formats subject to change upstream. `session_builder::ingest_record` turns a `ParsedRecord` into DB writes (project/session upsert) and reports what changed via `IngestOutcome` so the caller knows whether to emit a UI event.
3. **`src-tauri/src/db/`** — SQLite via `rusqlite`, WAL mode, one shared `Connection` behind a single `Mutex` (`db::Db`), managed as Tauri state. Schema evolves through numbered files in `src-tauri/migrations/` applied via `rusqlite_migration` at startup (`db::open`) — never edit an existing migration file, add a new one and register it in `db::open`. `db/queries.rs` holds all SQL.
4. **`src/`** (React frontend) — `src/lib/tauri.ts` is the single typed boundary over `invoke()` calls into every `#[tauri::command]`; nothing else should call `invoke` directly. One view per nav item under `src/views/`, React Query for data fetching/caching, and a single Tauri event `data-changed` (emitted by the watcher, the idle sweep, and command handlers that mutate state) drives cache invalidation everywhere via `src/hooks/useDataChangedEvents.ts` — there is no polling.

### The idle-session sweep (`spawn_idle_sweep` in `src-tauri/src/lib.rs`)

A tokio interval (every `SWEEP_INTERVAL_SECS`) finalizes sessions idle longer than `IDLE_THRESHOLD_SECS`, then runs tag classification and AI summarization for newly-`ended` sessions. This function's structure is deliberate and important to preserve when touching it: it runs in three phases — **gather** (DB lock held, cheap queries only), **compute** (lock released, does file I/O: reading/parsing raw session logs), **write** (lock re-acquired briefly). Never hold the shared `Mutex<Connection>` across a file read or an `.await` — a slow log read or hung API call would otherwise stall every other DB access (every UI command, the watcher, the next tick) for as long as it takes. Each spawned summary task re-acquires the lock independently for its own brief DB-only steps rather than holding a lock passed in from the caller.

### Cost tracking

`src-tauri/src/cost/pricing.rs` computes `cost_usd` from token counts against a bundled `src-tauri/resources/pricing.json` lookup table, keyed by model name. Tokens are accumulated per `(session, model, cache-TTL)` in the `session_model_usage` table (migration `0007`), and a session's cost is the SUM of `pricing::cost_usd` over those buckets — so a session that spanned more than one model (a sub-agent on a cheaper model, or a mid-session `/model` switch) bills each model's tokens at its own rate, and 1-hour cache writes bill at their higher rate (`cache_write_1h`) than 5-minute ones. The `sessions` table keeps aggregate token columns for display; `session_model_usage` is consulted only for costing. Costs are recomputed once at every startup (`backfill_session_costs`, via `queries::all_session_costs`) so pricing-table updates retroactively correct historical sessions, and recomputed live (`queries::session_cost`) as a session's token counts grow.

### Tag classification & AI summaries

`src-tauri/src/tags.rs` classifies a session (`feature`/`bugfix`/`refactor`/`test`/`docs`/`infra`) from a keyword heuristic over the first user prompt — no API call. `src-tauri/src/summarize/` generates a one-sentence summary via the Anthropic API (Claude Haiku, resolved by model name containing "haiku" in `pricing.json`) once a session ends; this is fully optional and silently (but only once — see `log_summarization_disabled_once`) skipped if no API key is resolved. Key resolution order (`summarize::resolve_api_key`, evaluated once at startup, never re-read): `ANTHROPIC_API_KEY` env var, then `api_key` in `<app_data_dir>/config.json`.

### Kanban board

`get_board`/`create_card`/`move_card`/`update_card`/`delete_card`/`link_session_to_card`/`create_column`/`rename_column` commands back a per-project kanban board (`src/components/board/`). A session finalizing always moves its linked card to the `review` column (`db::queries::sync_card_for_session`), overriding any manual drag — this is intentional, not a bug to "fix" if a card seems to jump columns.

### Terminal attach (macOS only, `src-tauri/src/terminal.rs`)

`launch_or_attach_session` drives Terminal.app via AppleScript (`resources/attach_session.applescript`) to find an existing `claude` tab for a card's project and paste its prompt in, or open a new window. The prompt is delivered via clipboard paste (not simulated keystrokes) because a typed multi-line prompt would submit early at the first newline in Claude Code's interactive input. Requires macOS Accessibility + Automation permissions.

### Reports

`generate_report`/`export_report`/`export_transcript` commands (`src/views/ReportView.tsx`) produce Markdown written to the user's Downloads folder, paired with a `reveal_in_finder` follow-up command — this is the pattern to follow for any future "export to file" feature.

## Working conventions in this codebase

- Rust doc comments here routinely explain *why*, including concurrency/locking discipline and failure-mode reasoning (see `lib.rs`, `watcher/mod.rs`) — read them before changing the functions they're attached to, and preserve that level of reasoning in new code touching shared state.
- Parser and activity code must degrade gracefully, never panic, on missing git, malformed logs, or absent directories — errors become logged warnings and empty/zero results, not propagated failures that break the UI.
- `docs/PLAN.md` and `docs/SPEC.md` describe the original phased roadmap (Relay is currently past Phase 1/2 — multi-agent parsing and the kanban board, both mentioned as "later phases" in `README.md`, already exist in code) — treat the README's "Status" section as stale relative to what's actually implemented; check `src-tauri/src/parser/` and `src/components/board/` directly.
