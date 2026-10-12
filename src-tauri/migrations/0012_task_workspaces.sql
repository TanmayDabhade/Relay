-- Isolated git workspace for a dispatched task that should end in a pull request. Each such
-- task runs in its own `git worktree` on a `relay/<slug>` branch, so concurrent tasks in one
-- repo never share a checkout and a task's commit contains only that task's changes.
--
-- `work_dir` is where the agent actually runs: the worktree root plus the project's path
-- relative to its repo root (a project can be a subdirectory of a repo). Ingest maps any
-- session whose cwd falls under `work_dir` back onto `project_path`, so a worktree never
-- shows up as a phantom project of its own (see queries::project_path_for_worktree_cwd).
--
-- `ship_status`: 'pending' (not shipped yet) | 'shipping' (claimed by a ship attempt) |
-- 'shipped' (pushed, PR open) | 'no_changes' (nothing to commit or push) | 'failed'.
-- A shipped workspace returns to 'pending' only when a later turn ships again, which pushes
-- onto the same branch and therefore updates the same PR.
CREATE TABLE IF NOT EXISTS task_workspaces (
  task_id         TEXT PRIMARY KEY REFERENCES dispatch_tasks(id) ON DELETE CASCADE,
  project_path    TEXT NOT NULL,
  repo_root       TEXT NOT NULL,
  worktree_path   TEXT NOT NULL UNIQUE,
  work_dir        TEXT NOT NULL,
  branch          TEXT NOT NULL,
  base_branch     TEXT NOT NULL,
  auto_ship       INTEGER NOT NULL DEFAULT 1,
  ship_status     TEXT NOT NULL DEFAULT 'pending',
  ship_error      TEXT,
  pr_url          TEXT,
  last_shipped_at INTEGER,
  removed_at      INTEGER,
  created_at      INTEGER NOT NULL
);
