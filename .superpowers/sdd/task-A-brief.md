# Task A: Frontend shell wiring (Projects view + nav stubs)

Source: PLAN.md, "Ordered task breakdown" items 9 and 20, plus §7 "Frontend".

## Current state (read before writing anything)

- `src/App.tsx` is still the unmodified Vite React+TS template (counter button, vite/react logos). It must be replaced.
- `src/main.tsx` imports `./index.css` (the Vite template's stylesheet). That import must be swapped for the project's real stylesheets.
- `src/styles/tokens.css` and `src/styles/global.css` already exist and define the design system (CSS custom properties, `.app-shell` / `.app-main` layout classes). Read both files.
- `src/index.css` is Vite template cruft — once nothing imports it, delete it (also delete `src/App.css` and the `src/assets/react.svg`, `src/assets/vite.svg`, `src/assets/hero.png` template assets if they exist and nothing references them after your changes).
- Already built and usable as-is:
  - `src/components/nav/Sidebar.tsx` (+ `Sidebar.css`) — exports `Sidebar` component and a `View` union type: `"projects" | "sessions" | "timeline" | "connections" | "agent-manager"`. Takes `active`, `onSelect`, `footer` props.
  - `src/components/ui/Pill.tsx` (+ `Pill.css`)
  - `src/views/ProjectCard.tsx` (+ `ProjectCard.css`) — takes a `project: ProjectSummary` prop.
  - `src/views/ComingSoonView.tsx` — takes a `title` prop, already styled with `var(--text-lg)` etc.
  - `src/lib/types.ts` — `ProjectSummary`, `Session`, `FileChanged` TS interfaces matching the Rust `#[derive(Serialize)]` structs.
  - `src/lib/tauri.ts` — exports `listProjects(): Promise<ProjectSummary[]>` wrapping `invoke("list_projects")`.
  - `src/hooks/useDataChangedEvents.ts` — exports `useDataChangedEvents()`, a hook with no args/return that must be mounted exactly once near the app root. It listens for the Rust backend's `data-changed` Tauri event and calls `queryClient.invalidateQueries` for the `["projects"]` and `["sessions"]` query keys. It assumes a `QueryClientProvider` (from `@tanstack/react-query`, already a dependency in package.json) is an ancestor.
- Backend Tauri command surface today is **only** `list_projects` (registered in `src-tauri/src/lib.rs`'s `invoke_handler`). `list_sessions` / `get_session_detail` exist as Rust query functions but are not yet exposed as commands — that's a separate task (Batch B), not yours. Don't add TS calls for them yet.

## What to build

1. **`src/main.tsx`**: import `./styles/tokens.css` then `./styles/global.css` instead of `./index.css`. Wrap `<App />` in a `QueryClientProvider` from `@tanstack/react-query` (construct one `QueryClient` instance at module scope, standard React Query setup — `staleTime` choices are your call, nothing in the plan mandates a value).

2. **`src/App.tsx`**: replace entirely. Responsibilities:
   - `useState<View>` for the active nav item, default `"projects"` (no react-router — PLAN.md §7 is explicit: "No react-router — 5 fixed sidebar items ... a single `useState<View>` switch in App.tsx is sufficient").
   - Call `useDataChangedEvents()` once.
   - Render the shell: `<div className="app-shell"><Sidebar .../><main className="app-main">{...view switch...}</main></div>` (the `app-shell`/`app-main` classes already exist in `global.css` — flex layout, sidebar + scrollable main).
   - View switch: `"projects"` → a `ProjectsView` component (build this — see below). `"sessions"` and `"timeline"` → for now render `<ComingSoonView title="Sessions" />` / `<ComingSoonView title="Timeline" />` respectively (Batches E and F build the real views later; don't build them here — just don't leave the nav item dead). `"connections"` and `"agent-manager"` → `<ComingSoonView title="Connections" />` / `<ComingSoonView title="Agent Manager" />` per PLAN.md item 20 ("Connections/Agent Manager nav stubs (disabled + coming soon)... nav items visibly present but disabled (not removed) so the full IA reads correctly from day one").
   - Sidebar's `footer` prop: any reasonable static string is fine (e.g. app name/version, or omit detail — nothing in the plan mandates specific footer content).

3. **New file `src/views/ProjectsView.tsx`** (+ CSS if needed): fetches projects via React Query (`useQuery({ queryKey: ["projects"], queryFn: listProjects })` from `src/lib/tauri.ts`) and renders a grid/list of `<ProjectCard project={p} />` for each. Handle the loading and empty states explicitly (loading: simple text/placeholder is fine; empty: something like "No projects yet — start a Claude Code session in any repo and it will appear here" is in the spirit of PLAN.md's local-first pitch, but exact copy is your call). Don't invent a "Layout per SPEC §8.7" pixel-perfect design — PLAN.md item 19 (a later, separate task) is explicitly the "design system pass" that retrofits polish; here the priority is a working, honestly-styled Projects list wired to real data, using the tokens/classes that already exist rather than ad hoc inline styles wherever a class already covers it. It's fine and expected that this looks plain — do not gold-plate.

4. **Disabled state for stub nav items**: PLAN.md says stub views' nav items are "disabled (not removed)". Decide what "disabled" means in practice (e.g. visually muted, or a `disabled`/`aria-disabled` state on the button) — `Sidebar.tsx`'s `NAV_ITEMS` array currently has no per-item disabled flag, so you'll need to either add one there or handle "disabled-looking" styling from the parent. Use your judgment; keep the change minimal and consistent with the existing Sidebar component's structure. Since Connections/Agent Manager still need to render their ComingSoonView when clicked (per PLAN.md: nav items are "visibly present but disabled", not inert), clicking them should still switch the view — "disabled" here is a visual/affordance signal, not a functional block. Don't overthink this; a `.disabled` CSS class + `aria-disabled` is enough, no `pointer-events: none` (that would make them functionally inert, contradicting "not removed").

## Explicitly out of scope for this task

- Do not add `list_sessions`/`get_session_detail` calls, a SessionsView, or a TimelineView — those are Batches E and F.
- Do not touch any Rust/`src-tauri` code.
- Do not do a broad design-system retrofit pass across components that already exist (Sidebar, Pill, ProjectCard) — Batch G handles that later. Only touch their CSS if something is actually broken/missing for this task to render correctly.

## Verification

- `npm run build` (runs `tsc -b && vite build`) must succeed with no type errors.
- `npm run dev` must serve without console errors (you can start it, curl the root URL or check the process starts and stays up, then stop it — a real Tauri window screenshot isn't available in this environment, so build success + no obvious runtime errors from a quick dev-server smoke check is the bar here).
- No leftover references to the deleted Vite template files (`index.css`, `App.css`, template assets) — grep for them.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-A-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file. Commit your work with git (small, logical commits) before reporting DONE.
