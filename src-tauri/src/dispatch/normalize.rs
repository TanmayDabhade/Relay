use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedEventUpdate {
    Append,
    Replace,
}

#[derive(Debug, Clone)]
pub struct NormalizedEvent {
    pub kind: String,
    pub role: Option<String>,
    pub content: String,
    pub payload: Option<String>,
    pub provider_event_id: Option<String>,
    pub state: Option<String>,
    pub update: NormalizedEventUpdate,
}

#[derive(Debug, Clone, Default)]
pub struct NormalizedLine {
    pub provider_session_id: Option<String>,
    pub events: Vec<NormalizedEvent>,
    pub turn_complete: bool,
}

#[derive(Debug, Clone, Default)]
pub struct NormalizeState {
    claude_message_id: Option<String>,
    gemini_message_id: Option<String>,
    gemini_message_sequence: u64,
    cursor_message_id: Option<String>,
    cursor_message_sequence: u64,
}

fn string_at(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        current.as_str().map(str::to_string)
    })
}

fn compact_json(value: &Value) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn text_from_content(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))
                    .or_else(|| {
                        item.get("content")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| value.get("content").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn event(
    kind: &str,
    role: Option<&str>,
    content: impl Into<String>,
    payload: Option<String>,
    provider_event_id: Option<String>,
    state: Option<&str>,
) -> NormalizedEvent {
    event_with_update(
        kind,
        role,
        content,
        payload,
        provider_event_id,
        state,
        NormalizedEventUpdate::Replace,
    )
}

#[allow(clippy::too_many_arguments)]
fn event_with_update(
    kind: &str,
    role: Option<&str>,
    content: impl Into<String>,
    payload: Option<String>,
    provider_event_id: Option<String>,
    state: Option<&str>,
    update: NormalizedEventUpdate,
) -> NormalizedEvent {
    NormalizedEvent {
        kind: kind.to_string(),
        role: role.map(str::to_string),
        content: content.into(),
        payload,
        provider_event_id,
        state: state.map(str::to_string),
        update,
    }
}

fn normalize_approval(value: &Value, event_type: &str) -> Option<NormalizedEvent> {
    let lower = event_type.to_ascii_lowercase();
    let claude_control_request = lower == "control_request"
        && value
            .get("request")
            .and_then(|request| request.get("subtype"))
            .and_then(Value::as_str)
            == Some("can_use_tool");
    if !lower.contains("approval") && !lower.contains("permission") && !claude_control_request {
        return None;
    }
    if lower.contains("decision") || lower.contains("response") || lower.contains("completed") {
        return None;
    }
    let request_id = string_at(
        value,
        &[
            &["request_id"],
            &["requestId"],
            &["approval_id"],
            &["approvalId"],
            &["id"],
            &["params", "requestId"],
            &["params", "approvalId"],
            &["request", "id"],
        ],
    );
    let content = string_at(
        value,
        &[
            &["message"],
            &["reason"],
            &["description"],
            &["params", "reason"],
            &["params", "command"],
            &["tool_name"],
            &["toolName"],
            &["request", "reason"],
            &["request", "tool_name"],
        ],
    )
    .unwrap_or_else(|| "The agent needs approval to continue".to_string());
    Some(event(
        "approval_request",
        None,
        content,
        compact_json(value),
        request_id,
        Some("pending"),
    ))
}

fn normalize_claude_stream_event(
    value: &Value,
    state: &mut NormalizeState,
    output: &mut NormalizedLine,
) {
    output.provider_session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let stream = value.get("event").unwrap_or(value);
    match stream.get("type").and_then(Value::as_str).unwrap_or_default() {
        "message_start" => {
            state.claude_message_id = string_at(stream, &[&["message", "id"]]);
        }
        "content_block_start" => {
            let block = stream.get("content_block").unwrap_or(stream);
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("Tool");
                output.events.push(event(
                    "tool_call",
                    None,
                    name,
                    compact_json(block),
                    block.get("id").and_then(Value::as_str).map(str::to_string),
                    Some("running"),
                ));
            }
        }
        "content_block_delta" => {
            let delta = stream.get("delta").unwrap_or(stream);
            if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
                let content = delta.get("text").and_then(Value::as_str).unwrap_or_default();
                if !content.is_empty() {
                    output.events.push(event_with_update(
                        "assistant_message",
                        Some("assistant"),
                        content,
                        None,
                        state.claude_message_id.clone(),
                        Some("running"),
                        NormalizedEventUpdate::Append,
                    ));
                }
            }
        }
        _ => {}
    }
}

