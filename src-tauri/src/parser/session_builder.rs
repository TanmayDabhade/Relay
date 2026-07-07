use super::claude_jsonl::{ParsedRecord, ToolUse};
use crate::db::queries::{self, TokenDelta};
use rusqlite::Connection;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// What changed as a result of ingesting one record — lets the watcher decide what
/// `data-changed` events to emit without re-querying the DB just to find out.
#[derive(Debug, Default)]
pub struct IngestOutcome {
    pub project_touched: Option<String>,
    pub session_created: Option<String>,
    pub session_updated: Option<String>,
}

pub fn ingest_record(
    conn: &Connection,
    raw_log_path: &str,
    record: ParsedRecord,
) -> anyhow::Result<IngestOutcome> {
    let mut outcome = IngestOutcome::default();

    // No cwd means we have no idea which project this belongs to — nothing to persist.
    let Some(cwd) = record.cwd.clone() else {
        return Ok(outcome);
    };
    // No timestamp means we can't place this in time (real logs have such lines, e.g.
    // some records preceding the first user/attachment record) — skip persisting.
    let Some(timestamp) = record.timestamp else {
        return Ok(outcome);
    };

    let project_id = project_id_for_path(&cwd);
    let project_name = std::path::Path::new(&cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&cwd)
        .to_string();

    queries::upsert_project(conn, &project_id, &project_name, &cwd, timestamp)?;
    outcome.project_touched = Some(project_id.clone());

    // `system` records only carry cwd/gitBranch metadata refresh, no session content.
    let Some(session_id) = record.session_id.clone() else {
        return Ok(outcome);
    };

    let delta = record
        .usage
        .as_ref()
        .map(|u| TokenDelta {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            cache_read_tokens: u.cache_read_input_tokens,
            cache_creation_tokens: u.cache_creation_input_tokens,
        })
        .unwrap_or_default();

    let created = queries::upsert_session(
        conn,
        &session_id,
        &project_id,
        record.model.as_deref(),
        timestamp,
        raw_log_path,
        &delta,
    )?;

    if created {
        outcome.session_created = Some(session_id.clone());
    } else {
        outcome.session_updated = Some(session_id.clone());
    }

    for tool_use in &record.tool_uses {
        ingest_tool_use(conn, &session_id, timestamp, tool_use)?;
    }

    Ok(outcome)
}

fn ingest_tool_use(
    conn: &Connection,
    session_id: &str,
    occurred_at: i64,
    tool_use: &ToolUse,
) -> anyhow::Result<()> {
    match tool_use {
        ToolUse::Write { file_path, content } => {
            // No pre-write file state is available from the log alone, so lines_removed is
            // always 0 here — true before/after diffing is the git-diff fallback (Phase 3).
            let lines_added = content.lines().count() as i64;
            queries::insert_file_changed(
                conn, session_id, file_path, "write", lines_added, 0, occurred_at,
            )?;
        }
        ToolUse::Edit {
            file_path,
            old_string,
            new_string,
        } => {
            let (added, removed) = diff_counts(old_string, new_string);
            queries::insert_file_changed(
                conn, session_id, file_path, "edit", added, removed, occurred_at,
            )?;
        }
        ToolUse::MultiEdit { file_path, edits } => {
            for (old, new) in edits {
                let (added, removed) = diff_counts(old, new);
                queries::insert_file_changed(
                    conn,
                    session_id,
                    file_path,
                    "multi_edit",
                    added,
                    removed,
                    occurred_at,
                )?;
            }
        }
        ToolUse::NotebookEdit {
            file_path,
            old_string,
            new_string,
        } => {
            let (added, removed) = diff_counts(old_string.as_deref().unwrap_or(""), new_string);
            queries::insert_file_changed(
                conn,
                session_id,
                file_path,
                "notebook_edit",
                added,
                removed,
                occurred_at,
            )?;
        }
    }
    Ok(())
}

fn diff_counts(old: &str, new: &str) -> (i64, i64) {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0i64;
    let mut removed = 0i64;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

/// Stable id derived from the real filesystem path (never the dash-encoded log directory
/// name, which is ambiguous when the real path itself contains hyphens).
fn project_id_for_path(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_lowercase().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
