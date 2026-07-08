# Task D: Activity bars (git log sparkline)

Source: PLAN.md, "Ordered task breakdown" item 18: "Activity bars: `git log --since=... --format=%ct` sparkline per project (shell out, cache, decorative)." Also §7 "Frontend"'s component-library list, which names `ActivityBars` as one of the reusable `src/components/ui/` components to build.

## Current state (read before writing anything)

- `ProjectSummary` (`src/lib/types.ts`) already has `path: string` — the absolute filesystem path to the project, exactly what a `git log` shellout in that directory needs. No new backend query is needed to get a project's path to the frontend; it's already there.
- `ProjectCard.tsx` (`src/views/ProjectCard.tsx`) renders each project's card in `ProjectsView`'s grid — this is where the sparkline gets displayed, one per card.
- No `src/components/ui/ActivityBars.tsx` exists yet — PLAN.md's component-library list names it, this task builds it.
- No caching layer or shell-out infrastructure exists yet for anything git-related in this codebase. `similar` is used for text diffing (unrelated). No existing Rust module shells out to external processes except `open_in_editor` (`src-tauri/src/commands.rs`, built in a prior task) — look at that for the `std::process::Command` pattern already established (though `open_in_editor` uses `.spawn()` fire-and-forget; this task needs `.output()` to actually capture `git log`'s stdout).
- This feature is explicitly called "decorative" in the plan — a project with no git history, a path that's no longer a valid git repo, or a machine without `git` on `PATH` must all degrade to "no data" (empty/flat sparkline), never a visible error, crash, or a broken-looking card. Nothing about a project card's core functionality (name, session count, spend, last-active time) should ever be affected by this feature failing.

## What to build

### 1. Backend: `project_activity` Tauri command (new)

Add a new command, e.g. `#[tauri::command] pub fn project_activity(project_path: String) -> Vec<i64>` (note: return the data directly, not `Result<Vec<i64>, String>` — per the "decorative, never surfaces an error" requirement above, **every failure mode inside this command should resolve to a default value, not an `Err` the frontend has to handle as an error state**; contrast this with `open_in_editor`, which correctly does return `Result` because a failed editor-open is a real, user-actionable failure — this command's failures are not).

Behavior:
- Shell out to `git log --since="14.days" --format=%ct` with the current working directory set to `project_path` (`std::process::Command::new("git").args([...]).current_dir(&project_path).output()`).
- Each line of successful stdout is a Unix timestamp (commit time, `%ct` format — seconds since epoch, one per line, oldest-to-newest or newest-to-oldest, either order is fine since you're bucketing by day, not relying on order).
- Bucket commits into **daily counts for a fixed 14-day window** (today plus the 13 days before it), returned as a `Vec<i64>` of length 14, index 0 = 13 days ago, index 13 = today (oldest-to-newest, matching how a sparkline reads left-to-right). A day with no commits is `0`, not missing/omitted.
- **Every failure path returns `vec![0; 14]`** (all-zero, i.e. "no activity" / flat sparkline), not an error: `git` not on `PATH`, the directory isn't a git repo, the directory doesn't exist, the process exits non-zero, stdout isn't parseable as expected. Log a debug/warning-level message for genuine failures (missing git, non-repo) so it's discoverable but never propagates to the UI as visible breakage. Use your judgment on log level — this is expected/common (e.g. every non-git project falls into this path), so it shouldn't be `warn!`-spammy for the common case of "not a git repo," but should be more visible for a genuinely unexpected failure — don't over-think this distinction, a reasonable judgment call is fine.
- Register the command in `src-tauri/src/lib.rs`'s `tauri::generate_handler![...]` alongside the existing commands.

### 2. Backend: cache layer

Shelling out to `git log` on every single card render (which happens on every React Query refetch, e.g. after any `data-changed` event) is wasteful — the plan explicitly says "cache." Add a small in-memory cache keyed by `project_path`, with a short TTL (a value in the 30-90 second range is reasonable — your call, document why you picked it), managed as Tauri state (e.g. `app.manage(ActivityCache(Mutex::new(HashMap::<String, (Instant, Vec<i64>)>::new())))`, following the same "small `Mutex`-guarded state, managed via `app.manage`" pattern already established for `Db`, `ApiKeyState`, and `InFlight` in this codebase). On a cache hit within the TTL, return the cached value without shelling out again; on a miss or expiry, shell out, store the fresh result, return it. This is a plain in-memory cache — no persistence to SQLite needed, it's fine for it to reset on app restart (this data is cheap to regenerate and explicitly decorative).

### 3. Frontend: `src/lib/tauri.ts` wrapper

```ts
export function getProjectActivity(projectPath: string): Promise<number[]> {
  return invoke("project_activity", { projectPath });
}
```

### 4. Frontend: `src/components/ui/ActivityBars.tsx` (+ CSS)

A small, reusable, presentational component: takes `data: number[]` (an array of daily counts, any length — don't hardcode `14` inside this component, that's a concern of whoever calls it) and renders a row of small vertical bars, one per data point, height scaled relative to the max value in the array (a day with `0` commits renders as a minimal/empty bar, not an invisible one — the sparkline's shape should be readable even on a quiet project). Use the existing design tokens (`src/styles/tokens.css`) for color/spacing — a muted/subtle color is appropriate since this is decorative chrome, not a primary data point (don't use `--accent` at full strength the way a primary CTA might; something like a lower-emphasis tone is more fitting — your call on the exact token, use your judgment about what reads as "subtle" in this design system). Handle an empty or all-zero array gracefully (still renders the bar row shape, just all at minimum height — don't return `null`/hide the component entirely, since "no visible activity" is itself useful information at a glance, not an error state).

### 5. Frontend: wire into `ProjectCard.tsx`

Fetch via `useQuery({ queryKey: ["project-activity", project.path], queryFn: () => getProjectActivity(project.path) })` inside `ProjectCard` (or lift to `ProjectsView` and pass down — your call, but per-card fetching with its own query key is simpler and React Query will handle the fan-out fine at this app's scale; don't over-engineer a batched multi-project endpoint that wasn't asked for). Render `<ActivityBars data={...} />` in a sensible spot in the card's existing layout (below the stats row is a reasonable default, but use your judgment on what looks least cramped given the existing `ProjectCard.css` layout — you're not expected to do a full visual redesign here, just fit this in tidily). While loading, either render nothing or an all-zero placeholder (your call) — don't block the rest of the card's rendering waiting on this decorative element, and don't show a visible loading spinner for something this minor.

## Explicitly out of scope for this task

- No new git-history-derived features beyond the sparkline itself (no commit list, no author info, no branch info).
- No persistence of the cache to SQLite — in-memory only, resets on restart.
- No configurability of the 14-day window or the cache TTL via a settings UI.
- No broader design-system retrofit of `ProjectCard.tsx`/`ProjectsView.tsx` beyond fitting the new element in.

## Verification

- `cd src-tauri && cargo build` must succeed; `cargo test` must still show all previously-passing tests green (you're not expected to add Rust tests for the git-shellout command itself — it's an OS-process-spawning wrapper against real git history with no clean pure-logic unit-test seam at this scale, similar to how `open_in_editor` wasn't unit-tested in a prior task — but the **daily-bucketing logic** (given a list of raw Unix timestamps, correctly bucket into a 14-length array with the right day boundaries) is pure and should be extracted into a small testable function with unit tests covering: timestamps all on one day, timestamps spread across the window, a timestamp older than the 14-day window (must be excluded, not miscounted into day 0), an empty timestamp list).
- `npm run build` and `npm run lint` must succeed/be clean.
- A dev-server smoke check (`npm run dev`, confirm no console errors, stop it) — same bar as prior frontend tasks; `project_activity` will reject in a browser-only context outside the real Tauri app, which `ActivityBars`/`ProjectCard`'s handling of the loading/absent-data case should absorb without a crash.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-D-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
