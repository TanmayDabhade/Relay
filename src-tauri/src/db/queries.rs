use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub lang: Option<String>,
    pub stack: Option<String>,
    pub created_at: i64,
    pub last_active: i64,
    pub session_count: i64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub agent: String,
    pub model: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub last_activity_at: i64,
    pub status: String,
    pub duration_seconds: Option<i64>,
    pub summary: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost_usd: f64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub tags: Option<String>,
    pub raw_log_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileChanged {
    pub id: i64,
    pub session_id: String,
    pub file_path: String,
    pub change_type: String,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct TokenDelta {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
}

#[derive(Debug, Clone)]
pub struct IngestState {
    pub byte_offset: i64,
    pub partial_line: String,
}

/// For recomputing cost_usd against the current pricing table without re-parsing logs.
pub struct SessionTokenTotals {
    pub id: String,
    pub model: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
}

fn row_to_session(row: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        project_id: row.get(1)?,
        agent: row.get(2)?,
        model: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        last_activity_at: row.get(6)?,
        status: row.get(7)?,
        duration_seconds: row.get(8)?,
        summary: row.get(9)?,
        prompt_tokens: row.get(10)?,
        completion_tokens: row.get(11)?,
        cache_read_tokens: row.get(12)?,
        cache_creation_tokens: row.get(13)?,
        cost_usd: row.get(14)?,
        lines_added: row.get(15)?,
        lines_removed: row.get(16)?,
        tags: row.get(17)?,
        raw_log_path: row.get(18)?,
    })
}

const SESSION_COLUMNS: &str = "id, project_id, agent, model, started_at, ended_at, last_activity_at, status,
     duration_seconds, summary, prompt_tokens, completion_tokens, cache_read_tokens,
     cache_creation_tokens, cost_usd, lines_added, lines_removed, tags, raw_log_path";

// --- Ingest (parser/watcher) side ---

pub fn upsert_project(
    conn: &Connection,
    id: &str,
    name: &str,
    path: &str,
    timestamp: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO projects (id, name, path, created_at, last_active)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
            last_active = MAX(last_active, excluded.last_active)",
        params![id, name, path, timestamp],
    )?;
    Ok(())
}

/// Upserts a session with monotonic accumulation of token deltas — safe to
/// replay if a log file is ever re-scanned from offset 0. Returns true if a
/// new session row was created (vs. an existing one updated).
pub fn upsert_session(
    conn: &Connection,
    session_id: &str,
    project_id: &str,
    model: Option<&str>,
    timestamp: i64,
    raw_log_path: &str,
    delta: &TokenDelta,
) -> rusqlite::Result<bool> {
    let existed: bool = conn
        .query_row(
            "SELECT 1 FROM sessions WHERE id = ?1",
            params![session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    conn.execute(
        "INSERT INTO sessions (
            id, project_id, agent, model, started_at, last_activity_at, status,
            prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens,
            raw_log_path
         )
         VALUES (?1, ?2, 'claude', ?3, ?4, ?4, 'active', ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            model = COALESCE(excluded.model, model),
            started_at = MIN(started_at, excluded.started_at),
            last_activity_at = MAX(last_activity_at, excluded.last_activity_at),
            status = 'active',
            prompt_tokens = prompt_tokens + excluded.prompt_tokens,
            completion_tokens = completion_tokens + excluded.completion_tokens,
            cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
            cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens",
        params![
            session_id,
            project_id,
            model,
            timestamp,
            delta.prompt_tokens,
            delta.completion_tokens,
            delta.cache_read_tokens,
            delta.cache_creation_tokens,
            raw_log_path,
        ],
    )?;

    Ok(!existed)
}

pub fn insert_file_changed(
    conn: &Connection,
    session_id: &str,
    file_path: &str,
    change_type: &str,
    lines_added: i64,
    lines_removed: i64,
    occurred_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO files_changed (session_id, file_path, change_type, lines_added, lines_removed, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, file_path, change_type, lines_added, lines_removed, occurred_at],
    )?;
    conn.execute(
        "UPDATE sessions SET lines_added = lines_added + ?2, lines_removed = lines_removed + ?3 WHERE id = ?1",
        params![session_id, lines_added, lines_removed],
    )?;
    Ok(())
}

pub fn get_ingest_state(conn: &Connection, file_path: &str) -> rusqlite::Result<IngestState> {
    let result = conn
        .query_row(
            "SELECT byte_offset, partial_line FROM ingest_state WHERE file_path = ?1",
            params![file_path],
            |row| {
                Ok(IngestState {
                    byte_offset: row.get(0)?,
                    partial_line: row.get(1)?,
                })
            },
        )
        .optional()?;
    Ok(result.unwrap_or(IngestState {
        byte_offset: 0,
        partial_line: String::new(),
    }))
}

pub fn set_ingest_state(
    conn: &Connection,
    file_path: &str,
    byte_offset: i64,
    partial_line: &str,
    mtime: i64,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO ingest_state (file_path, byte_offset, partial_line, last_mtime, last_ingested_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(file_path) DO UPDATE SET
            byte_offset = excluded.byte_offset,
            partial_line = excluded.partial_line,
            last_mtime = excluded.last_mtime,
            last_ingested_at = excluded.last_ingested_at",
        params![file_path, byte_offset, partial_line, mtime, now],
    )?;
    Ok(())
}

// --- Read side (frontend commands) ---

pub fn list_projects(conn: &Connection) -> rusqlite::Result<Vec<ProjectSummary>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.path, p.lang, p.stack, p.created_at, p.last_active,
                COUNT(s.id) as session_count,
                COALESCE(SUM(s.cost_usd), 0.0) as total_cost_usd
         FROM projects p
         LEFT JOIN sessions s ON s.project_id = p.id
         GROUP BY p.id
         ORDER BY p.last_active DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ProjectSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            lang: row.get(3)?,
            stack: row.get(4)?,
            created_at: row.get(5)?,
            last_active: row.get(6)?,
            session_count: row.get(7)?,
            total_cost_usd: row.get(8)?,
        })
    })?;
    rows.collect()
}

