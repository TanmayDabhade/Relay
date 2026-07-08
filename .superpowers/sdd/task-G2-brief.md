# Task G2: Design system retrofit — layout patterns + applying the component library

Source: PLAN.md, "Ordered task breakdown" item 19 (final part of the "design system pass") and §7 "Frontend": "Layout per SPEC §8.7: Projects = 296px left panel + f[lex-1 right panel]; Sessions/Timeline = full-width single column with topbar; Connections/Agent Manager = same shell, centered 'Coming soon' card, nav items visibly present but disabled (not removed)." Exact spec: SPEC.md §8.7 (Layout Patterns).

This is **G2 of a two-part final task**. G1 (already complete, reviewed and approved) built the foundation: self-hosted fonts, and four new `src/components/ui/` components (`StatTile`, `Button`, `Select`, `TextInput`), plus brought `Modal`/`Pill` in line with SPEC's exact values. **This task applies that foundation across the actual views** — it's the only task in this whole build that's allowed to touch `src/views/*` and `src/App.tsx` purely for layout/consistency reasons (not new data/features).

## Current state (read before writing anything)

- `src/components/ui/StatTile.tsx`, `Button.tsx`, `Select.tsx`, `TextInput.tsx` all exist now (built in G1) — read them before using them, they're small. `StatTile` takes `{ value: string | number; label: string }`. `Button` takes `{ variant: "primary" | "secondary" }` plus all native button props. `Select`/`TextInput` are drop-in wrappers around native `<select>`/`<input>` — same props, just styled to spec.
- `src/views/ProjectsView.tsx` currently renders a plain CSS grid of `ProjectCard`s, full width, no detail panel — this is what needs to become the master-detail layout. `ProjectCard.tsx` is unchanged/reused as the left-panel list item.
- **No `ProjectDetail` component exists yet.** PLAN.md's own repo-structure section (§2) names `ProjectsView(+ProjectCard/ProjectDetail)` as the intended file grouping from the start — this task is where `ProjectDetail` finally gets built.
- `src/views/SessionsView.tsx` and `src/views/TimelineView.tsx` currently render directly into `<div className="app-main">` (see `src/App.tsx`) with no page-level heading/topbar — just their content starts immediately. SPEC §8.7 wants "full-width single column, topbar with title" for these.
- `src/views/ComingSoonView.tsx` (used for `"connections"`/`"agent-manager"`) currently renders plain unstyled `<h1>`/`<p>` text, not a card. PLAN.md wants it as "a centered 'Coming soon' card."
- `src/components/nav/Sidebar.tsx` already marks `"connections"`/`"agent-manager"` nav items with a `disabled` visual flag (built in Task A) — don't touch this, it's already correct per PLAN's "nav items visibly present but disabled (not removed)."
- `TimelineView.tsx` currently uses raw `<select>` elements for its project/tag filters (built in a prior task, before `Select` existed) and plain `<button>`s for its date-range presets.
- `SessionDetailModal.tsx` currently renders a `.session-detail-stats` grid (around line 70) as plain `<span>`/`<div>` markup — a `label`+`value` pair per stat (status, tokens, cost, duration, etc.) — this is the natural `StatTile` use site. It also has an "Open" button per file in `files_changed` (calling `openInEditor`) that's currently a plain `<button>`.
- Reminder from `SPEC.md §8.1` "Philosophy": "Clean, calm, information-dense without being cluttered... Anti-patterns to avoid: dark mode with neon accents, excessive gradients or glassmorphism, cramped information density, images or decorative photography." Keep this in mind as a north star for any layout judgment calls below — this is a calm, dense professional tool, not a flashy consumer app.

## What to build

### 1. `src/views/ProjectDetail.tsx` (+ CSS) — new

