//! Turning a finished dispatched task into a pull request: each PR-bound task runs in its own
//! git worktree (`git::prepare_workspace`), and when it finishes Relay commits what the agent
//! left, pushes the branch, and opens a PR with `gh`.
//!
//! Like the idle sweep in `lib.rs`, a ship is split into **gather** (DB lock held, cheap reads
//! plus the claim), **compute** (lock released: git/gh process I/O that can take seconds or
//! hang on the network), and **write** (lock re-acquired briefly). The shared
//! `Mutex<Connection>` is never held across a git or gh call — a slow push would otherwise
//! stall every UI command and the watcher for as long as it took.

pub mod git;

use crate::db::{queries, Db};
use crate::dispatch;
use chrono::Utc;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

/// Where task worktrees live: inside Relay's own data dir, never inside the user's repo, so
/// they can't be picked up by the repo's tooling or accidentally committed.
pub fn worktrees_root(app: &AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join("worktrees"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipTrigger {
    /// The task finished on its own; only ships when the task opted into auto-shipping.
    Auto,
    /// The user pressed "Ship" — ships regardless of the auto toggle.
    Manual,
}

/// Starts a ship on a background thread. Returns `Ok(false)` without doing anything when the
/// task has no workspace, auto-shipping is off for an `Auto` trigger, or another ship already
/// holds the claim; errors only when the claim itself can't be evaluated.
pub fn spawn_ship(app: &AppHandle, task_id: &str, trigger: ShipTrigger) -> anyhow::Result<bool> {
    // Gather + claim, under the lock.
    let gathered = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let Some(workspace) = queries::get_task_workspace(&conn, task_id)? else {
            return Ok(false);
        };
        if trigger == ShipTrigger::Auto && !workspace.auto_ship {
            return Ok(false);
        }
        let Some(task) = queries::get_dispatch_task(&conn, task_id)? else {
            return Ok(false);
        };
        let Some(run) = queries::latest_run_for_task(&conn, task_id)? else {
            return Ok(false);
        };
        let summary = queries::last_assistant_message_for_run(&conn, &run.id)?;
        if !queries::claim_task_ship(&conn, task_id)? {
            return Ok(false);
        }
        (workspace, task, run, summary)
    };
    emit_changed(app);

    let app = app.clone();
    std::thread::spawn(move || {
        let (workspace, task, run, summary) = gathered;
        // Compute: git/gh I/O with no lock held.
        let result = git::ship(
            &workspace,
            &git::ShipContext {
                title: &task.title,
                prompt: &task.prompt,
                agent: &run.agent,
                model: &run.model,
                summary: summary.as_deref(),
            },
        );
        let (status, error, pr_url, notice_state, notice) = match result {
            Ok(outcome) => match outcome.status {
                git::ShipStatus::Shipped => {
                    let url = outcome.pr_url.clone();
                    let notice = match &url {
                        Some(url) if workspace.pr_url.is_some() => {
                            format!(
                                "Pushed new commits to `{}` — the PR is updated: {url}",
                                workspace.branch
                            )
                        }
                        Some(url) => format!("Opened pull request: {url}"),
                        None => format!(
                            "Pushed `{}`, but gh did not report a PR URL — check GitHub.",
                            workspace.branch
                        ),
                    };
                    ("shipped", None, url, "completed", notice)
                }
                git::ShipStatus::NoChanges => (
                    "no_changes",
                    None,
                    None,
                    "warning",
                    "No PR opened: the agent left no changes on its branch.".to_string(),
                ),
            },
            Err(error) => {
                let message = format!("{error:#}");
                log::warn!("shipping task {} failed: {message}", task.id);
                (
                    "failed",
                    Some(message.clone()),
                    None,
                    "error",
                    format!("Could not open the PR: {message}"),
                )
            }
        };

        // Write: brief lock for the result and a notice in the task's conversation.
        let event = {
            let db = app.state::<Db>();
            let Ok(conn) = db.0.lock() else { return };
            let now = Utc::now().timestamp();
            if let Err(db_error) = queries::finish_task_ship(
                &conn,
                &task.id,
                status,
                error.as_deref(),
                pr_url.as_deref(),
                now,
            ) {
                log::warn!(
                    "could not record ship result for task {}: {db_error:#}",
                    task.id
                );
            }
            queries::append_dispatch_event(
                &conn,
                &run.id,
                None,
                if notice_state == "error" {
                    "error"
                } else {
                    "status"
                },
                None,
                &notice,
                None,
                None,
                Some(notice_state),
                now,
            )
            .ok()
        };
        if let Some(event) = event {
            dispatch::emit_dispatch_event(&app, &event);
        }
        emit_changed(&app);
    });
    Ok(true)
}

/// Removes a task's worktree on a background thread after its conversation is shut down.
/// A worktree with uncommitted changes is kept (nothing is ever discarded); the branch is
/// always kept because it backs the PR.
pub fn spawn_cleanup(app: &AppHandle, task_id: &str) {
    let workspace = {
        let db = app.state::<Db>();
        let Ok(conn) = db.0.lock() else { return };
        match queries::get_task_workspace(&conn, task_id) {
            Ok(Some(workspace)) if workspace.removed_at.is_none() => workspace,
            _ => return,
        }
    };
    let app = app.clone();
    std::thread::spawn(move || match git::remove_worktree(&workspace) {
        Ok(true) => {
            let db = app.state::<Db>();
            if let Ok(conn) = db.0.lock() {
                let _ = queries::set_task_workspace_removed(
                    &conn,
                    &workspace.task_id,
                    Some(Utc::now().timestamp()),
                );
            }
            emit_changed(&app);
        }
        Ok(false) => log::info!(
            "kept worktree {} for task {}: it has uncommitted changes",
            workspace.worktree_path,
            workspace.task_id
        ),
        Err(error) => log::warn!(
            "could not remove worktree {}: {error:#}",
            workspace.worktree_path
        ),
    });
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit(
        "data-changed",
        serde_json::json!({ "entity": "task_workspace", "kind": "updated" }),
    );
}
