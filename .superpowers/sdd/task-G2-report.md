# Task G2 Report: Design system retrofit — layout patterns + applying the component library

## Status: DONE

## What I implemented

1. **`src/views/ProjectDetail.tsx` + `ProjectDetail.css` (new)** — right-panel project detail
   for `ProjectsView`'s master-detail layout.
   - Overview section: project name, monospace full path, a `StatTile` row (`session_count`
     as "Sessions", `total_cost_usd` formatted as "Total cost"), and the project's
     `ActivityBars` sparkline (fetched via the same `getProjectActivity` query key/pattern
     `ProjectCard` already uses).
   - Sessions section: filters the shared `["sessions"]` query client-side by
     `project_id === project.id`, renders each match with the shared `SessionRow` component
     (see extraction below), and opens `SessionDetailModal` on click — identical interaction
     to `SessionsView`.
   - Empty state: "No sessions yet for this project." when the filtered list is empty; loading
     and error states handled the same way as the other views.
   - **Judgment call — no literal tabs.** I built this as a single scrollable column (overview
     always visible, sessions list always visible below it) rather than a tabbed toggle. SPEC
     §8.7 says "tabs" but the brief explicitly permits skipping them if a simpler layout reads
     better at this app's content density. A project with a handful of sessions doesn't
     benefit from hiding the session list behind an extra click, and it keeps the component
     simpler with no additional local UI state. Noting this per the report contract.
   - **Judgment call — `ActivityBars` placement.** Left it in `ProjectCard` (left panel, where
     it already lived) *and* added it to `ProjectDetail`'s overview section. Kept the query in
     both places (React Query dedupes/caches by `["project-activity", project.path]`, so this
     isn't a duplicate network call when both are mounted) — the sparkline is useful at both
     the "which project" glance level and the "tell me about this project" detail level.

2. **`src/views/SessionRow.tsx` + `SessionRow.css` (new)** — extracted `SessionRow` out of
   `SessionsView.tsx` into its own file (component + its CSS, which previously lived inline in
   `SessionsView.css`) so both `SessionsView.tsx` and `ProjectDetail.tsx` import the same
   component rather than duplicating the row markup. See "Self-review" below for the
   verification grep.

3. **`src/views/ProjectsView.tsx`** — rebuilt as the master-detail layout: `useState<string |
   null>` tracks the selected project id; falls back to `data[0]` when nothing's selected yet
   (auto-select-first-on-load, one of the two options the brief explicitly allowed) so there's
   no extra "select a project" placeholder state to build. Left panel renders `ProjectCard`s
   with `selected`/`onClick` props; right panel renders `ProjectDetail` for the selected
   project.

4. **`src/views/ProjectsView.css`** — `.projects-view` is `display:flex`; `.projects-view-list`
   is `width: 296px; flex-shrink: 0`; `.projects-view-detail` is `flex: 1; min-width: 0`. Exact
   296px, not an approximation — confirmed by grep in verification.

5. **`src/views/ProjectCard.tsx` + `.css`** — added optional `selected`/`onClick` props;
   changed the root element from `<div>` to `<button>` (matching the existing
   `SessionRow`/`TimelineEntry` convention for clickable full-card rows — full-width, reset
   button chrome, `text-align: left`) so the newly-added click-to-select interaction is
   keyboard-accessible. Added `.project-card-selected` (border-selected color +
   accent-subtle background) as the minimal selected-state visual, per the brief's "subtle
   border/background change" suggestion. No other prop changes.

6. **Sessions/Timeline topbars** — added a `<div className="view-topbar"><h1
   className="view-topbar-title">…</h1></div>` at the top of both `SessionsView.tsx` and
   `TimelineView.tsx`. `.view-topbar-title` (shared CSS class, `--text-lg`/600 weight) lives in
   `src/styles/global.css` since it's identical across both call sites — two call sites felt
   too small to justify a `<Topbar>` component per the brief's own framing, but the CSS is
   still centralized so the two headers can't drift. Restructured both views so the topbar
   renders unconditionally (even during loading/error/empty states) rather than only after
   data resolves.

7. **`src/views/ComingSoonView.tsx` + new `ComingSoonView.css`** — wrapped content in an actual
   card (border-default/8px-radius/surface-card, same values `ProjectCard.css` already
   establishes — no new values invented), centered within `app-main` via a flex wrapper at
   `height: 100%`.

8. **`TimelineView.tsx`** — swapped both raw `<select>` filters (project, tag) for the `Select`
   component (same value/onChange/`<option>` children, no filtering-logic changes) and swapped
   the three raw date-preset `<button>`s for `Button` (`variant="secondary"`), keeping the
   `timeline-filter-preset-active` class as a conditional `className` override for the
   currently-selected preset (accent border/bg/text) rather than growing `Button` a new prop.
   Removed the now-redundant `.timeline-filter-select`/`.timeline-filter-preset` CSS from
   `TimelineView.css` (styling now comes from `Select`/`Button`'s own CSS).

9. **`SessionDetailModal.tsx`** — replaced the `.session-detail-stats` grid's plain
   `<span>`/`<div>` pairs with `StatTile` for the numeric/textual stats: Duration, Cost,
   Prompt tokens, Completion tokens, Cache read tokens, Cache creation tokens, Lines added,
   Lines removed. **Judgment call:** Status stayed a `Pill` (not a StatTile — it's categorical,
   not a number/metric, and `Pill` with a `tone` already conveys it well) and Model stayed a
   plain monospace text label next to the status Pill (mirroring exactly how `SessionRow`/
   `TimelineEntry` already render status+model together, so the modal doesn't invent a third
   pattern for the same two fields). Replaced the per-file "Open" button with `Button`
   (`variant="secondary"`), keeping a small `.session-detail-file-open` override for
   size/spacing only (padding/font-size), since `Button` now owns background/border/radius/
   hover.

## Files changed

New:
- `src/views/ProjectDetail.tsx`, `src/views/ProjectDetail.css`
- `src/views/SessionRow.tsx`, `src/views/SessionRow.css`
- `src/views/ComingSoonView.css`
- `src/views/ProjectsView.css`, `src/views/ProjectsView.tsx` (were untracked/new from a prior
  session's partial state; fully rewritten here)

Modified:
- `src/views/ProjectCard.tsx`, `src/views/ProjectCard.css`
- `src/views/SessionsView.tsx`, `src/views/SessionsView.css`
- `src/views/TimelineView.tsx`, `src/views/TimelineView.css`
- `src/views/SessionDetailModal.tsx`, `src/views/SessionDetailModal.css`
- `src/views/ComingSoonView.tsx`
- `src/styles/global.css` (added shared `.view-topbar-title`)

Not touched: `src/App.tsx` (routing already wired correctly by a prior task — no change
needed), `src/components/nav/Sidebar.tsx`/`.css` (already correct per Task A, left alone as
instructed), any `src-tauri/*` file, any Tauri command surface.

## Verification

- `npm run build` (`tsc -b && vite build`) — **passed**, no type errors. Full output showed a
  clean Vite production build (105 modules transformed, `dist/` emitted).
- `npm run lint` (oxlint) — **passed**, exit code 0, no output (this project's `.oxlintrc.json`
  only enables `react`/`typescript`/`oxc` plugins; clean runs produce no output here, confirmed
  by also running `npx oxlint` directly with the same empty-output/exit-0 result).
- Dev-server smoke check: `npm run dev` in the background, waited for Vite's ready banner
  (`VITE v8.1.3 ready in 603 ms`, `Local: http://localhost:5173/`), confirmed `curl` against
  `http://localhost:5173/` returned `HTTP 200`, re-checked the server log afterward for any
  new lines (none — no console/server errors), then stopped the process with `pkill -f vite`
  and confirmed no vite process remained.

## Self-review findings

- **SessionRow-equivalent logic exists in exactly one place.** Verified via
  `grep -rn 'className="session-row"' src --include="*.tsx"` — the only match is inside
  `src/views/SessionRow.tsx` itself. `grep -rn "SessionRow" src --include="*.tsx"` shows
  `SessionsView.tsx` and `ProjectDetail.tsx` both `import { SessionRow } from "./SessionRow"`
  and use it as `<SessionRow ... />`; neither has its own copy of the row JSX.
- **296px/flex-1 confirmed literal**, not approximated: `src/views/ProjectsView.css` has
  `.projects-view-list { width: 296px; ... }` and `.projects-view-detail { flex: 1; ... }`.
- **No raw `<select>` left in `TimelineView.tsx`**: `grep -n "<select" src/views/TimelineView.tsx`
  returns nothing; both filters now use `Select`.
- **Zero-session project doesn't crash**: `ProjectDetail`'s `projectSessions` is a plain
  `.filter()` over the shared sessions array (defaults to `[]` if the query hasn't resolved
  yet), and the empty-state branch renders a message instead of an empty list — confirmed by
  reading the code path and by the successful `tsc` type-check (no unsafe indexing/assumptions
  on a non-empty array).
- **Discipline check**: no `src-tauri` changes, no new Tauri command usage (only
  `listProjects`/`listSessions`/`getProjectActivity`/`getSessionDetail`/`openInEditor`, all
  pre-existing), no changes to `TimelineView`'s actual filter/sort logic
  (`isWithinDatePreset`, `parseTags`, `sortedAndFiltered`, `availableTags` are untouched —
  only the JSX rendering the controls changed), `ComingSoonView` still just renders a static
  "Coming soon" card for both `connections` and `agent-manager` with no new functionality.
- One incidental fix while doing this: `ProjectCard`'s root element changed from a plain
  `<div>` to a `<button>` since it's now genuinely clickable (project selection) — this
  matches the existing accessibility convention already used by `SessionRow`/`TimelineEntry`
  for other full-card clickable rows in this codebase, rather than introducing a
  keyboard-inaccessible `onClick` on a non-interactive element.

## Deviations / concerns

None outside the judgment calls already called out above (no-tabs ProjectDetail layout,
ActivityBars kept in both places, Status/Model staying outside the StatTile grid in
SessionDetailModal). Nothing left half-implemented; nothing skipped from the brief's minimum
requirements.
