-- One row per billable API response, so usage that agent logs repeat across several lines
-- (Claude Code: one line per content block; Codex: re-emitted token_count events) is stored
-- once. Session token and cost totals are sums over these rows for agents whose logs expose
-- a response identity; other sessions keep the older additive totals.
CREATE TABLE IF NOT EXISTS session_usage (
  session_id       TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  usage_key        TEXT NOT NULL,
  model            TEXT,
  speed            TEXT,
  input_tokens     INTEGER NOT NULL DEFAULT 0,
  output_tokens    INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  cache_write_5m_tokens INTEGER NOT NULL DEFAULT 0,
  cache_write_1h_tokens INTEGER NOT NULL DEFAULT 0,
  -- NULL when the model has no known price (e.g. non-Anthropic models).
  cost_usd         REAL,
  PRIMARY KEY (session_id, usage_key)
);

ALTER TABLE sessions ADD COLUMN cache_creation_1h_tokens INTEGER NOT NULL DEFAULT 0;
-- 1 when some of this session's usage has no known price, so its cost_usd is incomplete.
ALTER TABLE sessions ADD COLUMN cost_unpriced INTEGER NOT NULL DEFAULT 0;
