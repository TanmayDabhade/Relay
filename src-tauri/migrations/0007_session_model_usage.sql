-- Per-model, per-cache-TTL token breakdown for accurate cost.
--
-- Before this, a session stored a single `model` and one flat cache_creation bucket, and
-- cost was `f(that_one_model, aggregate_tokens)`. That over- or under-counted any session
-- whose tokens spanned more than one model (a Task-tool sub-agent on a cheaper model, or a
-- mid-session `/model` switch): every token was priced at whichever model happened to be
-- recorded last. It also couldn't tell 5-minute cache writes (1.25x input) from 1-hour ones
-- (2x input), so it priced all cache creation at the 5m rate.
--
-- This table records tokens keyed by (session, model, cache TTL) so cost becomes the SUM over
-- rows of `cost_usd(model, ...)`. `model = ''` is the "no model seen" bucket (priced at the
-- `_default` rate, matching the old `model: None` behavior). The sessions table keeps its
-- aggregate token columns for display — this table is consulted only for costing — and cost
-- is still recomputed from stored tokens at startup, so a pricing.json edit still corrects
-- history without re-parsing logs.
CREATE TABLE session_model_usage (
    session_id              TEXT NOT NULL,
    model                   TEXT NOT NULL,
    prompt_tokens           INTEGER NOT NULL DEFAULT 0,
    completion_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_creation_5m_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_1h_tokens INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (session_id, model),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Seed from existing sessions so historical costs still recompute. Per-record model and cache
-- TTL can't be recovered from the pre-migration aggregates, so each old session is seeded as a
-- single-model row with all cache creation treated as 5-minute — i.e. exactly the pre-migration
-- assumptions. Their cost is unchanged by the breakdown itself; it changes only through the
-- corrected pricing table (e.g. the Opus rate fix), which the startup backfill applies to these
-- seeded rows. Sessions ingested from here on get true per-model / per-TTL rows.
INSERT INTO session_model_usage (
    session_id, model, prompt_tokens, completion_tokens,
    cache_read_tokens, cache_creation_5m_tokens, cache_creation_1h_tokens
)
SELECT id, COALESCE(model, ''), prompt_tokens, completion_tokens,
       cache_read_tokens, cache_creation_tokens, 0
FROM sessions;
