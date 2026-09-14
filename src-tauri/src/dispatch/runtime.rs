use super::{
    normalize_provider_line_with_state, AgentCommand, NormalizeState, NormalizedEvent,
    NormalizedEventUpdate,
};
use crate::db::{queries, Db};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

struct ActiveTurn {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    interrupted: AtomicBool,
}

/// Owns only currently-executing provider turns. Conversation identity, transcript, and
/// resumability are durable in SQLite, so a provider process is free to exit between prompts.
#[derive(Default)]
pub struct Runtime {
    active: Mutex<HashMap<String, Arc<ActiveTurn>>>,
}

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub fn start_turn(
        &self,
        app: AppHandle,
        run_id: String,
        turn_id: String,
        agent: String,
        project_path: &str,
        command: AgentCommand,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.active.lock().unwrap().contains_key(&run_id),
            "conversation {run_id} already has an active turn"
        );

        let AgentCommand {
            executable,
            args,
            initial_stdin,
        } = command;
        let mut builder = Command::new(&executable);
        builder
            .args(args)
            .current_dir(project_path)
            // Not the app's own launchd PATH — see `agent_search_paths` for why.
            .env("PATH", super::agent_path_env())
            .env("TERM", "dumb")
            .env("NO_COLOR", "1")
            .env("CLICOLOR", "0")
            .env("RELAY_DISPATCH_RUN_ID", &run_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = builder.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("{} did not expose structured stdout", agent))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("{} did not expose stderr", agent))?;
        let active = Arc::new(ActiveTurn {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            interrupted: AtomicBool::new(false),
        });
        if let Some(initial_stdin) = initial_stdin {
            let mut stdin = active.stdin.lock().unwrap();
            let writer = stdin
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("{agent} did not accept its initial prompt"))?;
            if let Err(error) = writer
                .write_all(initial_stdin.as_bytes())
                .and_then(|_| writer.flush())
            {
                let _ = active.child.lock().unwrap().kill();
                return Err(error.into());
            }
        }
        self.active
            .lock()
            .unwrap()
            .insert(run_id.clone(), active.clone());

        {
            let db = app.state::<Db>();
            let conn = db.0.lock().unwrap();
            queries::sync_card_for_dispatch_run(&conn, &run_id, "in_progress")?;
        }
        emit_run_changed(&app, &run_id, "running");

        let output_app = app.clone();
        let output_run_id = run_id.clone();
        let output_turn_id = turn_id.clone();
        let output_agent = agent.clone();
        let output_active = active.clone();
        let output_thread = std::thread::spawn(move || {
            let mut normalize_state = NormalizeState::default();
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        persist_diagnostic(
                            &output_app,
                            &output_run_id,
                            &output_turn_id,
                            "error",
                            &format!("Could not read {output_agent} output: {error}"),
                        );
                        break;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                match normalize_provider_line_with_state(
                    &output_agent,
                    &line,
                    &mut normalize_state,
                ) {
                    Ok(normalized) => {
                        let turn_complete = normalized.turn_complete;
                        if let Err(error) = persist_normalized_line(
                            &output_app,
                            &output_run_id,
                            &output_turn_id,
                            normalized.provider_session_id.as_deref(),
                            normalized.events,
                        ) {
                            log::warn!(
                                "failed to persist structured {output_agent} output for run {output_run_id}: {error:#}"
                            );
                        }
                        if turn_complete {
                            *output_active.stdin.lock().unwrap() = None;
                        }
                    }
                    Err(error) => {
                        log::warn!(
                            "ignored malformed structured {output_agent} output for run {output_run_id}: {error}"
                        );
                        persist_diagnostic(
                            &output_app,
                            &output_run_id,
                            &output_turn_id,
                            "warning",
                            &line,
                        );
                    }
                }
            }
        });

        let error_app = app.clone();
        let error_run_id = run_id.clone();
        let error_turn_id = turn_id.clone();
        let error_agent = agent.clone();
        let error_thread = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                log::debug!("{error_agent} stderr for run {error_run_id}: {line}");
                persist_diagnostic(&error_app, &error_run_id, &error_turn_id, "warning", &line);
            }
        });

        std::thread::spawn(move || {
            let exit = loop {
                let result = active.child.lock().unwrap().try_wait();
                match result {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => std::thread::sleep(Duration::from_millis(80)),
                    Err(error) => break Err(error),
                }
            };
            let _ = output_thread.join();
            let _ = error_thread.join();

            let interrupted = active.interrupted.load(Ordering::SeqCst);
            let (turn_status, conversation_status, exit_code, error) = match exit {
                Ok(status) if interrupted => (
                    "interrupted",
                    "idle",
                    Some(status.code().unwrap_or(130) as i64),
                    None,
                ),
                Ok(status) if status.success() => (
                    "completed",
                    "idle",
                    Some(status.code().unwrap_or(0) as i64),
                    None,
                ),
                Ok(status) => {
                    let message = format!("{agent} exited with {status}");
                    (
                        "failed",
                        "failed",
                        status.code().map(i64::from),
                        Some(message),
                    )
                }
                Err(error) => (
                    "failed",
                    "failed",
                    None,
                    Some(format!("Could not wait for {agent}: {error}")),
                ),
            };

            let final_status = {
                let db = app.state::<Db>();
                let conn = db.0.lock().unwrap();
                if let Err(db_error) = queries::settle_dispatch_turn(
                    &conn,
                    &run_id,
                    &turn_id,
                    turn_status,
                    conversation_status,
                    exit_code,
                    error.as_deref(),
                    chrono::Utc::now().timestamp(),
                ) {
                    log::warn!("failed to settle dispatched turn {turn_id}: {db_error:#}");
                }
                if let Err(db_error) = queries::sync_card_for_dispatch_run(&conn, &run_id, "review")
                {
                    log::warn!("failed to sync card for dispatched run {run_id}: {db_error:#}");
                }
                queries::get_dispatch_run(&conn, &run_id)
                    .ok()
                    .flatten()
                    .map(|run| run.status)
                    .unwrap_or_else(|| conversation_status.to_string())
            };
            app.state::<Runtime>()
                .active
                .lock()
                .unwrap()
                .remove(&run_id);
            emit_run_changed(&app, &run_id, &final_status);
            // Only after the handle is removed: a continuation needs this run to have no
            // active turn, or `start_turn` would refuse it.
            crate::commands::continue_dispatch_loop(&app, &run_id, &turn_id, turn_status);
        });

        Ok(())
    }

    pub fn interrupt_turn(&self, app: &AppHandle, run_id: &str) -> anyhow::Result<()> {
        let active = self
            .active
            .lock()
            .unwrap()
            .get(run_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("conversation {run_id} has no active turn"))?;
        active.interrupted.store(true, Ordering::SeqCst);
        {
            let db = app.state::<Db>();
            let conn = db.0.lock().unwrap();
            queries::set_dispatch_run_status(
                &conn,
                run_id,
                "interrupting",
                None,
                chrono::Utc::now().timestamp(),
            )?;
        }
        active.child.lock().unwrap().kill()?;
        emit_run_changed(app, run_id, "interrupting");
        Ok(())
    }

    pub fn resolve_approval(
        &self,
        run_id: &str,
        agent: &str,
        request_id: Option<&str>,
        decision: &str,
    ) -> anyhow::Result<()> {
        let active = self
            .active
            .lock()
            .unwrap()
            .get(run_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("conversation {run_id} has no active turn"))?;
        let payload = approval_payload(agent, request_id, decision)?;
        let mut stdin = active.stdin.lock().unwrap();
        let writer = stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("{agent} is not accepting approval input"))?;
        writer.write_all(payload.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    pub fn stop_for_shutdown(&self, run_id: &str) -> anyhow::Result<()> {
        let active = self.active.lock().unwrap().get(run_id).cloned();
        if let Some(active) = active {
            active.interrupted.store(true, Ordering::SeqCst);
            active.child.lock().unwrap().kill()?;
        }
        Ok(())
    }
}