fn normalize_claude(
    value: &Value,
    event_type: &str,
    state: &mut NormalizeState,
    output: &mut NormalizedLine,
) {
    if event_type == "stream_event" {
        normalize_claude_stream_event(value, state, output);
        return;
    }
    if event_type == "system" && value.get("subtype").and_then(Value::as_str) == Some("init") {
        output.provider_session_id = value
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        return;
    }
    if event_type == "assistant" {
        let message = value.get("message").unwrap_or(value);
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(blocks) = message.get("content").and_then(Value::as_array) {
            let content = blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            if !content.is_empty() {
                output.events.push(event(
                    "assistant_message",
                    Some("assistant"),
                    content,
                    None,
                    message_id.clone(),
                    Some("completed"),
                ));
            }
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        let name = block.get("name").and_then(Value::as_str).unwrap_or("Tool");
                        let id = block.get("id").and_then(Value::as_str).map(str::to_string);
                        output.events.push(event(
                            "tool_call",
                            None,
                            name,
                            compact_json(block),
                            id,
                            Some("running"),
                        ));
                }
            }
        }
        state.claude_message_id = None;
        return;
    }
    if event_type == "user" {
        if let Some(blocks) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
        {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let id = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let content = block
                    .get("content")
                    .map(text_from_content)
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "Tool finished".to_string());
                output.events.push(event(
                    "tool_result",
                    None,
                    content,
                    compact_json(block),
                    id,
                    Some(
                        if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                            "error"
                        } else {
                            "completed"
                        },
                    ),
                ));
            }
        }
        return;
    }
    if event_type == "result" && value.get("is_error").and_then(Value::as_bool) == Some(true) {
        let content = string_at(value, &[&["result"], &["error"], &["message"]])
            .unwrap_or_else(|| "Claude ended the turn with an error".to_string());
        output.events.push(event(
            "error",
            None,
            content,
            compact_json(value),
            None,
            Some("error"),
        ));
    }
    if event_type == "result" {
        output.turn_complete = true;
    }
}

fn normalize_codex(value: &Value, event_type: &str, output: &mut NormalizedLine) {
    if event_type == "thread.started" {
        output.provider_session_id = string_at(value, &[&["thread_id"], &["thread", "id"]]);
        return;
    }
    if event_type == "turn.failed" || event_type == "error" {
        output.turn_complete = true;
        let content = string_at(value, &[&["error", "message"], &["message"], &["error"]])
            .unwrap_or_else(|| "Codex ended the turn with an error".to_string());
        output.events.push(event(
            "error",
            None,
            content,
            compact_json(value),
            None,
            Some("error"),
        ));
        return;
    }
    if event_type == "turn.completed" {
        output.turn_complete = true;
        return;
    }
    if !matches!(
        event_type,
        "item.started" | "item.completed" | "item.updated"
    ) {
        return;
    }
    let item = value.get("item").unwrap_or(value);
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    let item_id = item.get("id").and_then(Value::as_str).map(str::to_string);
    let completed = event_type == "item.completed";
    match item_type {
        "agent_message" | "message" => {
            let content = string_at(item, &[&["text"], &["content"], &["message"]])
                .unwrap_or_else(|| text_from_content(item));
            if !content.is_empty() {
                output.events.push(event(
                    "assistant_message",
                    Some("assistant"),
                    content,
                    None,
                    item_id,
                    Some(if completed { "completed" } else { "running" }),
                ));
            }
        }
        "command_execution" | "file_change" | "mcp_tool_call" | "web_search" | "tool_call" => {
            let name = string_at(item, &[&["name"], &["command"], &["tool"], &["path"]])
                .unwrap_or_else(|| item_type.replace('_', " "));
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or(if completed { "completed" } else { "running" });
            output.events.push(event(
                if completed {
                    "tool_result"
                } else {
                    "tool_call"
                },
                None,
                name,
                compact_json(item),
                item_id,
                Some(status),
            ));
        }
        "reasoning" if completed => {
            let content = string_at(item, &[&["text"], &["summary"]]).unwrap_or_default();
            if !content.is_empty() {
                output.events.push(event(
                    "tool_result",
                    None,
                    "Reasoning",
                    compact_json(item),
                    item_id,
                    Some("completed"),
                ));
            }
        }
        _ => {}
    }
}

fn finish_gemini_message(state: &mut NormalizeState, output: &mut NormalizedLine) {
    let Some(message_id) = state.gemini_message_id.take() else {
        return;
    };
    output.events.push(event_with_update(
        "assistant_message",
        Some("assistant"),
        "",
        None,
        Some(message_id),
        Some("completed"),
        NormalizedEventUpdate::Append,
    ));
}

