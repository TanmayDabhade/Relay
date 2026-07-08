# Task E: Sessions view (SessionRow list, SessionDetailModal, open-in-editor)

Source: PLAN.md, "Ordered task breakdown" item 13, plus §7 "Frontend" (component library note) and the "open in editor via $EDITOR/code" phrase from item 13 itself.

## Current state (read before writing anything)

- `src/App.tsx` currently renders `<ComingSoonView title="Sessions" />` for the `"sessions"` nav item. Replace that branch with a real `SessionsView`.
- Backend session commands exist and are tested (Task B, just approved):
  - `list_sessions` → `Session[]` (see `src/lib/types.ts`'s `Session` interface — already correct, matches the Rust struct field-for-field: `id, project_id, agent, model, started_at, ended_at, last_activity_at, status, duration_seconds, summary, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens, cost_usd, lines_added, lines_removed, tags, raw_log_path`).
  - `get_session_detail(session_id: string)` → `{ session: Session, files_changed: FileChanged[] } | null` (Rust return type is `Result<Option<SessionDetail>, String>` where `SessionDetail { session, files_changed }` — see `src-tauri/src/commands.rs`). **This exact shape (`{ session, files_changed }`) is not yet in `src/lib/types.ts`** — you need to add it.
  - Neither command has a TypeScript wrapper in `src/lib/tauri.ts` yet — it currently only has `listProjects()`. Follow its exact pattern (`invoke("command_name")`) to add `listSessions()` and `getSessionDetail(sessionId: string)`.
- `Session.status` is `"active" | "ended"` — a session is `"active"` until the idle-sweep (Task B, now running every ~20s with a 120s idle threshold) flips it. `Session.summary` and `Session.tags` are always `null` right now (Task C, not yet built, will populate them) — your UI must handle `null` gracefully for both, not treat it as an error state.
- `Session.model` can be `null` (no assistant record processed yet) and can also be a "raw" string like `claude-opus-4-8` or a synthetic sentinel like `<synthetic>` — display whatever string is there as-is, don't try to prettify/validate it.
- `useDataChangedEvents` (`src/hooks/useDataChangedEvents.ts`) already invalidates the `["sessions"]` query key on any `data-changed` event — your `useQuery` call for the sessions list must use exactly that key (`["sessions"]`) or it won't get the automatic refresh the idle-sweep and live ingestion rely on.
- No `Modal` component exists yet in `src/components/ui/` — PLAN.md's §7 component-library list names one (`Card, StatTile, Modal, Button, Select, TextInput, ActivityBars`) but only `Pill` has been built so far. You need to build a minimal, reusable `Modal` for `SessionDetailModal` to use — keep it generic (an `isOpen`/`onClose`/`children` style component other views can reuse later), not hard-coded to session content. Don't build `StatTile`/`Select`/`TextInput`/`ActivityBars` — out of scope, build only what this task needs.
- Existing patterns to follow: `src/views/ProjectsView.tsx` + `.css` (React Query `useQuery` usage, loading/error/empty states, grid layout using CSS custom properties from `src/styles/tokens.css`), `src/views/ProjectCard.tsx` + `.css` (card layout, `Pill` usage), `src/components/ui/Pill.tsx` (component + adjacent `.css` file pattern).
- `src/lib/types.ts`'s `ProjectSummary` has `name`/`path` per project. The `Session` type only has `project_id` (no project name) — `list_sessions` does not join project name server-side. To show a human-readable project name per session row without a backend change, fetch projects via the same `["projects"]` query key (already cached from `ProjectsView`'s `useQuery`, or fetch fresh if `SessionsView` is opened first — React Query handles this either way) and look up by `project_id`. Fall back to showing the raw `project_id` if no matching project is found (e.g. project was deleted from the table somehow, or query hasn't resolved yet) — don't crash or show `undefined`.

## What to build

### 1. Backend: `open_in_editor` Tauri command (small, new)

`src-tauri/src/commands.rs`: add `#[tauri::command] pub fn open_in_editor(path: String) -> Result<(), String>`. Behavior: try `$EDITOR` env var first (spawn `$EDITOR <path>` via `std::process::Command`), and if `$EDITOR` isn't set, fall back to `code <path>` (VS Code CLI) — matching the plan's literal phrase "open in editor via `$EDITOR`/code". If neither succeeds to spawn (e.g. `code` isn't on `PATH` either), return `Err` with a clear message rather than panicking — this runs against a real file path on the user's disk, driven by a button click, so a spawn failure is an expected, recoverable case, not a bug. Don't block waiting for the editor to exit — spawn and return (a `.spawn()` call, not `.output()`/`.status()`, so the UI doesn't hang waiting for the user to close their editor). Register it in `src-tauri/src/lib.rs`'s `tauri::generate_handler![...]` alongside the existing three commands.

### 2. `src/lib/types.ts`: add the `SessionDetail` type

```ts
export interface SessionDetail {
  session: Session;
  files_changed: FileChanged[];
}
```

(Place it near the existing `Session`/`FileChanged` interfaces.)

### 3. `src/lib/tauri.ts`: add three wrappers

```ts
export function listSessions(): Promise<Session[]> { return invoke("list_sessions"); }
export function getSessionDetail(sessionId: string): Promise<SessionDetail | null> { return invoke("get_session_detail", { sessionId }); }
export function openInEditor(path: string): Promise<void> { return invoke("open_in_editor", { path }); }
```

(Match the exact casing Tauri expects for command args — Tauri's default arg-name convention converts Rust's `snake_case` parameter names to `camelCase` on the JS side, which is why `session_id` becomes `sessionId` and `path` stays `path`. `listProjects` already establishes this file's pattern; follow it.)

### 4. New file `src/components/ui/Modal.tsx` (+ `Modal.css`)

Minimal, reusable: takes `isOpen: boolean`, `onClose: () => void`, `children: React.ReactNode` (add a `title?: string` prop if useful for `SessionDetailModal`'s header — your call). Renders a backdrop + centered panel; clicking the backdrop or pressing Escape closes it (standard modal affordances — don't skip these, they're expected baseline behavior, not gold-plating). Returns `null` when `isOpen` is `false`. No portal/`createPortal` requirement — a fixed-position `div` rendered inline is fine at this app's scale, don't over-engineer.

### 5. New file `src/views/SessionsView.tsx` (+ CSS)

- `useQuery({ queryKey: ["sessions"], queryFn: listSessions })` and `useQuery({ queryKey: ["projects"], queryFn: listProjects })` (for the project-name lookup described above).
- Explicit loading/error/empty states, following `ProjectsView.tsx`'s pattern (empty-state copy is your call — something in the spirit of "No sessions yet").
- Renders a list of session rows (build `SessionRow` as a sub-component in the same file or a separate `src/views/SessionRow.tsx` — your call given how much logic it ends up with; keep it simple either way). Each row shows at minimum: project name (via the lookup), a status `Pill` (`"active"` vs `"ended"` — pick reasonable `tone` values, e.g. `green` for active, `gray` for ended, consistent with `Pill`'s existing `tone` prop options in `src/components/ui/Pill.tsx`), model string, relative last-activity time (there's already a `formatRelativeTime` helper duplicated in `ProjectCard.tsx` — extract it to a small shared util, e.g. `src/lib/format.ts`, and use it from both `ProjectCard.tsx` and your new row component, rather than copy-pasting it a second time), token counts or cost (`$${cost_usd.toFixed(2)}` — `ProjectCard.tsx` already does this pattern for a project's total), and `summary` if present (render nothing / a placeholder like "No summary yet" if `null` — don't hide the row, just don't show a summary).
- Clicking a row opens `SessionDetailModal` for that session (local `useState<string | null>` for "which session id is currently open," or your own reasonable state approach).

### 6. New file `src/views/SessionDetailModal.tsx` (+ CSS)

- Takes the selected `sessionId` (or `null`) and a close handler; renders nothing when no session is selected.
- Fetches full detail via `useQuery({ queryKey: ["session-detail", sessionId], queryFn: () => getSessionDetail(sessionId!), enabled: sessionId !== null })`.
- Uses the `Modal` component from step 4.
- Shows: session stats (tokens — prompt/completion/cache read/cache creation, cost, lines added/removed, duration if `ended`, status, model, tags if present as a list of `Pill`s, summary if present) and the list of `files_changed` (file path, change type, lines added/removed per file). For each file in `files_changed`, a small "Open" button/link calling `openInEditor(file_path)` (fire-and-forget is fine — `.catch()` and log/no-op on failure, or show a lightweight inline error; don't let a failed spawn crash the modal).
- Handle the loading state (detail still fetching) and the "session not found" case (`getSessionDetail` resolved to `null` — can happen if a session id from a stale list no longer exists, edge case worth a one-line message rather than a blank modal or crash).

### 7. `src/App.tsx`: wire it in

Replace the `"sessions"` branch's `<ComingSoonView title="Sessions" />` with `<SessionsView />`. Don't touch the other branches (`"timeline"`, `"connections"`, `"agent-manager"` stay as `ComingSoonView` — Timeline is a separate task).

## Explicitly out of scope for this task

- No Timeline view, no tag-classification UI, no AI-summary generation trigger (summaries/tags are populated by a later backend task — this task only needs to *display* them if present, and handle `null` gracefully).
- No changes to `list_sessions`/`get_session_detail` on the Rust side beyond adding `open_in_editor` alongside them — the query logic itself is already correct and tested.
- No `StatTile`/`Select`/`TextInput`/`ActivityBars` components — build only `Modal`, which this task needs.
- No broad retrofit of `ProjectsView`/`ProjectCard`/`Sidebar` styling — touch them only for the `formatRelativeTime` extraction described above (a genuine shared-utility fix, not a design pass).

## Verification

- `npm run build` (`tsc -b && vite build`) must succeed with no type errors.
- `npm run lint` (oxlint) must be clean.
- `cd src-tauri && cargo build` must succeed (verifying `open_in_editor` compiles and is registered).
- `cd src-tauri && cargo test` must still show all previously-passing tests green (you're not expected to add Rust tests for `open_in_editor` — it's a thin OS-process-spawning wrapper with no meaningful pure-logic to unit test at this scale, but note in your report that you considered it).
- A dev-server smoke check (`npm run dev`, confirm no console errors, stop it) — same bar as Task A used, since a real Tauri IPC bridge isn't available outside the actual app window in this environment; `list_sessions`/`get_session_detail`/`open_in_editor` calls will reject in a browser-only context, which is expected and should surface as your `SessionsView`'s handled error state, not a crash.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-E-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
