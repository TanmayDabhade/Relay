-- Relay-owned agent dispatch. Agent configuration is intentionally local: Relay only
-- launches CLIs installed on this machine and never stores provider credentials itself.
-- The launch-side agent stamp lets concurrent runs by different agents in one project adopt
-- the correct board card when their native session logs appear asynchronously.
ALTER TABLE cards ADD COLUMN pending_launch_agent TEXT;

CREATE TABLE IF NOT EXISTS agent_configs (
  agent         TEXT PRIMARY KEY,
  enabled       INTEGER NOT NULL DEFAULT 1,
  executable    TEXT NOT NULL,
  models        TEXT NOT NULL,
  default_model TEXT NOT NULL DEFAULT 'default',
  updated_at    INTEGER NOT NULL
);

INSERT OR IGNORE INTO agent_configs (agent, enabled, executable, models, default_model, updated_at)
VALUES
  ('claude', 1, 'claude', '["default","sonnet","opus","haiku"]', 'default', unixepoch()),
  ('codex',  1, 'codex',  '["default"]', 'default', unixepoch()),
  ('gemini', 1, 'gemini', '["default","auto","gemini-3-pro-preview","gemini-3-flash-preview"]', 'auto', unixepoch()),
  ('cursor', 1, 'cursor-agent', '["default","gpt-5"]', 'default', unixepoch());

-- A task is the durable intent. Retrying creates another dispatch_run rather than mutating
-- the earlier attempt, so the global viewer keeps the full workday history.
CREATE TABLE IF NOT EXISTS dispatch_tasks (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL REFERENCES projects(id),
  card_id      TEXT REFERENCES cards(id) ON DELETE SET NULL,
  title        TEXT NOT NULL,
  prompt       TEXT NOT NULL,
  status       TEXT NOT NULL DEFAULT 'queued',
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  completed_at INTEGER
);

CREATE TABLE IF NOT EXISTS dispatch_runs (
  id          TEXT PRIMARY KEY,
  task_id     TEXT NOT NULL REFERENCES dispatch_tasks(id) ON DELETE CASCADE,
  attempt     INTEGER NOT NULL,
  agent       TEXT NOT NULL REFERENCES agent_configs(agent),
  model       TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'queued',
  started_at  INTEGER,
  ended_at    INTEGER,
  exit_code   INTEGER,
  error       TEXT,
  session_id  TEXT REFERENCES sessions(id),
  created_at  INTEGER NOT NULL,
  UNIQUE(task_id, attempt)
);

-- PTY output is chunked rather than line-oriented: interactive terminal programs can emit
-- partial lines, spinners, and permission prompts without a newline.
CREATE TABLE IF NOT EXISTS dispatch_run_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id     TEXT NOT NULL REFERENCES dispatch_runs(id) ON DELETE CASCADE,
  sequence   INTEGER NOT NULL,
  stream     TEXT NOT NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(run_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_tasks_created ON dispatch_tasks(created_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_tasks_project ON dispatch_tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_runs_task ON dispatch_runs(task_id, attempt);
CREATE INDEX IF NOT EXISTS idx_dispatch_runs_status ON dispatch_runs(status);
CREATE INDEX IF NOT EXISTS idx_dispatch_run_events_run ON dispatch_run_events(run_id, sequence);
