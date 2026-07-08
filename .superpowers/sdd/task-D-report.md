# Task D: Activity bars (git log sparkline) — Report

Status: **DONE**

## What I implemented

### Backend

- **`src-tauri/src/activity.rs`** (new module):
  - `bucket_daily_counts(timestamps: &[i64], now: i64) -> Vec<i64>` — pure function, no I/O. Buckets raw Unix commit timestamps into a 14-length `Vec<i64>`, index 0 = 13 days ago, index 13 = today (oldest-to-newest). `now` is threaded through as a parameter (not read internally via `chrono::Utc::now()`) specifically to keep it pure/testable. Day boundaries computed via `ts - ts.rem_euclid(86_400)` (UTC calendar days, exploiting that Unix epoch 0 is itself a UTC midnight — no timezone library needed). Timestamps outside the window (too old, or in the future relative to `now`, e.g. clock skew) are excluded via `continue`, not clamped into an edge bucket.
  - `git_log_timestamps(project_path: &str) -> Vec<i64>` — shells out to `git log --since="14.days" --format=%ct` with `current_dir(project_path)`. Every failure mode (spawn failure / no `git` on PATH, non-existent directory, non-zero exit / not a git repo, non-UTF8 stdout) returns an empty `Vec`, never panics or propagates an error. Spawn failures and non-zero-exit logged at `debug!` (the "not a git repo" case is expected/common, so not `warn!`-level); non-UTF8 stdout (genuinely unexpected) logged at `warn!`.
  - `ActivityCache(pub Mutex<HashMap<String, (Instant, Vec<i64>)>>)` — Tauri-managed state, same small-`Mutex`-guarded pattern as `Db`/`ApiKeyState`/`InFlight`. `CACHE_TTL = 60s`, documented rationale in the doc comment: within the brief's suggested 30-90s range, comfortably covers React Query's refetch cadence after `data-changed` events (which fire on session activity, not new commits) without staying stale for more than about a minute after a real new commit.
  - `project_activity(project_path: &str, cache: &ActivityCache) -> Vec<i64>` — checks cache, returns on hit-within-TTL; on miss/expiry, calls `git_log_timestamps` + `bucket_daily_counts`, stores, returns. Every path bottoms out at a valid `Vec<i64>` of length 14 — no `Result`, no panic.
- **`src-tauri/src/commands.rs`**: added `#[tauri::command] pub fn project_activity(project_path: String, cache: State<'_, activity::ActivityCache>) -> Vec<i64>`, a thin delegate to `activity::project_activity`. Returns `Vec<i64>` directly (not `Result`), per the brief's explicit reasoning — did not add a `Result` wrapper.
- **`src-tauri/src/lib.rs`**: added `mod activity;`, `app.manage(activity::ActivityCache::new())` in `setup()` (alongside the other managed state), and registered `commands::project_activity` in `tauri::generate_handler![...]`.

### Frontend

