//! Planner and task-draft assistants. Both run the user's local `claude` CLI once, headless
//! and read-only, inside the project folder so the model can look at the real code before it
//! answers — no Anthropic API key needed, and it bills to whatever the CLI is logged in as.
//!
//! The flags are chosen so an assistant call can never change the user's repo or pollute
//! Relay's own data:
//! - `--tools Read,Glob,Grep`: no Bash, Edit, Write, or web tools exist in the session at all.
//! - `--no-session-persistence`: no `~/.claude/projects/**.jsonl` log is written, so the
//!   watcher never ingests the call as a "session" and no stray board card appears.
//! - `--safe-mode --strict-mcp-config`: skips the user's plugins, hooks, and MCP servers. With
//!   a heavily customized install the same tiny prompt measured ~$0.30 vs ~$0.001 without
//!   them. Safe mode also skips CLAUDE.md, so the prompts ask the model to read it itself.
//! - `--json-schema` + `--output-format json`: the answer arrives as `structured_output`.

pub mod prompts;

use crate::dispatch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Reading a sizable repo before planning takes a while; past this, something is stuck.
const ASSIST_TIMEOUT: Duration = Duration::from_secs(300);
/// Sonnet: the planner has to read code and reason about it; Haiku plans noticeably worse.
pub const ASSIST_MODEL: &str = "sonnet";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDraft {
    pub title: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedTask {
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub summary: String,
    pub tasks: Vec<PlannedTask>,
}

/// Runs one read-only structured `claude` call in `cwd` and returns its `structured_output`.
/// Blocking — callers run it off the async runtime (`spawn_blocking`) and never under the DB
/// lock.
pub fn run_structured(
    claude_executable: &str,
    cwd: &Path,
    prompt: &str,
    schema: &Value,
) -> anyhow::Result<Value> {
    let executable = dispatch::resolve_executable(claude_executable).ok_or_else(|| {
        anyhow::anyhow!(
            "the claude CLI ('{claude_executable}') was not found; set it up in Connections"
        )
    })?;
    anyhow::ensure!(
        cwd.is_dir(),
        "project folder {} does not exist",
        cwd.display()
    );
    let schema = serde_json::to_string(schema)?;
    let mut child = Command::new(executable)
        .args([
            "--print",
            "--output-format",
            "json",
            "--no-session-persistence",
            "--safe-mode",
            "--strict-mcp-config",
            "--model",
            ASSIST_MODEL,
            "--json-schema",
            &schema,
            // Last, and the prompt goes over stdin: `--tools` takes a variadic list that
            // would swallow a positional prompt placed after it.
            "--tools",
            "Read,Glob,Grep",
        ])
        .current_dir(cwd)
        .env("PATH", dispatch::agent_path_env())
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("claude did not accept a prompt"))?;
        stdin.write_all(prompt.as_bytes())?;
        // Dropping stdin closes it, which is what tells `--print` the prompt is complete.
    }
    // Drain stdout/stderr on their own threads so a large reply can't fill the pipe buffer
    // and deadlock against the wait loop below.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });
    let deadline = Instant::now() + ASSIST_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "the assistant took longer than {}s and was stopped",
                ASSIST_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    parse_cli_result(&stdout).map_err(|error| {
        if status.success() {
            error
        } else {
            let detail = stderr.trim();
            anyhow::anyhow!(
                "claude exited with {status}{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )
        }
    })
}

/// Extracts the structured answer from `claude --output-format json`. Prefers the validated
/// `structured_output` field; falls back to parsing `result` as JSON for CLI versions that
/// only put it there.
pub fn parse_cli_result(stdout: &str) -> anyhow::Result<Value> {
    let envelope: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| anyhow::anyhow!("claude returned output Relay could not read"))?;
    if envelope.get("is_error").and_then(Value::as_bool) == Some(true) {
        let message = envelope
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        anyhow::bail!("claude reported an error: {message}");
    }
    if let Some(structured) = envelope.get("structured_output").filter(|v| v.is_object()) {
        return Ok(structured.clone());
    }
    let result = envelope
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("claude returned no answer"))?;
    let (start, end) = (result.find('{'), result.rfind('}'));
    match (start, end) {
        (Some(start), Some(end)) if end > start => Ok(serde_json::from_str(&result[start..=end])?),
        _ => anyhow::bail!("claude's answer was not structured"),
    }
}

pub fn draft_task(
    claude_executable: &str,
    project_path: &Path,
    rough: &str,
    current_title: &str,
) -> anyhow::Result<TaskDraft> {
    let value = run_structured(
        claude_executable,
        project_path,
        &prompts::draft_prompt(rough, current_title),
        &prompts::draft_schema(),
    )?;
    let draft: TaskDraft = serde_json::from_value(value)?;
    anyhow::ensure!(
        !draft.title.trim().is_empty() && !draft.prompt.trim().is_empty(),
        "the assistant returned an empty draft"
    );
    Ok(TaskDraft {
        title: draft.title.trim().to_string(),
        prompt: draft.prompt.trim().to_string(),
    })
}

pub fn plan_tasks(
    claude_executable: &str,
    project_path: &Path,
    goal: &str,
    existing_titles: &[String],
) -> anyhow::Result<Plan> {
    let value = run_structured(
        claude_executable,
        project_path,
        &prompts::plan_prompt(goal, existing_titles),
        &prompts::plan_schema(),
    )?;
    let mut plan: Plan = serde_json::from_value(value)?;
    plan.tasks
        .retain(|task| !task.title.trim().is_empty() && !task.prompt.trim().is_empty());
    anyhow::ensure!(!plan.tasks.is_empty(), "the planner returned no tasks");
    plan.tasks.truncate(prompts::MAX_PLANNED_TASKS);
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_output_is_preferred() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"title\":\"x\"}","structured_output":{"title":"Fix Login Bug"}}"#;
        assert_eq!(
            parse_cli_result(stdout).unwrap(),
            serde_json::json!({"title": "Fix Login Bug"})
        );
    }

    #[test]
    fn result_text_is_a_fallback_even_with_surrounding_prose() {
        let stdout = r#"{"is_error":false,"result":"Here you go:\n{\"title\":\"A\",\"prompt\":\"B\"}\nDone."}"#;
        assert_eq!(
            parse_cli_result(stdout).unwrap(),
            serde_json::json!({"title": "A", "prompt": "B"})
        );
    }

    #[test]
    fn cli_errors_and_garbage_are_reported_not_panicked_on() {
        let error = parse_cli_result(r#"{"is_error":true,"result":"Not logged in"}"#).unwrap_err();
        assert!(error.to_string().contains("Not logged in"));
        assert!(parse_cli_result("not json").is_err());
        assert!(parse_cli_result(r#"{"is_error":false,"result":"no json here"}"#).is_err());
        assert!(parse_cli_result(r#"{"is_error":false}"#).is_err());
    }

    #[test]
    fn plans_deserialize_with_optional_rationale() {
        let plan: Plan = serde_json::from_value(serde_json::json!({
            "summary": "s",
            "tasks": [{"title": "t", "prompt": "p"}]
        }))
        .unwrap();
        assert_eq!(plan.tasks[0].rationale, "");
    }
}
