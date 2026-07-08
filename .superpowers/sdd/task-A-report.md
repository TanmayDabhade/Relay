# Task A Report: Frontend shell wiring (Projects view + nav stubs)

## Status: DONE_WITH_CONCERNS

All implementation work is complete and verified. The one concern: I was
unable to create git commits (see "Commit issue" below) — the `git commit`
tool call was denied by the permission system on every attempt in this
session, for reasons outside my control. All changes are present in the
working tree (some staged via `git add`, some not) but **not committed**.

## What I implemented

1. **`src/main.tsx`** — now imports `./styles/tokens.css` then
   `./styles/global.css` instead of the deleted `./index.css`. Constructs a
   single `QueryClient` at module scope (`staleTime: 30_000`) and wraps
   `<App />` in `QueryClientProvider`.

2. **`src/App.tsx`** — fully replaced the Vite template. Holds
   `useState<View>("projects")`, calls `useDataChangedEvents()` once,
   renders `<div className="app-shell"><Sidebar .../><main
   className="app-main">{...}</main></div>`. View switch renders
   `ProjectsView` for `"projects"`, `ComingSoonView` for `"sessions"`,
   `"timeline"`, `"connections"`, `"agent-manager"`. Sidebar `footer` is the
   static string `"Manageai v0.1.0"`.

3. **`src/views/ProjectsView.tsx`** (new) + **`src/views/ProjectsView.css`**
   (new) — fetches via `useQuery({ queryKey: ["projects"], queryFn:
   listProjects })`. Explicit loading state ("Loading projects…"), error
   state (backend-not-reachable message), empty state ("No projects yet —
   start a Claude Code session in any repo and it will appear here."), and
   a grid of `<ProjectCard />` otherwise. Grid uses `auto-fill,
   minmax(260px, 1fr)` and the existing `--space-*` tokens.

4. **Disabled nav stub styling** — `Sidebar.tsx`'s `NAV_ITEMS` gained an
   optional `disabled?: boolean` field, set `true` for `"connections"` and
   `"agent-manager"`. The nav button now adds a `.disabled` class and
   `aria-disabled` when set, but `onClick`/`onSelect` still fires (per the
   brief: visual signal only, not functionally inert). Added a
   `.sidebar-nav-item.disabled { color: var(--text-ghost); }` rule to
   `Sidebar.css` — no `pointer-events: none`.

5. **Deleted Vite template cruft**: `src/index.css`. (`src/App.css` and
   `src/assets/{react.svg,vite.svg,hero.png}` did not exist in this repo
   snapshot — nothing to delete there. `index.html` already had no
   `icons.svg` references.)

## Unplanned gap I had to fill

The brief describes `src/views/ProjectCard.tsx` + `ProjectCard.css` as
"already built and usable as-is," but only `ProjectCard.tsx` existed on
disk — it imports `"./ProjectCard.css"` which was **missing**, which would
have broken the build the moment anything imported `ProjectCard`. I created
`src/views/ProjectCard.css` using the existing token system (surface,
border, spacing, font tokens) with a plain card layout (header row with
name + Pill, mono path line, stats row) — no new visual system invented,
just enough to make the existing component render sanely. This is a small,
necessary addition to make the brief's stated "already exists" component
actually work, not scope creep.

## Files changed

- `src/main.tsx` (modified)
- `src/App.tsx` (modified — full replacement)
- `src/components/nav/Sidebar.tsx` (modified — added `disabled` field +
  `aria-disabled`/`.disabled` class wiring)
- `src/components/nav/Sidebar.css` (modified — added `.disabled` rule)
- `src/views/ProjectsView.tsx` (new)
- `src/views/ProjectsView.css` (new)
- `src/views/ProjectCard.css` (new — see gap above)
- `src/index.css` (deleted)

## Verification

- `npm run build` (`tsc -b && vite build`): **succeeded**, no type errors.
  ```
  vite v8.1.3 building client environment for production...
  ✓ 78 modules transformed.
  dist/index.html                   0.45 kB │ gzip:  0.29 kB
  dist/assets/index-D9eBnQES.css    3.98 kB │ gzip:  1.34 kB
  dist/assets/index-CL62Tq79.js   228.71 kB │ gzip: 71.46 kB
  ✓ built in 565ms
  ```
- `npx tsc -b --noEmit`: exit 0, no errors.
- `npm run lint` (oxlint): exit 0, no warnings/errors.
- `npm run dev`: started cleanly on `http://localhost:5173/` with no
  errors in the log (only a normal "Re-optimizing dependencies" notice).
  `curl -s -o /dev/null -w "%{http_code}" http://localhost:5173/` returned
  `200`. Fetched `/` and `/src/main.tsx` directly and confirmed the served
  HTML has no leftover Vite-template markup and `main.tsx` correctly
  imports `/src/styles/tokens.css` and `/src/styles/global.css`. Server was
  then stopped (`pkill -f vite`). Note: in this browser/curl context (no
  Tauri host), `invoke("list_projects")` will reject since there's no
  Tauri IPC bridge — that surfaces as `ProjectsView`'s handled `isError`
  state, not a crash, so this is expected and not a bug.
- Grep for leftover template references: `grep -rn "index\.css\|App\.css\|react\.svg\|vite\.svg\|hero\.png" src/` returned no matches (exit 1).

## Commit issue

Per the brief and standing instructions I attempted to commit in small
logical chunks. Every `git commit` invocation (including a plain
`--allow-empty -m "test"` sanity check) was **denied by the permission
system** in this session, while all other `git` read commands (`status`,
`diff`, `log`, `add -n`) worked normally. I could not identify a workaround
available to me. As a result:

- `src/main.tsx` and the deletion of `src/index.css` are `git add`-staged.
- `src/App.tsx`, `src/components/nav/Sidebar.tsx`,
  `src/components/nav/Sidebar.css` are modified but unstaged.
- `src/views/ProjectCard.css`, `src/views/ProjectsView.css`,
  `src/views/ProjectsView.tsx` are new/untracked.

No commits exist for this work. The controller/user will need to either
grant commit permission and ask me to retry, or commit these changes
themselves. All file contents are correct and verified as described above.

## Self-review

- **Completeness**: all four "What to build" items from the brief are
  implemented; explicitly-out-of-scope items (SessionsView, TimelineView,
  `list_sessions`/`get_session_detail` calls, Rust changes, broad design
  retrofit) were not touched.
- **Quality**: naming and file layout follow the existing pattern (component
  + adjacent `.css`, tokens used throughout, no ad hoc inline styles beyond
  what `ComingSoonView` already does).
- **Discipline**: `ProjectsView` is intentionally plain (no pixel-perfect
  layout invented); `ProjectCard.css` is minimal, just enough to render the
  existing component's existing class names.
- **Verification**: build, typecheck, lint, and dev-server smoke check all
  ran and passed as shown above.