pub fn list_sessions(conn: &Connection) -> rusqlite::Result<Vec<Session>> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions ORDER BY last_activity_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.collect()
}

pub fn get_session_detail(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<(Session, Vec<FileChanged>)>> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1");
    let session = conn
        .query_row(&sql, params![session_id], row_to_session)
        .optional()?;

    let Some(session) = session else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT id, session_id, file_path, change_type, lines_added, lines_removed, occurred_at
         FROM files_changed WHERE session_id = ?1 ORDER BY occurred_at ASC",
    )?;
    let files = stmt
        .query_map(params![session_id], |row| {
            Ok(FileChanged {
                id: row.get(0)?,
                session_id: row.get(1)?,
                file_path: row.get(2)?,
                change_type: row.get(3)?,
                lines_added: row.get(4)?,
                lines_removed: row.get(5)?,
                occurred_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some((session, files)))
}

// --- Idle-session sweep / summarization / tagging (Phase 2) ---

pub fn sessions_to_finalize(
    conn: &Connection,
    idle_threshold_secs: i64,
    now: i64,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM sessions WHERE status = 'active' AND (?1 - last_activity_at) > ?2",
    )?;
    let ids = stmt
        .query_map(params![now, idle_threshold_secs], |row| row.get(0))?
        .collect();
    ids
}

pub fn finalize_session(conn: &Connection, session_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions
         SET status = 'ended',
             ended_at = last_activity_at,
             duration_seconds = last_activity_at - COALESCE(started_at, last_activity_at)
         WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

pub fn sessions_needing_summary(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM sessions WHERE status = 'ended' AND summary IS NULL")?;
    let ids = stmt.query_map([], |row| row.get(0))?.collect();
    ids
}

pub fn update_summary(conn: &Connection, session_id: &str, summary: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET summary = ?2 WHERE id = ?1",
        params![session_id, summary],
    )?;
    Ok(())
}

pub fn sessions_needing_tags(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM sessions WHERE status = 'ended' AND tags IS NULL")?;
    let ids = stmt.query_map([], |row| row.get(0))?.collect();
    ids
}

pub fn update_tags(conn: &Connection, session_id: &str, tags_json: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET tags = ?2 WHERE id = ?1",
        params![session_id, tags_json],
    )?;
    Ok(())
}

/// Looks up just `raw_log_path` for a single session — used by the tag-classification sweep,
/// which needs this one field per id and shouldn't pull a full `list_sessions()` scan to get
/// it.
pub fn session_raw_log_path(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT raw_log_path FROM sessions WHERE id = ?1",
        params![session_id],
        |row| row.get(0),
    )
    .optional()
}

// --- Cost recompute (pricing-table edits applied without re-parsing logs) ---

pub fn update_cost(conn: &Connection, session_id: &str, cost_usd: f64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET cost_usd = ?2 WHERE id = ?1",
        params![session_id, cost_usd],
    )?;
    Ok(())
}

/// Single-session variant of `all_session_token_totals`, used right after `upsert_session` to
/// read back the now-updated accumulated totals for cost recomputation (see
/// `session_builder::ingest_record`) without re-parsing logs.
pub fn session_token_totals(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<SessionTokenTotals>> {
    conn.query_row(
        "SELECT id, model, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens
         FROM sessions WHERE id = ?1",
        params![session_id],
        |row| {
            Ok(SessionTokenTotals {
                id: row.get(0)?,
                model: row.get(1)?,
                prompt_tokens: row.get(2)?,
                completion_tokens: row.get(3)?,
                cache_read_tokens: row.get(4)?,
                cache_creation_tokens: row.get(5)?,
            })
        },
    )
    .optional()
}

pub fn all_session_token_totals(conn: &Connection) -> rusqlite::Result<Vec<SessionTokenTotals>> {
    let mut stmt = conn.prepare(
        "SELECT id, model, prompt_tokens, completion_tokens, cache_read_tokens, cache_creation_tokens
         FROM sessions",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SessionTokenTotals {
            id: row.get(0)?,
            model: row.get(1)?,
            prompt_tokens: row.get(2)?,
            completion_tokens: row.get(3)?,
            cache_read_tokens: row.get(4)?,
            cache_creation_tokens: row.get(5)?,
        })
    })?;
    rows.collect()
}
