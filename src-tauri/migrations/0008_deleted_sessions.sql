-- App-local deletion marker. Agent CLIs own their raw logs, so deleting a session from Relay
-- removes Relay's derived data but keeps the source file and prevents future watcher records
-- for the same native session id from recreating it.
CREATE TABLE IF NOT EXISTS deleted_sessions (
  session_id   TEXT PRIMARY KEY,
  raw_log_path TEXT NOT NULL,
  deleted_at   INTEGER NOT NULL
);