fn normalize_gemini(
    value: &Value,
    event_type: &str,
    state: &mut NormalizeState,
    output: &mut NormalizedLine,
) {
    match event_type {
        "init" => {
            output.provider_session_id = value
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        "message" if value.get("role").and_then(Value::as_str) == Some("assistant") => {
            let content = value
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !content.is_empty() {
                let explicit_id = string_at(value, &[&["id"], &["message_id"], &["messageId"]]);
                let message_id = explicit_id.unwrap_or_else(|| {
                    state
                        .gemini_message_id
                        .clone()
                        .unwrap_or_else(|| {
                            state.gemini_message_sequence += 1;
                            format!("gemini-message-{}", state.gemini_message_sequence)
                        })
                });
                state.gemini_message_id = Some(message_id.clone());
                let is_delta = value.get("delta").and_then(Value::as_bool) == Some(true);
                output.events.push(event_with_update(
                    "assistant_message",
                    Some("assistant"),
                    content,
                    None,
                    Some(message_id),
                    Some(if is_delta { "running" } else { "completed" }),
                    if is_delta {
                        NormalizedEventUpdate::Append
                    } else {
                        NormalizedEventUpdate::Replace
                    },
                ));
            }
        }
        "tool_use" => {
            finish_gemini_message(state, output);
            let name = value
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("Tool");
            let id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            output.events.push(event(
                "tool_call",
                None,
                name,
                compact_json(value),
                id,
                Some("running"),
            ));
        }
        "tool_result" => {
            let content = value
                .get("output")
                .and_then(Value::as_str)
                .or_else(|| {
                    value
                        .get("error")
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("Tool finished");
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            let id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            output.events.push(event(
                "tool_result",
                None,
                content,
                compact_json(value),
                id,
                Some(status),
            ));
        }
        "error" => {
            let content = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Gemini reported an error");
            output.events.push(event(
                "error",
                None,
                content,
                compact_json(value),
                None,
                Some("error"),
            ));
        }
        "result" => {
            finish_gemini_message(state, output);
            output.turn_complete = true;
        }
        _ => {}
    }
}

fn normalize_cursor(value: &Value, event_type: &str, output: &mut NormalizedLine) {
    output.provider_session_id = output.provider_session_id.take().or_else(|| {
        string_at(
            value,
            &[&["session_id"], &["sessionId"], &["chat_id"], &["chatId"]],
        )
    });
    if matches!(event_type, "assistant" | "assistant_message" | "message")
        && value.get("role").and_then(Value::as_str) != Some("user")
    {
        let content = string_at(
            value,
            &[
                &["content"],
                &["text"],
                &["message", "content"],
                &["message", "text"],
            ],
        )
        .unwrap_or_default();
        if !content.is_empty() {
            output.events.push(event(
                "assistant_message",
                Some("assistant"),
                content,
                None,
                string_at(value, &[&["id"], &["message", "id"]]),
                Some("completed"),
            ));
        }
        return;
    }
    if matches!(event_type, "tool_call" | "tool_use" | "tool-call") {
        let name = string_at(value, &[&["tool_name"], &["name"], &["tool", "name"]])
            .unwrap_or_else(|| "Tool".to_string());
        output.events.push(event(
            "tool_call",
            None,
            name,
            compact_json(value),
            string_at(value, &[&["tool_id"], &["id"]]),
            Some("running"),
        ));
    } else if matches!(event_type, "tool_result" | "tool-result") {
        let content = string_at(value, &[&["output"], &["content"], &["error", "message"]])
            .unwrap_or_else(|| "Tool finished".to_string());
        output.events.push(event(
            "tool_result",
            None,
            content,
            compact_json(value),
            string_at(value, &[&["tool_id"], &["id"]]),
            value
                .get("status")
                .and_then(Value::as_str)
                .or(Some("completed")),
        ));
    } else if event_type == "error" {
        let content = string_at(value, &[&["message"], &["error", "message"]])
            .unwrap_or_else(|| "Cursor reported an error".to_string());
        output.events.push(event(
            "error",
            None,
            content,
            compact_json(value),
            None,
            Some("error"),
        ));
    } else if matches!(event_type, "result" | "completed" | "turn_completed") {
        output.turn_complete = true;
    }
}

pub fn normalize_provider_line(
    agent: &str,
    line: &str,
) -> Result<NormalizedLine, serde_json::Error> {
    normalize_provider_line_with_state(agent, line, &mut NormalizeState::default())
}

pub fn normalize_provider_line_with_state(
    agent: &str,
    line: &str,
    state: &mut NormalizeState,
) -> Result<NormalizedLine, serde_json::Error> {
    let value: Value = serde_json::from_str(line)?;
    let event_type = string_at(&value, &[&["type"], &["event"], &["method"]]).unwrap_or_default();
    let mut output = NormalizedLine::default();

    if let Some(approval) = normalize_approval(&value, &event_type) {
        output.events.push(approval);
        return Ok(output);
    }

    match agent {
        "claude" => normalize_claude(&value, &event_type, state, &mut output),
        "codex" => normalize_codex(&value, &event_type, &mut output),
        "gemini" => normalize_gemini(&value, &event_type, state, &mut output),
        "cursor" => normalize_cursor(&value, &event_type, &mut output),
        _ => {}
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_partial_text_deltas_share_the_completed_messages_stable_id() {
        let mut state = NormalizeState::default();
        let started = normalize_provider_line_with_state(
            "claude",
            r#"{"type":"stream_event","uuid":"event-1","session_id":"session-1","event":{"type":"message_start","message":{"id":"message-1"}}}"#,
            &mut state,
        )
        .unwrap();
        assert!(started.events.is_empty());

        let first = normalize_provider_line_with_state(
            "claude",
            r#"{"type":"stream_event","uuid":"event-2","session_id":"session-1","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Updated"}}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].content, "Updated");
        assert_eq!(first.events[0].provider_event_id.as_deref(), Some("message-1"));
        assert_eq!(first.events[0].state.as_deref(), Some("running"));
        assert_eq!(first.events[0].update, NormalizedEventUpdate::Append);

        let second = normalize_provider_line_with_state(
            "claude",
            r#"{"type":"stream_event","uuid":"event-3","session_id":"session-1","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" the README."}}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(second.events[0].provider_event_id.as_deref(), Some("message-1"));
        assert_eq!(second.events[0].update, NormalizedEventUpdate::Append);

        let completed = normalize_provider_line_with_state(
            "claude",
            r#"{"type":"assistant","session_id":"session-1","message":{"id":"message-1","content":[{"type":"text","text":"Updated the README."}]}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(completed.events[0].provider_event_id.as_deref(), Some("message-1"));
        assert_eq!(completed.events[0].state.as_deref(), Some("completed"));
        assert_eq!(completed.events[0].update, NormalizedEventUpdate::Replace);
    }

    #[test]
    fn codex_updated_message_is_visible_before_completion() {
        let mut state = NormalizeState::default();
        let updated = normalize_provider_line_with_state(
            "codex",
            r#"{"type":"item.updated","item":{"id":"item-1","type":"agent_message","text":"Working on it"}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(updated.events.len(), 1);
        assert_eq!(updated.events[0].kind, "assistant_message");
        assert_eq!(updated.events[0].content, "Working on it");
        assert_eq!(updated.events[0].state.as_deref(), Some("running"));
        assert_eq!(updated.events[0].update, NormalizedEventUpdate::Replace);

        let completed = normalize_provider_line_with_state(
            "codex",
            r#"{"type":"item.completed","item":{"id":"item-1","type":"agent_message","text":"Working on it — done."}}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(completed.events[0].provider_event_id.as_deref(), Some("item-1"));
        assert_eq!(completed.events[0].state.as_deref(), Some("completed"));
    }

    #[test]
    fn gemini_message_chunks_append_until_a_tool_starts_a_new_message() {
        let mut state = NormalizeState::default();
        let first = normalize_provider_line_with_state(
            "gemini",
            r#"{"type":"message","role":"assistant","content":"Checking","delta":true}"#,
            &mut state,
        )
        .unwrap();
        let first_id = first.events[0].provider_event_id.clone().unwrap();
        assert_eq!(first.events[0].update, NormalizedEventUpdate::Append);

        let second = normalize_provider_line_with_state(
            "gemini",
            r#"{"type":"message","role":"assistant","content":" files","delta":true}"#,
            &mut state,
        )
        .unwrap();
        assert_eq!(second.events[0].provider_event_id.as_deref(), Some(first_id.as_str()));

        normalize_provider_line_with_state(
            "gemini",
            r#"{"type":"tool_use","tool_name":"Read","tool_id":"tool-1","parameters":{"file_path":"README.md"}}"#,
            &mut state,
        )
        .unwrap();
        let after_tool = normalize_provider_line_with_state(
            "gemini",
            r#"{"type":"message","role":"assistant","content":"Done","delta":true}"#,
            &mut state,
        )
        .unwrap();
        assert_ne!(
            after_tool.events[0].provider_event_id.as_deref(),
            Some(first_id.as_str())
        );
    }
}
