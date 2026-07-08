# Task F: Timeline view

Source: PLAN.md, "Ordered task breakdown" item 14: "Timeline view: chronological feed over the same data, colored agent dot (single color for now, per-agent mapping kept for Phase 3), client-side filters (project/tag/date)."

## Current state (read before writing anything)

- `src/App.tsx` currently renders `<ComingSoonView title="Timeline" />` for the `"timeline"` nav item. Replace that branch with a real `TimelineView`.
- The data this view needs already exists and is already fetched elsewhere — no backend changes, no new Tauri commands:
  - `listSessions()` (`src/lib/tauri.ts`) → `Session[]` — same data `SessionsView` (built in a prior task) already renders as a flat list. This task presents the *same underlying data* chronologically with filters, not a new data source. Use `useQuery({ queryKey: ["sessions"], queryFn: listSessions })` — same query key as `SessionsView`, so React Query dedupes the fetch if both views have been visited, and `useDataChangedEvents` (`src/hooks/useDataChangedEvents.ts`) keeps it live via that same key.
  - `listProjects()` for project names (same client-side join pattern `SessionsView.tsx` already uses: fetch `["projects"]`, look up by `project_id`, fall back to the raw id if not found).
- `Session` (`src/lib/types.ts`) fields relevant here: `id`, `project_id`, `agent` (always `"claude"` today — no other agent is ingested yet, per PLAN.md's explicit scope), `started_at: number | null`, `last_activity_at: number`, `status`, `tags: string | null` (a JSON-encoded array string, e.g. `'["bugfix","test"]'` — or `null` if not yet classified; parse defensively, the same way `SessionDetailModal.tsx` already does — look at its `parseTags` helper and reuse the same defensive parsing logic rather than writing a second, possibly-inconsistent parser. Either import/reuse it directly if it's reasonably extracted, or duplicate the same defensive logic — your call on the cleanest way, but don't let the two parsers diverge in behavior).
- `SessionDetailModal` (`src/views/SessionDetailModal.tsx`) already exists, is fully built and tested, and takes a `sessionId`/close-handler. **Reuse it directly for the timeline's click-to-expand behavior** — do not build a second detail view. Look at how `SessionsView.tsx` currently opens it (local `useState<string | null>` for "which session is open," passed down) and follow the same pattern.
- `Pill` (`src/components/ui/Pill.tsx`) — reuse for tag display and status, same as `SessionsView`/`SessionDetailModal` already do.
- No date-picker or multi-select component exists in this codebase. Don't add a component library dependency for this — plain HTML `<input type="date">` / `<select>` elements, styled with the existing design tokens (`src/styles/tokens.css`) via a plain CSS file (same pattern as every other view's `.css` file), are sufficient. A later task (design-system retrofit pass) will do a broader consistency pass across all views' form controls — don't try to anticipate that here, just make the filters functional and reasonably tidy.

## What to build

### 1. New file `src/views/TimelineView.tsx` (+ CSS)

- Fetches sessions (`["sessions"]`) and projects (`["projects"]`) as described above. Loading/error/empty states, following the established pattern in `ProjectsView.tsx`/`SessionsView.tsx` (empty-state copy is your call).
- **Chronological ordering**: sort sessions by `started_at` descending (most recent first), falling back to `last_activity_at` for the (rare) case where `started_at` is `null` — don't crash or sort inconsistently on a `null` value.
- **Colored agent dot**: a small colored marker per timeline entry representing which agent ran the session. Since `agent` is always `"claude"` today, a single fixed color is correct for now — but structure it as a small lookup (e.g. `const AGENT_COLORS: Record<string, string> = { claude: "var(--accent)" }` with a sensible fallback color for an unrecognized value) rather than hardcoding the color inline at the render site, so a future multi-agent phase can extend the map instead of restructuring the component — this is explicitly what PLAN.md means by "single color for now, per-agent mapping kept for Phase 3." Don't build anything more elaborate than this small lookup — no settings UI, no per-agent icons, just the color mapping structure.
- **Client-side filters** — all three are required, and must actually filter the rendered list (not be decorative):
  - **Project filter**: a `<select>` populated from the fetched projects list, plus an "All projects" option (default). Filters sessions by `project_id`.
  - **Tag filter**: a `<select>` (or a small set of toggleable `Pill`s — your call) populated from the *union of tags actually present* across the fetched sessions (derived at render time from each session's parsed `tags`, not hardcoded to the six SPEC.md categories — a session with no tags yet, or a future tag value, shouldn't be invisible to this control), plus an "All tags" option (default). Filters sessions whose parsed `tags` array includes the selected value.
  - **Date filter**: your call on exact UI (a simple "from"/"to" date range via two `<input type="date">`, or a small set of preset buttons like "Today" / "This week" / "All time" — pick whichever is less code for equivalent usefulness, don't build both). Filters sessions by `started_at` (or `last_activity_at` fallback, consistent with the sort) falling within the selected range.
  - Filters compose (AND, not OR) — e.g. selecting a project AND a tag shows only sessions matching both.
  - All filtering happens client-side, in-memory, over the already-fetched `Session[]` array — no new backend query parameters, no server-side filtering.
- **Each timeline entry**, at minimum: the colored agent dot, project name (via the lookup), a relative timestamp (reuse `formatRelativeTime` from `src/lib/format.ts` — already extracted in a prior task specifically so it wouldn't be duplicated a third time), a status `Pill`, tag `Pill`(s) if present, and the session's `summary` if present (omit/placeholder if `null` — a session may not have one yet, same as `SessionsView` already handles).
- Clicking an entry opens `SessionDetailModal` for that session (same pattern as `SessionsView.tsx`).

### 2. `src/App.tsx`: wire it in

Replace the `"timeline"` branch's `<ComingSoonView title="Timeline" />` with `<TimelineView />`. Don't touch the other branches.

## Explicitly out of scope for this task

- No backend/Rust changes at all — everything this view needs already exists as a Tauri command.
- No new reusable `Select`/`DatePicker`/`MultiSelect` components in `src/components/ui/` — plain HTML form elements are sufficient here, per the current-state note above.
- No changes to `SessionsView.tsx` or `SessionDetailModal.tsx` beyond what's needed to reuse `SessionDetailModal` as-is (if you find you need to change its public interface to reuse it, stop and report DONE_WITH_CONCERNS rather than modifying an already-reviewed, tested component's contract on your own judgment).
- No multi-agent color scheme beyond the one-entry lookup map described above — real per-agent theming is explicitly Phase 3.
- No broad design-system retrofit of other views — a later task handles that.

## Verification

- `npm run build` (`tsc -b && vite build`) must succeed with no type errors.
- `npm run lint` (oxlint) must be clean.
- A dev-server smoke check (`npm run dev`, confirm no console errors in the server log, stop it) — same bar as prior frontend tasks, since a real Tauri IPC bridge isn't available outside the actual app window in this environment; `list_sessions`/`list_projects` calls will reject in a browser-only context, which is expected and should surface as `TimelineView`'s handled error state, not a crash.
- Manually verify (by reading your own rendered logic, since there's no browser automation available here) that all three filters actually narrow the session list rather than being cosmetic — trace through what happens when a filter is set to a non-default value.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-F-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