fn approval_payload(
    agent: &str,
    request_id: Option<&str>,
    decision: &str,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        matches!(decision, "allowed_once" | "allowed_for_session" | "denied"),
        "unsupported approval decision: {decision}"
    );
    if agent == "claude" {
        let request_id = request_id
            .ok_or_else(|| anyhow::anyhow!("Claude approval is missing its request id"))?;
        let behavior = if decision == "denied" {
            "deny"
        } else {
            "allow"
        };
        return Ok(format!(
            "{}\n",
            serde_json::json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": { "behavior": behavior }
                }
            })
        ));
    }
    Ok(match decision {
        "allowed_once" => "y\n",
        "allowed_for_session" => "a\n",
        "denied" => "n\n",
        _ => unreachable!(),
    }
    .to_string())
}

fn persist_normalized_line(
    app: &AppHandle,
    run_id: &str,
    turn_id: &str,
    provider_session_id: Option<&str>,
    events: Vec<NormalizedEvent>,
) -> anyhow::Result<()> {
    let db = app.state::<Db>();
    let conn = db.0.lock().unwrap();
    if let Some(provider_session_id) = provider_session_id {
        queries::set_dispatch_provider_session(&conn, run_id, provider_session_id)?;
    }
    for normalized in events {
        let update = match normalized.update {
            NormalizedEventUpdate::Append => queries::DispatchEventUpdate::Append,
            NormalizedEventUpdate::Replace => queries::DispatchEventUpdate::Replace,
        };
        let event = queries::upsert_dispatch_event(
            &conn,
            run_id,
            Some(turn_id),
            &normalized.kind,
            normalized.role.as_deref(),
            &normalized.content,
            normalized.payload.as_deref(),
            normalized.provider_event_id.as_deref(),
            normalized.state.as_deref(),
            update,
            chrono::Utc::now().timestamp(),
        )?;
        if normalized.kind == "approval_request" {
            queries::set_dispatch_run_status(
                &conn,
                run_id,
                "awaiting_approval",
                None,
                chrono::Utc::now().timestamp(),
            )?;
        }
        emit_dispatch_event(app, &event);
    }
    Ok(())
}

fn persist_diagnostic(app: &AppHandle, run_id: &str, turn_id: &str, state: &str, content: &str) {
    let result = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        queries::append_dispatch_event(
            &conn,
            run_id,
            Some(turn_id),
            if state == "error" { "error" } else { "status" },
            None,
            content,
            None,
            None,
            Some(state),
            chrono::Utc::now().timestamp(),
        )
    };
    if let Ok(event) = result {
        emit_dispatch_event(app, &event);
    }
}

pub fn emit_dispatch_event(app: &AppHandle, event: &queries::DispatchEvent) {
    let _ = app.emit("dispatch-event", event);
}

fn emit_run_changed(app: &AppHandle, run_id: &str, status: &str) {
    let _ = app.emit(
        "data-changed",
        serde_json::json!({ "entity": "dispatch_run", "kind": status, "run_id": run_id }),
    );
}