Shows the currently-selected project's detail. Given SPEC §8.5's Modal body ordering precedent ("summary → stat grid → file list → action buttons") and this app's actual data model, a reasonable structure — **at minimum**, build these two things (exact visual arrangement/tabs beyond this minimum is your judgment call, but don't build less):
- An **overview section**: project name, full path (monospace, per how `ProjectCard.tsx` already renders paths), and a `StatTile` row for the project's own aggregate stats already available on `ProjectSummary` (`session_count`, `total_cost_usd`) — reuse `StatTile` here, don't hand-roll new stat markup. Include the project's `ActivityBars` sparkline here too (it's currently rendered inside `ProjectCard` — decide whether it makes more sense in the left-panel card, the right-panel detail, or both; your call, but don't remove it from wherever it currently provides value without a good reason).
- A **sessions-for-this-project section**: the subset of sessions (from the already-fetched `["sessions"]` query — fetch it here the same way `SessionsView.tsx`/`TimelineView.tsx` already do) where `project_id` matches the selected project. Render them using the same `SessionRow`-equivalent presentation `SessionsView.tsx` already has — **do not duplicate that row-rendering logic a third time**; either extract `SessionsView.tsx`'s existing `SessionRow` function/component to a shared location (e.g. its own small file) and import it from both places, or import it directly from `SessionsView.tsx` if it's already exported — your call on the cleanest way, but the actual JSX/logic for rendering one session row must not be copy-pasted a second time. Clicking a session row here should open `SessionDetailModal`, same as `SessionsView.tsx` does.
- Whether you organize this as literal tabs (e.g. two buttons toggling which section shows) or a single scrollable column with both sections is your call — SPEC says "tabs" but the exact interaction isn't specified, and a simple always-both-visible layout is defensible too if it reads better at this app's actual content density (a project with 2 sessions doesn't need tab-hiding). Use your judgment, matching the "clean, calm" philosophy — don't force a tabs UI if a simpler layout is genuinely better here, but if you skip literal tabs, note that judgment call in your report.
- Empty state: a project with zero sessions (shouldn't really happen given how projects are created, but a project's sessions could theoretically all be very new/unfetched) should show a reasonable empty message, not break.

### 2. `ProjectsView.tsx` — master-detail layout

Restructure into: a left panel (list of `ProjectCard`s, clickable to select) and a right panel (`ProjectDetail` for the selected project, or an empty/prompt state like "Select a project" when nothing's selected yet — auto-selecting the first project on load is also reasonable, your call). Per SPEC §8.7's exact width: **296px** for the left panel, **flex-1** for the right. Use `useState<string | null>` for "which project id is selected" (same local-state pattern used elsewhere in this codebase for "which item is open"). `ProjectCard.tsx` itself doesn't need prop changes for this unless you need to add a "selected" visual state to it (a subtle border/background change when it's the currently-selected card is a reasonable, minimal addition — don't overdo it).

### 3. Sessions/Timeline topbar

Both `SessionsView.tsx` and `TimelineView.tsx` need "a topbar with title" per SPEC §8.7. Add a consistent small header element at the top of each (e.g. an `<h1>`/page-title matching `--text-lg` per the existing typography scale, "Sessions" / "Timeline" respectively) — this can be a tiny shared pattern (e.g. both views wrapping their content in the same simple `<div className="view-topbar"><h1>...</h1></div>` structure with one shared CSS class, rather than two independently-styled headers that drift) — your call on whether that's worth a shared component or just consistent copy-pasted markup at this scale (two call sites is usually not worth a component, but keep the CSS class names/structure consistent between them either way).

### 4. `ComingSoonView.tsx` — centered card

Wrap its content in an actual card matching this design system's existing card treatment (see `ProjectCard.css`'s card styling for the established pattern — background/border/radius — reuse those values rather than inventing new ones), centered within the available space (`app-main`). Keep it simple — title + "Coming soon" text, no new content.

### 5. Swap raw form elements for G1's components

- `TimelineView.tsx`: replace its raw `<select>` elements (project filter, tag filter) with the new `Select` component — same `value`/`onChange`/`<option>` children pattern, just swap the element. If it uses raw `<button>`s for date-range presets, consider `Button` (`variant="secondary"`, or a distinct "active/selected" visual treatment for whichever preset is currently chosen — your call on exact styling for the "selected" state, `Button` doesn't need to grow a new prop for this if a simple conditional className achieves it).
- `SessionDetailModal.tsx`: replace the `.session-detail-stats` grid's plain markup with `StatTile` components, one per stat (status can stay as a `Pill` inside/beside a `StatTile`, or you can decide status doesn't fit the `StatTile` mold well since it's not really a "number" — your call, but the numeric/textual stats — tokens, cost, duration, lines added/removed — should become `StatTile`s). Replace the per-file "Open" button with the `Button` component (`variant="secondary"` is a reasonable choice for a non-primary in-context action — your call).

## Explicitly out of scope for this task

- No new Rust/backend changes — this task only touches `src/views/`, `src/App.tsx`, and (if you need a shared row-extraction) potentially a new small shared file under `src/views/` or `src/lib/`.
- No new data — `ProjectDetail` only uses data already fetched via `["projects"]`/`["sessions"]`, no new Tauri commands.
- No changes to `SessionDetailModal`'s or `TimelineView`'s actual filtering/query logic — only the presentational swap to `Select`/`Button`/`StatTile`.
- No Agent Manager real functionality (still a stub) — only the `ComingSoonView` card-wrapper styling.

## Verification

- `npm run build` (`tsc -b && vite build`) must succeed with no type errors.
- `npm run lint` (oxlint) must be clean.
- A dev-server smoke check (`npm run dev`, confirm no console errors in the server log, stop it) — same bar as prior frontend tasks.
- Confirm (by reading your own code) that `SessionRow`-equivalent rendering logic exists in exactly one place, not two, after `ProjectDetail` is added.
- Confirm the Projects left panel is genuinely `296px` (not an approximation) and the right panel is `flex-1`.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-G2-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why (especially your judgment calls: tabs vs. combined layout for `ProjectDetail`, where you kept `ActivityBars`, how you extracted/shared the session-row rendering), and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
