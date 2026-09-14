Relay is a macOS-first, local desktop control plane for AI coding agents—essentially “Datadog for coding-agent sessions.” It passively watches local Claude Code, Codex, Gemini,
  and Cursor logs, converts them into a normalized SQLite model, and presents projects, sessions, costs, diffs, reports, and Kanban state. It can also actively launch those CLIs
  and maintain Relay-owned, resumable agent conversations.

  The architecture is thoughtful and unusually substantial for an interview repository: roughly 10.6k lines of Rust, 4k lines of frontend TypeScript, 3.4k lines of CSS, 9
  migrations, and 127 Rust tests. The strongest implementation is Claude Code ingestion. The other passive log adapters are explicitly best-effort.

  I excluded landing-page from this analysis.

  ## System architecture

  Passive observability

  Agent-owned log files
    └─ filesystem watcher + incremental tailer
        └─ provider-specific parser
            └─ normalized ParsedRecord
                └─ session_builder
                    └─ SQLite
                        └─ Tauri commands
                            └─ React Query
                                └─ Dashboard / Projects / Sessions / Reports


  Active agent dispatch

  Dashboard or Kanban card
    └─ dispatch_task
        └─ local Claude/Codex/Gemini/Cursor CLI process
            └─ streamed provider JSON
                └─ provider normalizer
                    └─ durable dispatch turns/events
                        └─ dispatch-event
                            └─ Agent work chat UI

  The important architectural split is:

  - Passive ingestion observes sessions started independently in terminals.
  - Active dispatch starts a CLI process owned by Relay and stores its conversation.
  - A pending-launch handshake later correlates the active dispatch with the native session log and its Kanban card.

  The application startup and module wiring live in src-tauri/src/lib.rs:22. The frontend is mounted in src/main.tsx:1, with application-level view switching in src/App.tsx:18.

  ## Technology choices

  The desktop frontend uses:

  - React 19 and TypeScript 6.
  - Vite 8.
  - TanStack React Query for fetching and cache invalidation.
  - Tauri’s typed invoke and event APIs.
  - Inter and IBM Plex Mono bundled through @fontsource.
  - Plain CSS with shared design tokens—no component framework.
  - No React Router; navigation is an in-memory useState switch.

  The backend uses:

  - Tauri 2 and Rust.
  - SQLite through rusqlite, bundled with the application.
  - notify plus a 500 ms debouncer for log watching.
  - Tokio for idle sweeps and summary tasks.
  - similar for line diffs.
  - reqwest for optional Anthropic summaries.
  - Native std::process::Command for Git, editor, browser, Terminal automation, and agent CLI processes.

  This is intentionally one process with no Node sidecar or web server.

  ## Passive ingestion pipeline

  ### 1. Agent source discovery

  src-tauri/src/watcher/mod.rs:45 defines four sources:

   Agent          Assumed location                 Confidence
  ━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   Claude Code    ~/.claude/projects/**/*.jsonl    Verified against real logs
  ─────────────  ───────────────────────────────  ─────────────────────────────────────────────────
   Codex          ~/.codex/sessions/**/*.jsonl     Best-effort
  ─────────────  ───────────────────────────────  ─────────────────────────────────────────────────
   Gemini         ~/.gemini/tmp/**/*.jsonl         Best-effort and may use the wrong file strategy
  ─────────────  ───────────────────────────────  ─────────────────────────────────────────────────
   Cursor         ~/.cursor/logs/**/*.jsonl        Best-effort

  At startup, Relay recursively backfills every existing file and then establishes one recursive debounced watcher.

  ### 2. Incremental tailing

  src-tauri/src/watcher/tail.rs:12 stores a byte offset and trailing partial line in ingest_state. When an agent appends to a log:

  - Relay seeks to the previous offset.
  - Reads only new bytes.
  - Recombines the previously buffered partial line.
  - Returns complete newline-terminated records.
  - Saves the new offset and partial remainder.

  This avoids reparsing large growing logs on every write.

  There is an important consistency weakness here: the offset is persisted before the returned lines are ingested. A process crash between those two operations could permanently
  skip those records. Conversely, if a log is truncated and replayed, token and file-change totals can be counted twice. The code comments describe stronger replay safety than the
  implementation provides.

  ### 3. Defensive provider parsing

  All parsers emit a shared ParsedRecord from src-tauri/src/parser/record.rs:31. It contains:

  - Agent, session ID, project path, timestamp, model.
  - Token usage.
  - User/assistant text.
  - Recognized file-edit tool calls.
  - Claude’s generated session title.

  The Claude parser in src-tauri/src/parser/claude_jsonl.rs:1 is the mature one. It recognizes user, assistant, system, and ai-title records, extracts token-cache fields, and
  understands Write, Edit, MultiEdit, and NotebookEdit.

  Malformed records and unknown record types are ignored instead of crashing ingestion.

  Codex and Gemini use in-memory per-file context caches because their assumed header records contain the session ID and working directory only once. Cursor assumes every record
  is self-contained.

  ### 4. Normalized persistence

  src-tauri/src/parser/session_builder.rs:19 turns a ParsedRecord into database changes:

  - Derives a stable project ID by hashing the lowercase filesystem path.
  - Upserts the project and session.
  - Accumulates token counts.
  - Recalculates session cost.
  - Stores file changes and before/after text.
  - Creates or adopts the corresponding Kanban card.
  - Links a newly discovered native session to a recent Relay-owned dispatch.

  A session deleted inside Relay gets a tombstone, so its unchanged source log cannot recreate it during later watcher activity.

  ## Session lifecycle

  A background Tokio loop runs every 20 seconds. Any session inactive for more than 120 seconds becomes ended.

  The sweep deliberately uses three phases:

  1. Gather database targets while holding the SQLite mutex.
  2. Release the mutex and perform file reads.
  3. Reacquire the mutex briefly to write results.

  That lock discipline is one of the best backend decisions in the repository. The code consistently avoids holding the single database connection across slow filesystem or
  network operations.

  When a session ends:

  - ended_at becomes the last activity timestamp.
  - Duration is computed.
  - Its linked card moves to Review.
  - Tags are classified using keywords from the first user prompt.
  - An optional summary task may call Anthropic Haiku.
  - A data-changed event invalidates frontend caches.

  One qualification: transcript extraction currently always uses the Claude parser in src-tauri/src/parser/transcript.rs:22. Consequently, tags, summaries, and transcript export
  do not properly support Codex, Gemini, or Cursor logs even when their initial ingestion succeeds.

  ## Cost tracking

  src-tauri/src/cost/pricing.rs:12 loads a bundled static table from src-tauri/resources/pricing.json:1.

  The cost formula includes:

  - Prompt tokens.
  - Completion tokens.
  - Cache-read tokens.
  - Cache-creation tokens.

  Lookup order is exact model, longest matching prefix, then a default rate. Synthetic model names beginning with < are treated as nonbillable. All historical costs are recomputed
  on startup, so updating the bundled pricing table retroactively updates existing sessions.

  The main limitation is that the pricing table only contains Claude families. Unknown Codex or Gemini models receive the Claude-like default rate, which can produce misleading
  cross-agent comparisons. Cursor currently provides no usage and therefore displays zero cost.

  ## Database model

  SQLite is opened in WAL mode with foreign keys enabled in src-tauri/src/db/mod.rs:12. One connection is protected by a process-wide mutex.

  The migrations tell the project’s evolution clearly:

  1. Projects, sessions, file changes, and ingest offsets.
  2. Raw before/after diff content.
  3. Per-project Kanban boards, columns, and cards.
  4. Claude-generated session titles.
  5. An obsolete local auth-plan cache.
  6. Pending card-to-session launch adoption.
  7. Agent configurations and initial dispatch tables.
  8. Deleted-session tombstones.
  9. Structured, resumable dispatch conversations.

  The principal domains are:

  - projects → one per observed filesystem working directory.
  - sessions → normalized native agent sessions.
  - files_changed → one row per recognized edit.
  - ingest_state → byte offsets for passive log tails.
  - boards, columns, cards → project Kanban state.
  - agent_configs → local executable and model configuration.
  - dispatch_tasks → durable task intent.
  - dispatch_runs → one provider conversation/attempt.
  - dispatch_turns → prompts inside a conversation.
  - dispatch_events → normalized messages, tools, approvals, and errors.
  - deleted_sessions → prevents passive re-creation.

  All SQL is centralized in src-tauri/src/db/queries.rs:1, although that file is now 3,749 lines and is a natural refactoring candidate.

  ## Active dispatch and chat

  ### Connections

  src/views/ConnectionsView.tsx:117 lets users configure:

  - Whether each built-in agent is dispatchable.
  - Its executable or absolute path.
  - Available model identifiers.
  - Its default model.

  Relay does not store provider API keys. It searches PATH plus common Homebrew, npm, Volta, Bun, Cargo, and local-bin directories.

  ### Launching a task

  The dispatch command in src-tauri/src/commands.rs:673:

  - Validates project, agent, model, title, and prompt.
  - Rejects concurrent runs for the same project-agent pair.
  - Creates or reuses a Kanban card.
  - Moves it to In Progress.
  - Creates durable task, run, turn, and user-message records.
  - Launches the configured CLI in the project directory.

  src-tauri/src/dispatch/mod.rs:20 constructs provider-specific noninteractive commands. Claude receives its prompt through streaming JSON on stdin; Codex, Gemini, and Cursor
  receive prompts as command-line arguments.

  ### Runtime behavior

  src-tauri/src/dispatch/runtime.rs:24 owns only currently executing child processes. Conversation identity remains in SQLite, so a provider process can exit after each turn.

  Separate threads:

  - Consume structured stdout.
  - Consume stderr diagnostics.
  - Wait for child exit.
  - Persist normalized events.
  - Settle the turn and conversation.
  - Move the task card to Review.

  Supported actions are:

  - Send another prompt using the provider session ID.
  - Stop the current turn.
  - Resolve an approval.
  - Shut down the logical conversation.
  - Retry a shut-down/completed task as another attempt.

  At startup, runs left active by a prior Relay process are marked interrupted but remain resumable.

  ### Provider normalization

  src-tauri/src/dispatch/normalize.rs:626 maps provider-specific streamed JSON to:

  - User and assistant messages.
  - Tool calls and results.
  - Approval requests.
  - Status events.
  - Errors.

  Stable provider event IDs allow backend updates to modify one logical database row as assistant text grows.

  There is a current frontend bug: src/components/dispatch/RunChat.tsx:87 ignores an incoming event if its database ID already exists. Because the backend deliberately emits
  updated events with the same ID, assistant streaming can freeze at its first cached version until a later full refetch. The August 27 design explicitly calls for replacement-by-
  ID, but that portion is not implemented.

  ## Frontend surfaces

  The sidebar exposes seven application areas:

  - Home: ongoing Relay-owned work, aggregate spend, activity heatmap, top projects, and spend by agent.
  - Projects: project cards, Git activity, recent commits, project sessions, and Kanban board.
  - Sessions: all native sessions ordered by recent activity.
  - Timeline: client-side filters by project, tag, and date preset.
  - Reports: 7/30/90-day spend and token breakdowns with Markdown export.
  - Agent work: workday-indexed dispatch conversation viewer.
  - Connections: local CLI and model configuration.

  The typed Rust/TypeScript boundary is centralized in src/lib/tauri.ts:21. No view calls invoke directly.

  A global hook, src/hooks/useDataChangedEvents.ts:12, responds to coarse backend changes by invalidating the relevant React Query caches. This keeps event payloads intentionally
  small and avoids synchronizing large domain objects across Rust and TypeScript.

  The tradeoff is broad refetching: many mutations invalidate nearly every major query rather than only the affected entity.

  ## Kanban model

  Every project gets four role-bearing columns:

  - Todo.
  - In Progress.
  - Review.
  - Done.

  Users may add roleless custom columns. Sessions automatically create cards in In Progress. Finalized sessions and settled dispatch turns move their cards to Review.

  Manual dragging is allowed, but later lifecycle transitions intentionally override manual placement. Dropping an unlinked card into In Progress opens the dispatch modal rather
  than immediately launching Claude.

  The card adoption handshake is particularly useful to explain in an interview: the native session ID does not exist when Relay launches the CLI, so the card is stamped with a
  120-second pending window and agent ID. The first matching native session observed by the watcher adopts that card.

  ## Reports, activity, and exports

  There are two distinct activity concepts:

  - Dashboard activity counts session starts plus file-edit events.
  - Project overview activity comes from git log.

  Reports window sessions by last_activity_at, not when they began. Tag reporting is multi-label: a session with two tags contributes its full cost to both, so tag totals may
  exceed the overall report total.

  Reports and transcripts are written as Markdown to Downloads and can be revealed in Finder. Transcript export currently works correctly only for Claude-shaped logs.

  ## Verification status

  Fresh verification produced:

  - cargo test with an isolated target directory: 127 passed, 0 failed.
  - npx tsc -b: passed.
  - npx oxlint src tests vite.config.ts: clean.
  - node --test tests/dispatch-status.test.ts: 1 passed.

  There are seven Rust warnings, mostly legacy dispatch APIs and unused normalization fields.

  Ordinary cargo test currently fails before compilation because the local src-tauri/target cache contains absolute build metadata from /Users/tanmay/manageai. Before the
  interview, clear that generated cache with cargo clean from src-tauri; the isolated clean target proves the source tests themselves pass.

  No application build, dev server, or visual runtime test was performed because CLAUDE.md:11 explicitly prohibits agents from running them.

  ## Highest-priority interview risks

  1. No root README. The codebase needs a current interview-facing explanation, setup instructions, screenshot, architecture summary, and limitations.
  2. Documentation drift. The PRD still calls the product “Manageai,” the old plan understates current functionality, and the newest design spec describes unimplemented interface-
     size and conversation-projection work.

  3. Passive multi-agent support is overrepresented. Only Claude is verified; agent-aware transcript extraction, tagging, summaries, file changes, and pricing remain incomplete.
  4. Streaming cache bug. Same-ID event updates are ignored by the frontend.
  5. Tail-state atomicity. Byte offsets and record ingestion are not committed together, leaving a crash-loss window.
  6. Limited frontend testing. There is one pure Node test and no component/integration test framework. The Rust side is much better covered.
  7. Privacy wording needs precision. Native storage is local, but optional summaries send prompt excerpts and filenames to Anthropic, and dispatched CLIs communicate with their
     providers. “No data ever leaves the machine” is not strictly accurate.

  8. Stale dependencies and schema remnants. @supabase/supabase-js, the Vite Supabase environment types, and auth_state remain after auth removal. Existing migrations should not
     be edited, but unused dependencies/types can be removed.

  9. Metadata polish. Frontend version is 0.0.0, Tauri is 0.1.0, and Cargo still has placeholder description/author/license fields. The latest commit message, “staging all
     changes,” is also not interview-polished.

  10. Security hardening. Tauri’s CSP is disabled, prompts for several providers appear in process arguments, and local IPC commands largely rely on the frontend to supply
     consistent board/card relationships.

  ## Best place to add a feature

  For any database-backed feature, follow this path:

  1. Add migration 0010_...sql; never edit prior migrations.
  2. Register it in src-tauri/src/db/mod.rs:20.
  3. Add focused queries—preferably in a new domain-specific DB module instead of further enlarging queries.rs.
  4. Expose a narrow Tauri command.
  5. Add matching TypeScript types and a wrapper in src/lib/tauri.ts.
  6. Fetch it through React Query.
  7. Emit data-changed for mutations.
  8. Avoid holding the DB mutex across filesystem, network, subprocess, or async operations.
  9. Add Rust tests plus frontend behavior tests.

  The strongest bounded interview feature would be completing the August 27 streamed-chat work:

  - Fix replacement-by-ID in RunChat.
  - Project tool events into one activity block per turn.
  - Keep approvals and fatal errors prominent.
  - Add pure frontend tests for streaming merges and projection.
  - Optionally add the documented Small/Medium/Large interface-size setting.

  It is visible, technically meaningful, already backed by a written design, and touches persistence, events, React Query caching, UI state, accessibility, and tests without
  requiring a risky schema redesign.