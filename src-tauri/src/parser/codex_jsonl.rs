//! Defensive, best-effort parser for OpenAI Codex CLI's rollout log format
//! (`~/.codex/sessions/**/rollout-*.jsonl`).
//!
//! **Unlike `claude_jsonl.rs`, this has not been verified against real Codex CLI output** —
//! there was no captured log available while building this. The line shapes below (a
//! `session_meta` header carrying `id`/`cwd`, `response_item` turns wrapping OpenAI
//! Responses-API-style items, and `event_msg` telemetry like `token_count`) reflect the best
//! available public/training knowledge of Codex CLI's rollout format as of this writing, not a
//! ground-truth sample. Treat every field access as a guess: `Option`-based throughout, no
//! `.unwrap()`/`.expect()`, unrecognized shapes are skipped rather than treated as fatal. This
//! is expected to need adjustment once real rollout files can be inspected.
//!
//! A structural difference from Claude Code's format (the reason this module needs a
//! per-file cache, unlike the fully self-contained `claude_jsonl`): Codex's `cwd` and session
//! id are only expected to appear once, on the file's leading `session_meta` line, not
//! repeated on every subsequent line. Every other record in the same file only carries a
//! `timestamp`. So `parse_line` is keyed by `raw_log_path` and remembers the last-seen
//! `session_meta` for that specific file, applying it to later lines from the same file.

use super::record::{ParsedRecord, Usage};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
struct FileContext {
    session_id: Option<String>,
    cwd: Option<String>,
    /// From the latest `turn_context` line; token_count events don't name their model.
    model: Option<String>,
}

fn context_cache() -> &'static Mutex<HashMap<String, FileContext>> {
    static CACHE: OnceLock<Mutex<HashMap<String, FileContext>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parses a single complete JSONL line from a Codex CLI rollout file. `raw_log_path` is used
/// purely as a cache key to remember this file's `session_meta` across calls — never persisted
/// or otherwise interpreted.
pub fn parse_line(line: &str, raw_log_path: &str) -> Option<ParsedRecord> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "skipping malformed Codex CLI rollout JSONL line ({} bytes): {e}",
                trimmed.len()
            );
            return None;
        }
    };

    let record_type = value.get("type").and_then(Value::as_str)?.to_string();
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp());
    let payload = value.get("payload");

    match record_type.as_str() {
        "session_meta" => {
            let session_id = payload
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
                .map(String::from);
            let cwd = payload
                .and_then(|p| p.get("cwd"))
                .and_then(Value::as_str)
                .map(String::from);

            let mut cache = context_cache().lock().unwrap();
            cache.insert(
                raw_log_path.to_string(),
                FileContext {
                    session_id: session_id.clone(),
                    cwd: cwd.clone(),
                    model: None,
                },
            );
            drop(cache);

            Some(ParsedRecord {
                record_type,
                agent: "codex",
                cwd,
                git_branch: None,
                session_id,
                timestamp,
                model: None,
                usage: None,
                usage_key: None,
                tool_uses: Vec::new(),
                text: None,
                ai_title: None,
            })
        }
        "response_item" => {
            let ctx = cached_context(raw_log_path);
            let item_type = payload.and_then(|p| p.get("type")).and_then(Value::as_str);

            let text = match item_type {
                Some("message") => extract_message_text(payload?),
                _ => None,
            };

            Some(ParsedRecord {
                record_type,
                agent: "codex",
                cwd: ctx.cwd,
                git_branch: None,
                session_id: ctx.session_id,
                timestamp,
                model: None,
                usage: None,
                usage_key: None,
                tool_uses: Vec::new(),
                text,
                ai_title: None,
            })
        }
        "turn_context" => {
            // Carries the model for the turns that follow; nothing else to persist.
            if let Some(model) = payload.and_then(|p| p.get("model")).and_then(Value::as_str) {
                if let Some(ctx) = context_cache().lock().unwrap().get_mut(raw_log_path) {
                    ctx.model = Some(model.to_string());
                }
            }
            None
        }
        "event_msg" => {
            let ctx = cached_context(raw_log_path);
            let event_type = payload.and_then(|p| p.get("type")).and_then(Value::as_str);

            let (usage, usage_key) = if event_type == Some("token_count") {
                extract_token_usage(payload?).unzip()
            } else {
                (None, None)
            };

            Some(ParsedRecord {
                record_type,
                agent: "codex",
                cwd: ctx.cwd,
                git_branch: None,
                session_id: ctx.session_id,
                timestamp,
                model: usage.as_ref().and(ctx.model),
                usage,
                usage_key,
                tool_uses: Vec::new(),
                text: None,
                ai_title: None,
            })
        }
        _ => None,
    }
}

fn cached_context(raw_log_path: &str) -> FileContext {
    context_cache()
        .lock()
        .unwrap()
        .get(raw_log_path)
        .cloned()
        .unwrap_or_default()
}

/// Best-effort extraction of a `response_item` message's text: OpenAI Responses-API-shaped
/// content blocks with `"type": "input_text"` (user) or `"output_text"` (assistant).
fn extract_message_text(payload: &Value) -> Option<String> {
    let items = payload.get("content").and_then(Value::as_array)?;
    let blocks: Vec<&str> = items
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("input_text") | Some("output_text")
            )
        })
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect();
    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n"))
    }
}

