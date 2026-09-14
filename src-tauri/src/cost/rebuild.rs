//! One-time correction for sessions ingested before per-response usage existed. Their totals
//! were summed line by line, which over-counts Claude Code (usage repeated on every content
//! block line, ~1.5-2.3x on real logs) and wildly over-counts Codex (cumulative totals summed
//! as if they were per-request). Each such session's log is re-read once and stored as
//! `session_usage` rows; the session's totals then become the exact per-response sum.
//!
//! Sessions whose logs no longer exist (agents prune old logs) keep their old totals — there
//! is nothing left to recount from.

use crate::db::{queries, Db};
use crate::parser::{self, record::Usage, session_builder};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};

pub struct RebuildTarget {
    session_id: String,
    agent: String,
    raw_log_path: String,
}

/// Gather phase. Must run before the watcher starts: once the watcher records a live
/// session's first usage row, that session would no longer look like it needs a rebuild, and
/// its earlier over-counted history would be dropped instead of recounted.
pub fn gather(conn: &rusqlite::Connection) -> Vec<RebuildTarget> {
    match queries::sessions_needing_usage_rebuild(conn) {
        Ok(rows) => rows
            .into_iter()
            .map(|(session_id, agent, raw_log_path)| RebuildTarget {
                session_id,
                agent,
                raw_log_path,
            })
            .collect(),
        Err(error) => {
            log::warn!("could not list sessions for usage rebuild: {error:#}");
            Vec::new()
        }
    }
}

/// Compute + write phases, on a background thread. The log read and parse happen without the
/// DB lock; each session's writes then take the lock briefly. Inserts are idempotent
/// (`INSERT OR IGNORE` keyed by response), so racing the watcher on a live session is safe.
pub fn spawn(app: AppHandle, targets: Vec<RebuildTarget>) {
    if targets.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let mut rebuilt = 0usize;
        let mut missing = 0usize;
        for target in &targets {
            let Some(responses) = read_responses(target) else {
                missing += 1;
                continue;
            };
            let db = app.state::<Db>();
            let Ok(conn) = db.0.lock() else { return };
            for (key, model, usage) in &responses {
                if let Err(error) = session_builder::record_response_usage(
                    &conn,
                    &target.session_id,
                    key,
                    model.as_deref(),
                    usage,
                ) {
                    log::warn!("usage rebuild failed for {}: {error:#}", target.session_id);
                    break;
                }
            }
            rebuilt += 1;
        }
        log::info!(
            "usage rebuild: recounted {rebuilt} session(s) from logs; {missing} had no log left"
        );
        let _ = app.emit(
            "data-changed",
            serde_json::json!({ "entity": "session", "kind": "usage_rebuilt" }),
        );
    });
}

type Response = (String, Option<String>, Usage);

fn read_responses(target: &RebuildTarget) -> Option<Vec<Response>> {
    let path = Path::new(&target.raw_log_path);
    let contents = std::fs::read_to_string(path).ok()?;
    Some(responses_from_log(
        &target.agent,
        &target.session_id,
        path,
        &contents,
    ))
}

/// Every keyed usage record in a log that belongs to `session_id`, in file order.
pub fn responses_from_log(
    agent: &str,
    session_id: &str,
    path: &Path,
    contents: &str,
) -> Vec<Response> {
    let path_str = path.to_string_lossy();
    contents
        .lines()
        .filter_map(|line| match agent {
            "claude" => parser::parse_line(line),
            "codex" => parser::parse_codex_line(line, &path_str),
            _ => None,
        })
        .filter(|record| record.session_id.as_deref() == Some(session_id))
        .filter_map(|record| Some((record.usage_key?, record.model, record.usage?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_response_split_across_content_block_lines_is_counted_once() {
        // Real Claude Code shape: one API response (msg_1) logged as a text line and a
        // tool_use line, both carrying the full response usage.
        let log = [
            r#"{"type":"assistant","sessionId":"s1","cwd":"/p","timestamp":"2026-09-01T10:00:00Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5","content":[{"type":"text","text":"Looking"}],"usage":{"input_tokens":10,"output_tokens":128,"cache_read_input_tokens":14221,"cache_creation_input_tokens":500,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":500},"speed":"standard"}}}"#,
            r#"{"type":"assistant","sessionId":"s1","cwd":"/p","timestamp":"2026-09-01T10:00:01Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}],"usage":{"input_tokens":10,"output_tokens":128,"cache_read_input_tokens":14221,"cache_creation_input_tokens":500,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":500},"speed":"standard"}}}"#,
            r#"{"type":"assistant","sessionId":"s1","cwd":"/p","timestamp":"2026-09-01T10:00:05Z","requestId":"req_2","message":{"id":"msg_2","model":"claude-opus-5","content":[{"type":"text","text":"Done"}],"usage":{"input_tokens":2,"output_tokens":40,"cache_read_input_tokens":14800,"cache_creation_input_tokens":0}}}"#,
        ]
        .join("\n");
        let responses = responses_from_log("claude", "s1", Path::new("/tmp/s1.jsonl"), &log);
        let keys: std::collections::HashSet<_> = responses.iter().map(|r| r.0.clone()).collect();
        assert_eq!(keys.len(), 2, "two API responses, not three lines");
        assert_eq!(responses[0].2.cache_creation_1h_input_tokens, 500);
        assert_eq!(responses[0].2.speed.as_deref(), Some("standard"));
    }
}
