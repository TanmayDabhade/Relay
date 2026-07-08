use crate::activity;
use crate::db::{queries, Db};
use serde::Serialize;
use std::process::Command;
use tauri::State;

#[tauri::command]
pub fn list_projects(db: State<'_, Db>) -> Result<Vec<queries::ProjectSummary>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_projects(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(db: State<'_, Db>) -> Result<Vec<queries::Session>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_sessions(&conn).map_err(|e| e.to_string())
}

/// Return shape for `get_session_detail` — wraps the session row together with its file
/// changes, since a frontend detail view needs both.
#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub session: queries::Session,
    pub files_changed: Vec<queries::FileChanged>,
}

#[tauri::command]
pub fn get_session_detail(
    db: State<'_, Db>,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_session_detail(&conn, &session_id)
        .map(|opt| opt.map(|(session, files_changed)| SessionDetail { session, files_changed }))
        .map_err(|e| e.to_string())
}

/// Opens `path` in the user's editor: `$EDITOR <path>` if that env var is set, otherwise falls
/// back to VS Code's `code <path>` CLI. Spawns and returns immediately (doesn't wait for the
/// editor to exit) — this is triggered by a button click in the session detail modal and
/// shouldn't block the UI. A spawn failure (e.g. neither `$EDITOR` nor `code` is on `PATH`) is
/// an expected, recoverable case surfaced to the caller as an `Err`, not a panic.
#[tauri::command]
pub fn open_in_editor(path: String) -> Result<(), String> {
    if let Ok(editor) = std::env::var("EDITOR") {
        if !editor.trim().is_empty() {
            return Command::new(&editor)
                .arg(&path)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("failed to launch $EDITOR ({editor}): {e}"));
        }
    }

    Command::new("code")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to launch editor: $EDITOR is not set and `code` failed to start: {e}"))
}

/// Returns a 14-day daily git-commit-count sparkline for the project at `project_path`, for
/// the decorative `ActivityBars` component on each project card. Returns `Vec<i64>` directly,
/// not `Result` — see `activity`'s module doc comment for why: every failure mode (not a git
/// repo, no `git` on `PATH`, shellout failure) already degrades to `vec![0; 14]` inside
/// `activity::project_activity`, so there's no error state left for the frontend to handle.
#[tauri::command]
pub fn project_activity(project_path: String, cache: State<'_, activity::ActivityCache>) -> Vec<i64> {
    activity::project_activity(&project_path, &cache)
}