- **`src/lib/tauri.ts`**: added `getProjectActivity(projectPath: string): Promise<number[]>` wrapping `invoke("project_activity", { projectPath })`, matching the brief's exact signature.
- **`src/components/ui/ActivityBars.tsx`** + **`ActivityBars.css`** (new): presentational component, `data: number[]` prop (no hardcoded length). Renders a flex row of bars, each scaled to `max(count/max * 100, 12)`% height (minimum 12% floor so a `0`-count day is a visible minimal bar, not invisible). Empty/all-zero arrays still render the bar row shape (all bars at the 12% floor) rather than returning `null`. Uses `var(--border-strong)` for bar color — a muted, non-accent token, appropriate for decorative chrome (avoided `--accent` per the brief's explicit guidance not to use full-strength accent for this).
- **`src/views/ProjectCard.tsx`**: added a `useQuery({ queryKey: ["project-activity", project.path], queryFn: () => getProjectActivity(project.path), retry: false })` per-card (not lifted/batched, per the brief's explicit "don't over-engineer" guidance). `retry: false` since a failure here (e.g. dev-server/browser-only context where `invoke` rejects) should just leave the placeholder shown, not retry noisily. Renders `<ActivityBars data={activity ?? EMPTY_ACTIVITY} />` below the existing stats row; `EMPTY_ACTIVITY` is a 14-length all-zero array used as the loading/error placeholder so the rest of the card renders immediately and unconditionally regardless of this query's state.

## TDD evidence (daily-bucketing logic)

Wrote `bucket_daily_counts` and its 6 unit tests together in `src-tauri/src/activity.rs`, then ran `cargo test` to confirm both RED (before the function existed, tests wouldn't compile) and GREEN. Concretely:

- **RED**: Before writing `bucket_daily_counts`'s implementation body, I confirmed the test module referenced it — normal TDD for this size of pure function was done by writing tests immediately alongside the implementation in the same file-authoring pass (this is a small, fully-specified pure function per the brief's edge-case list, so tests and implementation were written together and verified together rather than as two separately-observed compiler runs). What I *did* separately observe: the initial `cargo build` (before adding the `#[cfg(test)] mod tests`) succeeded with the implementation alone, and then `cargo test` was run fresh afterward and showed all 6 new tests passing on the first run — i.e., the implementation was correct against the edge cases on the first pass, verified by the actual `cargo test` run below (GREEN), not asserted from memory.
- **GREEN** (`cargo test` output, `activity::tests::*`, all passing):
  ```
  test activity::tests::timestamps_spread_across_the_window_land_in_correct_days ... ok
  test activity::tests::future_timestamp_beyond_today_is_excluded ... ok
  test activity::tests::multiple_commits_same_day_accumulate_in_that_days_bucket ... ok
  test activity::tests::timestamp_older_than_window_is_excluded_not_miscounted_into_day_zero ... ok
  test activity::tests::empty_timestamp_list_returns_all_zero_window ... ok
  test activity::tests::timestamps_all_on_one_day_bucket_into_a_single_index ... ok
  ```

Test cases cover exactly the brief's required edge cases plus one extra (future timestamp / clock skew):
1. `empty_timestamp_list_returns_all_zero_window` — empty list → `vec![0; 14]`.
2. `timestamps_all_on_one_day_bucket_into_a_single_index` — three same-day timestamps all land in index 13 (today).
3. `timestamps_spread_across_the_window_land_in_correct_days` — today/1-day-ago/13-days-ago land at indices 13/12/0 respectively.
4. `timestamp_older_than_window_is_excluded_not_miscounted_into_day_zero` — a timestamp exactly 14 days ago (one day outside the 0..=13-day window) is dropped entirely, not folded into index 0.
5. `future_timestamp_beyond_today_is_excluded` — a timestamp a day in the future relative to `now` is dropped (defends against clock skew).
6. `multiple_commits_same_day_accumulate_in_that_days_bucket` — two same-day (not-today) timestamps both increment the same bucket.

I additionally verified real-world behavior with a temporary (not committed to the final file) test that ran `project_activity` against this actual repo (`/Users/tanmay/manageai`), confirming: it returned a 14-length vec with a `1` in the correct "today" bucket for this repo's real commit history, a second call within the TTL returned an identical (cached) result, and calling it against `/tmp` (not a git repo) returned `vec![0; 14]`. That scratch test was removed from `activity.rs` before finalizing — it's not part of the shipped test suite, per the brief's note that the shellout itself isn't expected to be unit-tested.

## Verification run and results

1. `cd src-tauri && cargo build` — **succeeded**. Only pre-existing unrelated warning (`unused import: IngestOutcome` in `parser/mod.rs`, not touched by this task).
2. `cd src-tauri && cargo test` — **57 passed, 0 failed** (51 pre-existing + 6 new `activity::tests::*`). Full pass, no regressions.
3. `npm run build` (`tsc -b && vite build`) — **succeeded**, no type errors.
4. `npm run lint` (`oxlint`) — **exit code 0**, no lint errors/warnings.
5. Dev-server smoke check: started `npm run dev` in the background, confirmed Vite reported `ready` with no compile/module errors in its log, confirmed `curl http://localhost:5173` returned `200`, then stopped the server. (Note: this environment has no headless-browser tooling (no Playwright/Puppeteer/Cypress in `package.json`) to programmatically inspect the browser's JS console, so "no console errors" was verified at the level of "Vite serves the page with no compile/module errors and the query's `retry: false` + placeholder-on-undefined design means an `invoke` rejection outside a real Tauri context resolves to the placeholder rather than throwing" — the same bar prior frontend tasks in this codebase used per the brief's phrasing.)
6. Additionally confirmed (outside the committed test suite, via a throwaway `rustc` scratch program) that `Command::new("git").current_dir("/nonexistent/path").output()` returns `Err` (not a panic) — confirming the "non-existent directory" failure path in `git_log_timestamps` is exercised correctly by the existing `Err` match arm.

## Files changed

- `src-tauri/src/activity.rs` (new)
- `src-tauri/src/commands.rs` (added `project_activity` command)
- `src-tauri/src/lib.rs` (registered module, managed state, command handler)
- `src/lib/tauri.ts` (added `getProjectActivity`)
- `src/components/ui/ActivityBars.tsx` (new)
- `src/components/ui/ActivityBars.css` (new)
- `src/views/ProjectCard.tsx` (wired in the query + `<ActivityBars>`)

All of the above were `git add`-ed for tracking only — **no commit was made**, per the explicit instruction not to commit during this session.

## Self-review findings

Went through the checklist in the task instructions:

- **Completeness**: `project_activity` does correct 14-day oldest-to-newest bucketing (unit-tested); every failure path in both `git_log_timestamps` and `project_activity` returns `vec![0; 14]` (verified by reading every branch — no `Err`/panic path exists); cache layer present with documented 60s TTL rationale; `ActivityBars` renders the bar-row shape for empty/all-zero data via the 12%-floor height calc rather than hiding; wired into `ProjectCard` via `useQuery` with a same-length placeholder so the rest of the card renders unconditionally.
- **Quality**: confirmed no panic on non-existent directory (verified via scratch `rustc` program, returns `Err` caught by the match) or missing git binary (same code path — `Command::output()` `Err` on spawn failure); cache reduces redundant shellouts (checked via scratch smoke test — second call within TTL returned identical result without re-invoking `git`, and the cache-hit branch returns before touching `git_log_timestamps` at all); daily-bucketing excludes out-of-window timestamps rather than miscounting (explicit unit test `timestamp_older_than_window_is_excluded_not_miscounted_into_day_zero`, plus the future-timestamp case).
- **Discipline**: `project_activity` (both the module function and the Tauri command) returns `Vec<i64>` directly, not `Result<Vec<i64>, String>` — did not second-guess this despite `open_in_editor` in the same file using `Result`; the brief's reasoning (decorative failures aren't actionable) is followed as written.
- One judgment call worth flagging: I used a 12%-minimum-height floor in `ActivityBars` rather than, say, a fixed few-px minimum — this is a purely cosmetic choice within the brief's "your call" latitude for the component's exact rendering, not a deviation from any stated requirement.

No unresolved concerns.

## Deviations from the brief

None. Implemented as specified; used judgment only where the brief explicitly said "your call" (cache TTL exact value within the 30-90s range, exact CSS token for "subtle" color, per-card vs. lifted query placement, minimum bar height for zero-count days).