/// Usage for one `token_count` event, verified against real rollout files (2026-08):
///
/// - `info.total_token_usage` is the **cumulative** session total and `info.last_token_usage`
///   the request that just finished. Summing cumulative totals line by line (what this used to
///   do) grows quadratically with turn count, so only `last_token_usage` is used.
/// - OpenAI's `input_tokens` *includes* `cached_input_tokens`; the uncached share is the
///   difference, so cached tokens aren't billed twice.
/// - Codex re-emits an unchanged `token_count` (about 1 in 6 events, e.g. on rate-limit
///   refreshes). The cumulative `total_tokens` identifies the request, so a repeat carries the
///   same key and ingest stores it once.
///
/// Events with no `info` (rate-limit-only updates) carry no usage.
fn extract_token_usage(payload: &Value) -> Option<(Usage, String)> {
    let info = payload.get("info")?;
    let last = info.get("last_token_usage")?;
    let cumulative = info
        .get("total_token_usage")
        .and_then(|t| t.get("total_tokens"))
        .and_then(Value::as_i64)?;
    let field = |name: &str| last.get(name).and_then(Value::as_i64).unwrap_or(0).max(0);
    let cached = field("cached_input_tokens");
    let usage = Usage {
        input_tokens: (field("input_tokens") - cached).max(0),
        output_tokens: field("output_tokens"),
        cache_read_input_tokens: cached,
        cache_creation_input_tokens: field("cache_write_input_tokens"),
        cache_creation_1h_input_tokens: 0,
        speed: None,
    };
    Some((usage, format!("total:{cumulative}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test uses its own fake path, since `parse_line`'s per-file cache is keyed by
    // `raw_log_path` — tests run in parallel threads, so sharing one path across tests races
    // on that global cache (distinct real files never collide this way in production).

    #[test]
    fn session_meta_yields_cwd_and_session_id() {
        let path = "/Users/testuser/.codex/sessions/2026/01/01/rollout-fixture-a.jsonl";
        let line = r#"{"timestamp":"2026-01-01T10:00:00Z","type":"session_meta","payload":{"id":"cx-1234","cwd":"/Users/testuser/Desktop/fixture-project"}}"#;
        let record = parse_line(line, path).expect("session_meta should parse");
        assert_eq!(record.agent, "codex");
        assert_eq!(record.session_id.as_deref(), Some("cx-1234"));
        assert_eq!(
            record.cwd.as_deref(),
            Some("/Users/testuser/Desktop/fixture-project")
        );
    }

    #[test]
    fn response_item_after_session_meta_inherits_cached_cwd_and_session_id() {
        let path = "/Users/testuser/.codex/sessions/2026/01/01/rollout-fixture-b.jsonl";
        let meta = r#"{"timestamp":"2026-01-01T10:00:00Z","type":"session_meta","payload":{"id":"cx-5678","cwd":"/tmp/proj"}}"#;
        parse_line(meta, path).unwrap();

        let user_msg = r#"{"timestamp":"2026-01-01T10:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello codex"}]}}"#;
        let record = parse_line(user_msg, path).expect("response_item should parse");
        assert_eq!(record.session_id.as_deref(), Some("cx-5678"));
        assert_eq!(record.cwd.as_deref(), Some("/tmp/proj"));
        assert_eq!(record.text.as_deref(), Some("hello codex"));
    }

    #[test]
    fn token_count_event_extracts_usage() {
        let path = "/Users/testuser/.codex/sessions/2026/01/01/rollout-fixture-c.jsonl";
        let meta = r#"{"timestamp":"2026-01-01T10:00:00Z","type":"session_meta","payload":{"id":"cx-9","cwd":"/tmp/proj2"}}"#;
        parse_line(meta, path).unwrap();

        let turn = r#"{"timestamp":"2026-01-01T10:00:01Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#;
        assert!(parse_line(turn, path).is_none());

        // Second request of a session: cumulative totals include the first request.
        let event = r#"{"timestamp":"2026-01-01T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":40512,"cached_input_tokens":22016,"output_tokens":434,"total_tokens":40946},"last_token_usage":{"input_tokens":21808,"cached_input_tokens":11008,"output_tokens":184,"total_tokens":21992}}}}"#;
        let record = parse_line(event, path).expect("event_msg should parse");
        let usage = record.usage.expect("token_count event should carry usage");
        assert_eq!(
            usage.input_tokens,
            21808 - 11008,
            "cached tokens are not input"
        );
        assert_eq!(usage.output_tokens, 184);
        assert_eq!(usage.cache_read_input_tokens, 11008);
        assert_eq!(record.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(record.usage_key.as_deref(), Some("total:40946"));

        // A re-emitted identical event produces the same key, so it is stored once.
        let repeat = parse_line(event, path).unwrap();
        assert_eq!(repeat.usage_key, record.usage_key);

        // Rate-limit-only updates carry no usage.
        let limits = r#"{"timestamp":"2026-01-01T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;
        assert!(parse_line(limits, path).unwrap().usage.is_none());
    }

    #[test]
    fn unknown_top_level_type_is_skipped_not_fatal() {
        let path = "/Users/testuser/.codex/sessions/2026/01/01/rollout-fixture-d.jsonl";
        assert!(parse_line(
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"some-future-codex-record"}"#,
            path
        )
        .is_none());
    }

    #[test]
    fn malformed_json_line_is_skipped_without_panicking() {
        let path = "/Users/testuser/.codex/sessions/2026/01/01/rollout-fixture-e.jsonl";
        assert!(parse_line("not json at all", path).is_none());
        assert!(parse_line("", path).is_none());
    }
}
