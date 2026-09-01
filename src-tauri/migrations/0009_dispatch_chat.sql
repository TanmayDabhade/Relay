-- Structured, resumable chat for Relay-owned agent sessions. The original
-- dispatch_run_events table is retained so pre-chat PTY history stays readable.
ALTER TABLE dispatch_runs ADD COLUMN provider_session_id TEXT;
ALTER TABLE dispatch_runs ADD COLUMN shutdown_at INTEGER;

CREATE TABLE IF NOT EXISTS dispatch_turns (
  id          TEXT PRIMARY KEY,
  run_id      TEXT NOT NULL REFERENCES dispatch_runs(id) ON DELETE CASCADE,
  sequence    INTEGER NOT NULL,
  prompt      TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'running',
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  error       TEXT,
  UNIQUE(run_id, sequence)
);

CREATE TABLE IF NOT EXISTS dispatch_events (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id            TEXT NOT NULL REFERENCES dispatch_runs(id) ON DELETE CASCADE,
  turn_id           TEXT REFERENCES dispatch_turns(id) ON DELETE SET NULL,
  sequence          INTEGER NOT NULL,
  kind              TEXT NOT NULL,
  role              TEXT,
  content           TEXT NOT NULL DEFAULT '',
  payload           TEXT,
  provider_event_id TEXT,
  state             TEXT,
  created_at        INTEGER NOT NULL,
  UNIQUE(run_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_turns_run ON dispatch_turns(run_id, sequence);
CREATE INDEX IF NOT EXISTS idx_dispatch_events_run ON dispatch_events(run_id, sequence);
CREATE INDEX IF NOT EXISTS idx_dispatch_events_provider
  ON dispatch_events(run_id, provider_event_id);
