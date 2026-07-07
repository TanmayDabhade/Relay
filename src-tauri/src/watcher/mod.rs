mod tail;

use crate::db::Db;
use crate::parser;
use notify_debouncer_mini::new_debouncer;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const DEBOUNCE_MS: u64 = 500;

pub fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// Starts the FS watcher on a dedicated thread: backfills every pre-existing session file
/// once at startup (resuming from `ingest_state`, never double-processing), then watches for
/// live appends and newly created project subdirectories.
pub fn start(app_handle: AppHandle) {
    let Some(watch_dir) = claude_projects_dir() else {
        log::warn!("could not resolve home directory; Claude Code log watching disabled");
        return;
    };

    backfill(&app_handle, &watch_dir);

    std::thread::spawn(move || {
        if let Err(e) = run_watcher(app_handle, watch_dir) {
            log::error!("Claude Code log watcher exited with error: {e:#}");
        }
    });
}

fn backfill(app_handle: &AppHandle, watch_dir: &Path) {
    let Ok(project_dirs) = std::fs::read_dir(watch_dir) else {
        log::info!(
            "{} does not exist yet; will start watching once Claude Code creates it",
            watch_dir.display()
        );
        return;
    };

    for project_dir in project_dirs.flatten() {
        let project_path = project_dir.path();
        if !project_path.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                process_file(app_handle, &path);
            }
        }
    }
}

fn run_watcher(app_handle: AppHandle, watch_dir: PathBuf) -> notify::Result<()> {
    // Watch the parent `~/.claude` dir if `projects/` doesn't exist yet — notify can't watch
    // a nonexistent path, but a recursive watch on the parent still picks up `projects/`
    // (and everything under it) the moment Claude Code creates it.
    let watch_target = if watch_dir.exists() {
        watch_dir.clone()
    } else {
        watch_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| watch_dir.clone())
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), tx)?;
    debouncer
        .watcher()
        .watch(&watch_target, notify::RecursiveMode::Recursive)?;

    log::info!("watching {} for Claude Code session logs", watch_target.display());

    for result in rx {
        match result {
            Ok(events) => {
                for event in events {
                    if event.path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        process_file(&app_handle, &event.path);
                    }
                }
            }
            Err(e) => log::warn!("watcher error: {e:?}"),
        }
    }

    Ok(())
}

fn process_file(app_handle: &AppHandle, path: &Path) {
    let db = app_handle.state::<Db>();
    let conn = db.0.lock().unwrap();

    let lines = match tail::read_new_lines(&conn, path) {
        Ok(lines) => lines,
        Err(e) => {
            log::warn!("failed to tail {}: {e:#}", path.display());
            return;
        }
    };

    if lines.is_empty() {
        return;
    }

    let raw_log_path = path.to_string_lossy().to_string();
    let mut anything_changed = false;
    let mut session_created = false;

    for line in lines {
        let Some(record) = parser::parse_line(&line) else {
            continue;
        };
        match parser::ingest_record(&conn, &raw_log_path, record) {
            Ok(outcome) => {
                if outcome.project_touched.is_some()
                    || outcome.session_created.is_some()
                    || outcome.session_updated.is_some()
                {
                    anything_changed = true;
                }
                if outcome.session_created.is_some() {
                    session_created = true;
                }
            }
            Err(e) => log::warn!("failed to ingest record from {}: {e:#}", path.display()),
        }
    }

    drop(conn);

    if anything_changed {
        let _ = app_handle.emit(
            "data-changed",
            serde_json::json!({
                "entity": "session",
                "kind": if session_created { "created" } else { "updated" },
            }),
        );
    }
}
