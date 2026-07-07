//! Defensive, best-effort parser for Claude Code's `~/.claude/projects/**/*.jsonl` format.
//!
//! This format is undocumented and explicitly marked internal by Anthropic — it may change
//! between Claude Code versions. Every field access here is `Option`-based on purpose: no
//! `.unwrap()`/`.expect()` on anything derived from a log line. An unrecognized `type` or a
//! malformed line is skipped and logged, never treated as fatal. Verified against real log
//! files on this machine (not just illustrative examples) as of Claude Code ~2.1.x.

use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Record types seen in real logs that carry no session content — safe to ignore.
const INERT_TYPES: &[&str] = &[
    "last-prompt",
    "mode",
    "permission-mode",
    "attachment",
    "file-history-snapshot",
    "ai-title",
    "queue-operation",
];

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub cache_creation_input_tokens: i64,
}

#[derive(Debug, Clone)]
pub enum ToolUse {
    Write {
        file_path: String,
        content: String,
    },
    Edit {
        file_path: String,
        old_string: String,
        new_string: String,
    },
    MultiEdit {
        file_path: String,
        edits: Vec<(String, String)>,
    },
    NotebookEdit {
        file_path: String,
        old_string: Option<String>,
        new_string: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedRecord {
    pub record_type: String,
    pub cwd: Option<String>,
    #[allow(dead_code)] // not yet surfaced in the UI; parsed for future stack/lang detection
    pub git_branch: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<i64>, // unix seconds
    pub model: Option<String>,
    pub usage: Option<Usage>,
    pub tool_uses: Vec<ToolUse>,
}

/// Parses a single complete JSONL line. Returns `None` for malformed JSON, records with no
/// `type` field, or record types that carry nothing worth persisting — never errors out, since
/// one bad/unknown line must never stop the rest of the file from being processed.
pub fn parse_line(line: &str) -> Option<ParsedRecord> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let value: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "skipping malformed Claude Code JSONL line ({} bytes): {e}",
                trimmed.len()
            );
            return None;
        }
    };

    let record_type = value.get("type").and_then(Value::as_str)?.to_string();

    match record_type.as_str() {
        "user" | "assistant" | "system" => {}
        t if INERT_TYPES.contains(&t) => return None,
        other => {
            log_unknown_type_once(other);
            return None;
        }
    }

    let cwd = value
        .get("cwd")
        .and_then(Value::as_str)
        .map(String::from);
    let git_branch = value
        .get("gitBranch")
        .and_then(Value::as_str)
        .map(String::from);
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .map(String::from);
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp());

    let message = value.get("message");
    let model = message
        .and_then(|m| m.get("model"))
        .and_then(Value::as_str)
        .map(String::from);

    let usage = message.and_then(|m| m.get("usage")).map(|u| Usage {
        input_tokens: u.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
        output_tokens: u.get("output_tokens").and_then(Value::as_i64).unwrap_or(0),
        cache_read_input_tokens: u
            .get("cache_read_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        cache_creation_input_tokens: u
            .get("cache_creation_input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    });

    let tool_uses = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(extract_tool_use).collect())
        .unwrap_or_default();

    Some(ParsedRecord {
        record_type,
        cwd,
        git_branch,
        session_id,
        timestamp,
        model,
        usage,
        tool_uses,
    })
}

fn extract_tool_use(item: &Value) -> Option<ToolUse> {
    if item.get("type").and_then(Value::as_str) != Some("tool_use") {
        return None;
    }
    let name = item.get("name").and_then(Value::as_str)?;
    let input = item.get("input")?;

    match name {
        "Write" => Some(ToolUse::Write {
            file_path: input.get("file_path").and_then(Value::as_str)?.to_string(),
            content: input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "Edit" => Some(ToolUse::Edit {
            file_path: input.get("file_path").and_then(Value::as_str)?.to_string(),
            old_string: input
                .get("old_string")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            new_string: input
                .get("new_string")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "MultiEdit" => {
            let file_path = input.get("file_path").and_then(Value::as_str)?.to_string();
            let edits = input
                .get("edits")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            let old = e.get("old_string").and_then(Value::as_str)?.to_string();
                            let new = e.get("new_string").and_then(Value::as_str)?.to_string();
                            Some((old, new))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(ToolUse::MultiEdit { file_path, edits })
        }
        // Best-effort: NotebookEdit's exact field names weren't confirmed against a real
        // captured record (only Write/Edit were). Never panics either way since every access
        // is Option-based; worst case this tool use is silently skipped.
        "NotebookEdit" => Some(ToolUse::NotebookEdit {
            file_path: input
                .get("notebook_path")
                .and_then(Value::as_str)?
                .to_string(),
            old_string: input
                .get("old_string")
                .and_then(Value::as_str)
                .map(String::from),
            new_string: input
                .get("new_source")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn log_unknown_type_once(record_type: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let set = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = set.lock().unwrap();
    if set.insert(record_type.to_string()) {
        log::warn!("encountered unknown Claude Code JSONL record type: {record_type:?} (ignoring)");
    }
}
