# Relay

A local-first desktop control plane for Claude Code — a "Datadog for AI coding agents." Relay watches your Claude Code session logs (`~/.claude/projects/**/*.jsonl`), parses them defensively, and gives you a live dashboard of every project, session, and dollar spent, without any of your code or conversation history ever leaving your machine.

Built with Tauri (Rust backend) + React/TypeScript (frontend). No Node.js sidecar, no cloud service, no telemetry — one process, one local SQLite database.

## What it does

- **Projects view** — every repo you've used Claude Code in, discovered automatically from Claude Code's own log directory (no manual setup, no `.relay` marker files). Master-detail layout: click a project to see its sessions, spend, and a 14-day git-activity sparkline.
- **Sessions view** — every session across every project, with live token counts, cost, files changed, and an AI-generated one-sentence summary (via Claude Haiku) once the session ends.
- **Timeline view** — a chronological feed across all sessions with project/tag/date filters.
- **Cost tracking** — real per-session spend, computed from actual token usage (including cache read/write) against a bundled pricing table, updated live as a session runs.
- **Tag auto-classification** — sessions are tagged (`feature`, `bugfix`, `refactor`, `test`, `docs`, `infra`) from a keyword heuristic over the first prompt, no API call needed.
- **Live updates** — a filesystem watcher tails session logs incrementally as Claude Code writes to them; the UI updates within about a second, no polling, no manual refresh.

Everything is read-only with respect to your Claude Code sessions — Relay never writes to `~/.claude/projects`, it only watches it.

## Requirements

- [Node.js](https://nodejs.org/) 18+ and npm
- [Rust](https://rustup.rs/) (stable toolchain; see `rust-version` in `src-tauri/Cargo.toml` for the current minimum)
- macOS (this build targets macOS only — no Windows/Linux packaging yet)
- [Claude Code](https://claude.com/claude-code) installed and used at least once, so `~/.claude/projects/` has something to watch

## Getting started

```bash
npm install
npm run tauri dev
```

This starts the Vite dev server and launches the native Relay window with hot-reload on the frontend. First launch compiles the Rust backend, which can take a couple of minutes; subsequent launches are fast via incremental compilation.

On startup, Relay backfills from every existing `.jsonl` file under `~/.claude/projects/`, then watches for live changes. Local state (SQLite database, resolved API key) lives under Tauri's app-data directory as `relay.db`.

### AI summaries (optional)

Session summaries are generated via the Anthropic API (Claude Haiku) once a session goes idle. This is entirely optional — everything else works without it. To enable it, set an API key either:

- as the `ANTHROPIC_API_KEY` environment variable before launching, or
- in `<app_data_dir>/config.json` as `{"api_key": "..."}`

If neither is present, summarization is silently skipped (logged once, not repeatedly) and the rest of the app is unaffected.

## Other scripts

```bash
npm run build      # typecheck + production frontend build
npm run lint        # oxlint
cd src-tauri && cargo test    # Rust test suite (parser, watcher, cost, tags, summarize, activity)
```

## Architecture

- **`src-tauri/src/watcher/`** — filesystem watcher (`notify`), incremental byte-offset tailing so a live-growing session log is never re-parsed from scratch.
- **`src-tauri/src/parser/`** — defensive, `Option`-based JSONL parser for Claude Code's (undocumented, internal) session log format. Never panics on malformed or unrecognized input.
- **`src-tauri/src/db/`** — SQLite schema and queries (`rusqlite`), single shared connection behind a mutex.
- **`src-tauri/src/cost/`** — token-usage-to-dollars pricing lookup, bundled `pricing.json`.
- **`src-tauri/src/tags/`, `src-tauri/src/summarize/`** — tag classification and the AI summary pipeline, both triggered by a periodic idle-session sweep.
- **`src-tauri/src/activity.rs`** — per-project git-commit sparkline (shells out to `git log`, cached).
- **`src/`** — React frontend, one view per nav item (`Projects`/`Sessions`/`Timeline`), React Query for data fetching, a single Tauri event (`data-changed`) driving live cache invalidation.

## Status

This covers Phase 1 (Foundation) and Phase 2 (full session layer) of a larger roadmap. Claude Code is the only agent currently parsed; multi-agent support, an Agent Manager for dispatching new sessions, and Windows/Linux packaging are later phases and currently show as disabled "coming soon" nav items in the UI.
