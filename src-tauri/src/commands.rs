use crate::activity;
use crate::assist;
use crate::db::{queries, Db};
use crate::dispatch::{self, looping, AgentCommand, Runtime};
use crate::parser;
use crate::ship::{self, ShipTrigger};
use crate::terminal;
use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use std::collections::HashSet;
use std::process::Command;
use tauri::{Emitter, Manager, State};

/// Width of the Dashboard's GitHub-style activity heatmap, in days.
const HEATMAP_DAYS: i64 = 365;

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
        .map(|opt| {
            opt.map(|(session, files_changed)| SessionDetail {
                session,
                files_changed,
            })
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: String,
) -> Result<(), String> {
    let deleted = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::delete_session(&conn, &session_id, Utc::now().timestamp())
            .map_err(|e| e.to_string())?
    };
    if !deleted {
        return Err(format!("session {session_id} not found"));
    }
    let _ = app.emit(
        "data-changed",
        serde_json::json!({ "entity": "session", "kind": "deleted", "session_id": session_id }),
    );
    Ok(())
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
        .map_err(|e| {
            format!("failed to launch editor: $EDITOR is not set and `code` failed to start: {e}")
        })
}

/// Returns a 14-day daily git-commit-count sparkline for the project at `project_path`, for
/// the decorative `ActivityBars` component on each project card. Returns `Vec<i64>` directly,
/// not `Result` — see `activity`'s module doc comment for why: every failure mode (not a git
/// repo, no `git` on `PATH`, shellout failure) already degrades to `vec![0; 14]` inside
/// `activity::project_activity`, so there's no error state left for the frontend to handle.
#[tauri::command]
pub fn project_activity(
    project_path: String,
    cache: State<'_, activity::ActivityCache>,
) -> Vec<i64> {
    activity::project_activity(&project_path, &cache)
}

/// Width of a project's Overview-tab commit heatmap, in days — same window as the
/// Dashboard's heatmap, for visual consistency between the two.
const GIT_HEATMAP_DAYS: i64 = 365;

/// How many recent commits the Overview tab's "Recent commits" list shows.
const RECENT_COMMITS_LIMIT: usize = 8;

/// Richer git-derived context for a single project's Overview tab: a full-year commit
/// heatmap (reusing the Dashboard's `ActivityHeatmap` component on the frontend) plus a
/// short list of the most recent commits. Unlike `project_activity`'s 14-day sparkline,
/// this isn't cached — it's only fetched once per Overview-tab visit, and react-query's
/// own 30s `staleTime` already absorbs repeat mounts.
#[derive(Debug, Clone, Serialize)]
pub struct GitInsights {
    /// Oldest first, one entry per day, `GIT_HEATMAP_DAYS` long (today inclusive) — commit
    /// counts, not session/usage activity.
    pub commit_heatmap: Vec<DailyActivity>,
    /// Newest first, capped at `RECENT_COMMITS_LIMIT`. Empty if `project_path` isn't a git
    /// repo, `git` isn't on `PATH`, or the shellout otherwise fails — same "degrade
    /// silently" contract as `project_activity`.
    pub recent_commits: Vec<activity::CommitInfo>,
}

#[tauri::command]
pub fn project_git_insights(project_path: String) -> GitInsights {
    let timestamps = activity::git_log_timestamps(&project_path, GIT_HEATMAP_DAYS);

    let mut counts_by_day: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    for ts in &timestamps {
        if let Some(dt) = chrono::DateTime::from_timestamp(*ts, 0) {
            *counts_by_day
                .entry(dt.format("%Y-%m-%d").to_string())
                .or_insert(0) += 1;
        }
    }

    let today = Utc::now().date_naive();
    let window_start = today - Duration::days(GIT_HEATMAP_DAYS - 1);
    let commit_heatmap = dense_daily_activity(window_start, today, &counts_by_day);

    let recent_commits = activity::git_recent_commits(&project_path, RECENT_COMMITS_LIMIT);

    GitInsights {
        commit_heatmap,
        recent_commits,
    }
}

/// Caps how many diff lines `get_file_diff_for_session_file` will ever serialize over IPC —
/// a `Write` of a very large generated file (e.g. a lockfile) would otherwise dump the entire
/// thing into the UI. Generous enough that real edits never hit it in practice.
const MAX_DIFF_LINES: usize = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// "insert" | "delete" | "equal".
    pub tag: &'static str,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub truncated: bool,
    /// When the most recent edit folded into this diff occurred.
    pub occurred_at: i64,
    /// How many separate tool-call edits (across the session) were folded into this diff —
    /// the frontend uses this to label a multi-edit file distinctly from a single-edit one.
    pub edit_count: i64,
}

/// Builds a single cumulative line-level diff for everything one session did to one file —
/// before-text from that file's earliest edit in the session, after-text from its most
/// recent — rather than one diff per tool call. Returns `Ok(None)` if this session never
/// touched `file_path`; returns `Ok(Some(FileDiff { lines: vec![], .. }))` if it did but
/// every row predates the migration that started capturing `old_content`/`new_content` (both
/// `NULL`) — the frontend tells these two "nothing to show" cases apart to explain *why*
/// there's no diff rather than just showing a blank panel.
#[tauri::command]
pub fn get_file_diff_for_session_file(
    db: State<'_, Db>,
    session_id: String,
    file_path: String,
) -> Result<Option<FileDiff>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let Some(span) =
        queries::file_diff_span(&conn, &session_id, &file_path).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };

    if span.old_content.is_none() && span.new_content.is_none() {
        return Ok(Some(FileDiff {
            lines: Vec::new(),
            truncated: false,
            occurred_at: span.latest_occurred_at,
            edit_count: span.edit_count,
        }));
    }

    let old = span.old_content.unwrap_or_default();
    let new = span.new_content.unwrap_or_default();

    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(&old, &new);
    let mut lines = Vec::new();
    let mut truncated = false;
    for change in diff.iter_all_changes() {
        if lines.len() >= MAX_DIFF_LINES {
            truncated = true;
            break;
        }
        let tag = match change.tag() {
            ChangeTag::Insert => "insert",
            ChangeTag::Delete => "delete",
            ChangeTag::Equal => "equal",
        };
        lines.push(DiffLine {
            tag,
            content: change.value().trim_end_matches('\n').to_string(),
        });
    }

    Ok(Some(FileDiff {
        lines,
        truncated,
        occurred_at: span.latest_occurred_at,
        edit_count: span.edit_count,
    }))
}

/// One day's worth of Dashboard heatmap data — a dense, zero-filled point (unlike
/// `queries::daily_activity_counts`'s sparse map) so the frontend can render a fixed grid
/// without doing its own gap-filling.
#[derive(Debug, Clone, Serialize)]
pub struct DailyActivity {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub total_cost_usd: f64,
    pub total_sessions: i64,
    /// Count of every project Relay knows about — independent of `top_projects`'s cap, so
    /// this stays accurate once there are more projects than the "highest usage" list shows.
    pub total_projects: i64,
    /// Oldest first, one entry per day, `HEATMAP_DAYS` long (today inclusive).
    pub daily_activity: Vec<DailyActivity>,
    /// Highest-spend projects first, capped for display — see `TOP_PROJECTS_LIMIT`.
    pub top_projects: Vec<queries::ProjectSummary>,
    pub agent_usage: Vec<queries::AgentUsage>,
    /// The most-recently-active `status = 'active'` session, if any — powers the Dashboard's
    /// "active now" widget.
    pub active_session: Option<queries::ActiveSessionSummary>,
}

/// How many projects the Dashboard's "highest usage" list shows.
const TOP_PROJECTS_LIMIT: usize = 5;

#[tauri::command]
pub fn dashboard_stats(db: State<'_, Db>) -> Result<DashboardStats, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;

    // Headline numbers (total_cost_usd/total_sessions/total_projects/top_projects) all derive
    // from this one list.
    let mut projects = queries::list_projects(&conn).map_err(|e| e.to_string())?;
    let total_cost_usd: f64 = projects.iter().map(|p| p.total_cost_usd).sum();
    let total_sessions: i64 = projects.iter().map(|p| p.session_count).sum();
    let total_projects = projects.len() as i64;

    // `list_projects` orders by `last_active DESC`; the Dashboard's "highest usage" list
    // needs spend order instead, so re-sort here rather than adding a second SQL query.
    projects.sort_by(|a, b| {
        b.total_cost_usd
            .partial_cmp(&a.total_cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    projects.truncate(TOP_PROJECTS_LIMIT);

    let agent_usage = queries::agent_usage(&conn).map_err(|e| e.to_string())?;
    let active_session = queries::most_recent_active_session(&conn).map_err(|e| e.to_string())?;

    let today = Utc::now().date_naive();
    let window_start = today - Duration::days(HEATMAP_DAYS - 1);
    let since_epoch = window_start
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always a valid time")
        .and_utc()
        .timestamp();
    let counts_by_day =
        queries::daily_activity_counts(&conn, since_epoch).map_err(|e| e.to_string())?;

    let daily_activity = dense_daily_activity(window_start, today, &counts_by_day);

    Ok(DashboardStats {
        total_cost_usd,
        total_sessions,
        total_projects,
        daily_activity,
        top_projects: projects,
        agent_usage,
        active_session,
    })
}

/// Walks `[start, end]` inclusive, one entry per calendar day, filling in `0` for any day
/// missing from `counts_by_day` — split out from `dashboard_stats` so the date-walking logic
/// is unit-testable without a DB connection.
fn dense_daily_activity(
    start: NaiveDate,
    end: NaiveDate,
    counts_by_day: &std::collections::HashMap<String, i64>,
) -> Vec<DailyActivity> {
    let mut out = Vec::new();
    let mut day = start;
    while day <= end {
        let date = day.format("%Y-%m-%d").to_string();
        let count = counts_by_day.get(&date).copied().unwrap_or(0);
        out.push(DailyActivity { date, count });
        day += Duration::days(1);
    }
    out
}

// --- Reports ---

/// Aggregated report payload for the Reports view: headline totals plus three breakdowns
/// (by project, by tag, by agent), all windowed to the same `[since_epoch, now]` range.
/// `range_days`/`since_epoch` are echoed back so the frontend and `render_report_markdown`
/// can label the window without recomputing it from `Utc::now()` a second time.
#[derive(Debug, Clone, Serialize)]
pub struct ReportData {
    pub range_days: i64,
    pub since_epoch: i64,
    pub totals: queries::ReportTotals,
    pub by_project: Vec<queries::ReportProjectRow>,
    pub by_tag: Vec<queries::ReportTagRow>,
    pub by_agent: Vec<queries::AgentUsage>,
}

fn build_report(conn: &rusqlite::Connection, range_days: i64) -> Result<ReportData, String> {
    let since_epoch = (Utc::now() - Duration::days(range_days)).timestamp();

    Ok(ReportData {
        range_days,
        since_epoch,
        totals: queries::report_totals(conn, since_epoch).map_err(|e| e.to_string())?,
        by_project: queries::report_by_project(conn, since_epoch).map_err(|e| e.to_string())?,
        by_tag: queries::report_by_tag(conn, since_epoch).map_err(|e| e.to_string())?,
        by_agent: queries::agent_usage_since(conn, since_epoch).map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub fn generate_report(db: State<'_, Db>, range_days: i64) -> Result<ReportData, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    build_report(&conn, range_days)
}

/// Renders the same data `generate_report` returns as a standalone Markdown document, writes
/// it to disk, and returns the absolute path — a shareable artifact for the manager/team-lead
/// audience the Reports view exists for, not just an in-app table. Written under the user's
/// Downloads directory (falling back to their home directory if that can't be resolved, e.g. a
/// locked-down sandbox) rather than the app-data directory, since the whole point is for the
/// user to find this file and hand it to someone else.
#[tauri::command]
pub fn export_report(db: State<'_, Db>, range_days: i64) -> Result<String, String> {
    let report = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        build_report(&conn, range_days)?
    };

    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not resolve a directory to save the report into".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let filename = format!("relay-report-{}.md", Utc::now().format("%Y-%m-%d-%H%M%S"));
    let path = dir.join(filename);
    std::fs::write(&path, render_report_markdown(&report)).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

fn render_report_markdown(report: &ReportData) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Relay spend report — last {} days\n\n",
        report.range_days
    ));
    out.push_str(&format!(
        "Generated {}\n\n",
        Utc::now().format("%Y-%m-%d %H:%M UTC")
    ));

    out.push_str("## Totals\n\n");
    out.push_str(&format!(
        "- Total spend: ${:.2}\n",
        report.totals.total_cost_usd
    ));
    out.push_str(&format!("- Sessions: {}\n", report.totals.session_count));
    let avg = if report.totals.session_count > 0 {
        report.totals.total_cost_usd / report.totals.session_count as f64
    } else {
        0.0
    };
    out.push_str(&format!("- Avg cost / session: ${avg:.2}\n"));
    out.push_str(&format!(
        "- Tokens: {} prompt, {} completion, {} cache read, {} cache write\n\n",
        report.totals.prompt_tokens,
        report.totals.completion_tokens,
        report.totals.cache_read_tokens,
        report.totals.cache_creation_tokens
    ));

    out.push_str("## By project\n\n| Project | Sessions | Spend |\n|---|---:|---:|\n");
    for row in &report.by_project {
        out.push_str(&format!(
            "| {} | {} | ${:.2} |\n",
            row.project_name, row.session_count, row.total_cost_usd
        ));
    }

    out.push_str("\n## By tag\n\n| Tag | Sessions | Spend |\n|---|---:|---:|\n");
    for row in &report.by_tag {
        out.push_str(&format!(
            "| {} | {} | ${:.2} |\n",
            row.tag, row.session_count, row.total_cost_usd
        ));
    }

    out.push_str("\n## By agent\n\n| Agent | Sessions | Spend |\n|---|---:|---:|\n");
    for row in &report.by_agent {
        out.push_str(&format!(
            "| {} | {} | ${:.2} |\n",
            row.agent, row.session_count, row.total_cost_usd
        ));
    }

    out
}

/// Opens `url` in the user's default browser — macOS only, matching Relay's current
/// platform scope.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    Command::new("open")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to open {url}: {e}"))
}

/// Reveals `path` in Finder — macOS only, matching Relay's current platform scope (see the
/// README's "Requirements"). Used by the Reports view's "Reveal in Finder" button right after
/// `export_report` writes a file, so the user doesn't have to hunt through Downloads for it.
#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to reveal {path} in Finder: {e}"))
}

/// Renders `session_id`'s raw agent log as a Markdown document without writing a file, so the
/// frontend can copy it without leaving an unwanted export in Downloads.
#[tauri::command]
pub fn get_transcript_markdown(db: State<'_, Db>, session_id: String) -> Result<String, String> {
    let session = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::get_session(&conn, &session_id).map_err(|e| e.to_string())?
    };
    let session = session.ok_or_else(|| format!("session {session_id} not found"))?;

    let transcript = parser::render_markdown(&session.raw_log_path).map_err(|e| e.to_string())?;
    Ok(render_transcript_document(&session, &transcript))
}

/// Renders `session_id`'s raw Claude Code log as a readable Markdown transcript and writes it
/// to the user's Downloads directory, mirroring `export_report`'s save-and-return-path
/// contract so the frontend can reuse the same `reveal_in_finder` follow-up action.
#[tauri::command]
pub fn export_transcript(db: State<'_, Db>, session_id: String) -> Result<String, String> {
    let session = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::get_session(&conn, &session_id).map_err(|e| e.to_string())?
    };
    let session = session.ok_or_else(|| format!("session {session_id} not found"))?;

    let transcript = parser::render_markdown(&session.raw_log_path).map_err(|e| e.to_string())?;

    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not resolve a directory to save the transcript into".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let filename = format!(
        "relay-transcript-{}-{}.md",
        &session.id[..8],
        Utc::now().format("%Y-%m-%d-%H%M%S")
    );
    let path = dir.join(filename);
    std::fs::write(&path, render_transcript_document(&session, &transcript))
        .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

fn render_transcript_header(session: &queries::Session) -> String {
    let title = session
        .title
        .as_deref()
        .or(session.summary.as_deref())
        .unwrap_or("Untitled session");

    format!(
        "# {title}\n\n- Session: {}\n- Model: {}\n- Status: {}\n- Cost: ${:.2}\n\n---\n\n",
        session.id,
        session.model.as_deref().unwrap_or("unknown"),
        session.status,
        session.cost_usd,
    )
}

fn render_transcript_document(session: &queries::Session, transcript: &str) -> String {
    format!("{}{transcript}", render_transcript_header(session))
}

// --- Relay-owned agent dispatch ---

const BUILT_IN_AGENTS: [&str; 4] = ["claude", "codex", "gemini", "cursor"];

#[derive(Debug, Clone, Serialize)]
pub struct AgentConnection {
    pub agent: String,
    pub enabled: bool,
    pub executable: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub updated_at: i64,
    pub installed: bool,
    pub resolved_executable: Option<String>,
}

fn validate_agent_config(
    agent: &str,
    executable: &str,
    models: Vec<String>,
    default_model: &str,
) -> Result<Vec<String>, String> {
    if !BUILT_IN_AGENTS.contains(&agent) {
        return Err(format!("unsupported built-in agent: {agent}"));
    }
    if executable.trim().is_empty() {
        return Err("executable cannot be empty".to_string());
    }

    let mut seen = HashSet::new();
    let models: Vec<String> = models
        .into_iter()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty() && seen.insert(model.clone()))
        .collect();
    if models.is_empty() {
        return Err("configure at least one model".to_string());
    }
    if !models.iter().any(|model| model == default_model) {
        return Err("the default model must be in the configured model list".to_string());
    }
    Ok(models)
}

#[tauri::command]
pub fn list_agent_connections(db: State<'_, Db>) -> Result<Vec<AgentConnection>, String> {
    let configs = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::list_agent_configs(&conn).map_err(|e| e.to_string())?
    };
    Ok(configs
        .into_iter()
        .map(|config| {
            let resolved = dispatch::resolve_executable(&config.executable);
            AgentConnection {
                agent: config.agent,
                enabled: config.enabled,
                executable: config.executable,
                models: config.models,
                default_model: config.default_model,
                updated_at: config.updated_at,
                installed: resolved.is_some(),
                resolved_executable: resolved.map(|path| path.to_string_lossy().to_string()),
            }
        })
        .collect())
}

#[tauri::command]
pub fn save_agent_connection(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    agent: String,
    enabled: bool,
    executable: String,
    models: Vec<String>,
    default_model: String,
) -> Result<(), String> {
    let models = validate_agent_config(&agent, &executable, models, &default_model)?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::update_agent_config(
        &conn,
        &agent,
        enabled,
        executable.trim(),
        &models,
        &default_model,
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

fn configured_command(
    config: &queries::AgentConfig,
    model: &str,
    prompt: &str,
    provider_session_id: Option<&str>,
) -> Result<AgentCommand, String> {
    if !config.enabled {
        return Err(format!("{} is disabled in Connections", config.agent));
    }
    if !config.models.iter().any(|configured| configured == model) {
        return Err(format!(
            "model {model} is not configured for {}",
            config.agent
        ));
    }
    let resolved = dispatch::resolve_executable(&config.executable).ok_or_else(|| {
        format!(
            "{} executable '{}' was not found; update it in Connections",
            config.agent, config.executable
        )
    })?;
    let mut command =
        dispatch::build_agent_command(&config.agent, model, prompt, provider_session_id)
            .map_err(|e| e.to_string())?;
    command.executable = resolved.to_string_lossy().to_string();
    Ok(command)
}

fn mark_dispatch_start_failed(app: &tauri::AppHandle, run_id: &str, turn_id: &str, error: &str) {
    let db = app.state::<Db>();
    if let Ok(conn) = db.0.lock() {
        if let Err(db_error) = queries::settle_dispatch_turn(
            &conn,
            run_id,
            turn_id,
            "failed",
            "failed",
            None,
            Some(error),
            Utc::now().timestamp(),
        ) {
            log::warn!("failed to record launch failure for run {run_id}: {db_error:#}");
        }
        let _ = queries::clear_pending_launch_for_dispatch_run(&conn, run_id);
        let _ = queries::sync_card_for_dispatch_run(&conn, run_id, "review");
    }
    emit_data_changed(app);
}

#[tauri::command]
pub fn dispatch_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    runtime: State<'_, Runtime>,
    project_id: String,
    card_id: Option<String>,
    title: String,
    prompt: String,
    agent: String,
    model: String,
    loop_max_iterations: Option<i64>,
    open_pr: Option<bool>,
) -> Result<queries::CreatedDispatch, String> {
    let loop_max_iterations = looping::validate_max_iterations(loop_max_iterations)?;
    let title = title.trim();
    let prompt = prompt.trim();
    if title.is_empty() {
        return Err("task title cannot be empty".to_string());
    }
    if prompt.is_empty() {
        return Err("task prompt cannot be empty".to_string());
    }

    let (config, project_path) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let config = queries::get_agent_config(&conn, &agent)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("agent {agent} is not configured"))?;
        let project_path = queries::project_path(&conn, &project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("project {project_id} was not found"))?;
        (config, project_path)
    };
    // The card and task keep the user's own words; only what the agent sees (and the turn
    // transcript, so it stays truthful) carries the loop's stopping contract.
    let agent_prompt = match loop_max_iterations {
        Some(_) => looping::initial_prompt(prompt),
        None => prompt.to_string(),
    };
    let command = configured_command(&config, &model, &agent_prompt, None)?;

    // The worktree is created before any task row exists (git I/O, so no DB lock), keyed by a
    // throwaway id that only names the branch/folder. Every failure after this point must
    // discard it, or a dead branch and folder would be left behind.
    let workspace = if open_pr.unwrap_or(false) {
        let root = ship::worktrees_root(&app).map_err(|e| e.to_string())?;
        let naming_id = uuid::Uuid::new_v4().to_string();
        Some(
            ship::git::prepare_workspace(
                &root,
                &project_path,
                &naming_id,
                title,
                true,
                Utc::now().timestamp(),
            )
            .map_err(|e| format!("{e:#}"))?,
        )
    } else {
        None
    };
    let discard_workspace = |workspace: &Option<queries::TaskWorkspace>| {
        if let Some(workspace) = workspace {
            if let Err(error) = ship::git::discard_workspace(workspace) {
                log::warn!(
                    "could not discard worktree {}: {error:#}",
                    workspace.worktree_path
                );
            }
        }
    };

    let created = (|| -> Result<queries::CreatedDispatch, String> {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        if queries::has_active_dispatch_for_project_agent(&transaction, &project_id, &agent)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "{agent} is already running a turn in this project; wait for it to finish or interrupt it"
            ));
        }
        let (board, columns, cards) =
            queries::get_board(&transaction, &project_id).map_err(|e| e.to_string())?;
        let in_progress = columns
            .iter()
            .find(|column| column.role.as_deref() == Some("in_progress"))
            .ok_or_else(|| "this project board has no In Progress column".to_string())?;

        let task_card_id = if let Some(card_id) = card_id.as_deref() {
            let context = queries::card_launch_context(&transaction, card_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("card {card_id} was not found"))?;
            if context.project_id != project_id {
                return Err("the selected card belongs to a different project".to_string());
            }
            if context.session_id.is_some() {
                return Err("this card is already linked to an agent session".to_string());
            }
            let position = cards
                .iter()
                .filter(|card| card.column_id == in_progress.id)
                .count() as i64;
            queries::move_card(&transaction, card_id, &in_progress.id, position)
                .map_err(|e| e.to_string())?;
            card_id.to_string()
        } else {
            queries::create_card(
                &transaction,
                &board.id,
                &in_progress.id,
                title,
                Some(prompt),
            )
            .map_err(|e| e.to_string())?
            .id
        };
        queries::set_card_pending_launch_for_agent(&transaction, &task_card_id, &agent)
            .map_err(|e| e.to_string())?;
        let created = queries::create_dispatch_task(
            &transaction,
            &project_id,
            Some(&task_card_id),
            title,
            prompt,
            &agent,
            &model,
            loop_max_iterations,
            Utc::now().timestamp(),
        )
        .map_err(|e| e.to_string())?;
        if let Some(workspace) = &workspace {
            let mut row = workspace.clone();
            row.task_id = created.task.id.clone();
            queries::insert_task_workspace(&transaction, &row).map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(created)
    })();
    let created = match created {
        Ok(created) => created,
        Err(error) => {
            discard_workspace(&workspace);
            return Err(error);
        }
    };
    let working_dir = workspace
        .as_ref()
        .map(|workspace| workspace.work_dir.clone())
        .unwrap_or_else(|| project_path.clone());

    let (turn, user_event) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::begin_dispatch_turn(
            &conn,
            &created.run.id,
            &agent_prompt,
            Utc::now().timestamp(),
        )
        .map_err(|e| e.to_string())?
    };
    dispatch::emit_dispatch_event(&app, &user_event);
    if let Err(error) = runtime.start_turn(
        app.clone(),
        created.run.id.clone(),
        turn.id.clone(),
        agent.clone(),
        &working_dir,
        command,
    ) {
        let message = error.to_string();
        mark_dispatch_start_failed(&app, &created.run.id, &turn.id, &message);
        return Err(format!("the run could not start: {message}"));
    }
    emit_data_changed(&app);
    Ok(created)
}

#[tauri::command]
pub fn retry_dispatch_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    runtime: State<'_, Runtime>,
    task_id: String,
    agent: String,
    model: String,
) -> Result<queries::CreatedDispatch, String> {
    let gathered = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let task = queries::get_dispatch_task(&conn, &task_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("task {task_id} was not found"))?;
        let config = queries::get_agent_config(&conn, &agent)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("agent {agent} is not configured"))?;
        let project_path = queries::project_path(&conn, &task.project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("project {} was not found", task.project_id))?;
        let runs = queries::list_runs_for_task(&conn, &task_id).map_err(|e| e.to_string())?;
        // A retry is "the same task again", so it keeps the previous attempt's loop setting.
        let loop_max_iterations = runs.last().and_then(|run| run.loop_max_iterations);
        let active = runs.iter().any(|run| {
            matches!(
                run.status.as_str(),
                "queued"
                    | "starting"
                    | "running"
                    | "awaiting_approval"
                    | "interrupting"
                    | "idle"
                    | "failed"
            )
        });
        if active {
            return Err("this task already has an active run".to_string());
        }
        let workspace = queries::get_task_workspace(&conn, &task_id).map_err(|e| e.to_string())?;
        (task, config, project_path, loop_max_iterations, workspace)
    };
    let (task, config, project_path, loop_max_iterations, workspace) = gathered;
    // A shut-down conversation's worktree was removed; recreate it on the same branch so the
    // retry keeps building (and shipping) onto the same PR. Git I/O, so outside the lock.
    let working_dir = match &workspace {
        Some(workspace) => {
            ship::git::ensure_worktree(workspace).map_err(|e| format!("{e:#}"))?;
            let conn = db.0.lock().map_err(|e| e.to_string())?;
            queries::set_task_workspace_removed(&conn, &task_id, None)
                .map_err(|e| e.to_string())?;
            workspace.work_dir.clone()
        }
        None => project_path,
    };
    let agent_prompt = match loop_max_iterations {
        Some(_) => looping::initial_prompt(&task.prompt),
        None => task.prompt.clone(),
    };
    let command = configured_command(&config, &model, &agent_prompt, None)?;
    let run = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        if queries::has_active_dispatch_for_project_agent(&conn, &task.project_id, &agent)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "{agent} is already running a turn in this project; wait for it to finish or interrupt it"
            ));
        }
        if let Some(card_id) = task.card_id.as_deref() {
            queries::set_card_pending_launch_for_agent(&conn, card_id, &agent)
                .map_err(|e| e.to_string())?;
        }
        queries::create_retry_run(
            &conn,
            &task_id,
            &agent,
            &model,
            loop_max_iterations,
            Utc::now().timestamp(),
        )
        .map_err(|e| e.to_string())?
    };
    let created = queries::CreatedDispatch { task, run };
    let (turn, user_event) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::begin_dispatch_turn(
            &conn,
            &created.run.id,
            &agent_prompt,
            Utc::now().timestamp(),
        )
        .map_err(|e| e.to_string())?
    };
    dispatch::emit_dispatch_event(&app, &user_event);
    if let Err(error) = runtime.start_turn(
        app.clone(),
        created.run.id.clone(),
        turn.id.clone(),
        agent.clone(),
        &working_dir,
        command,
    ) {
        let message = error.to_string();
        mark_dispatch_start_failed(&app, &created.run.id, &turn.id, &message);
        return Err(format!("the retry could not start: {message}"));
    }
    emit_data_changed(&app);
    Ok(created)
}

#[tauri::command]
pub fn list_dispatch_tasks(
    db: State<'_, Db>,
    day_start: i64,
    day_end: i64,
) -> Result<Vec<queries::DispatchTaskWithRun>, String> {
    if day_end <= day_start {
        return Err("day_end must be after day_start".to_string());
    }
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_dispatch_tasks(&conn, day_start, day_end).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_dispatch_runs(
    db: State<'_, Db>,
    task_id: String,
) -> Result<Vec<queries::DispatchRun>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_runs_for_task(&conn, &task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_dispatch_conversation(
    db: State<'_, Db>,
    run_id: String,
) -> Result<Option<queries::DispatchConversation>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_dispatch_conversation(&conn, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn send_dispatch_prompt(
    app: tauri::AppHandle,
    run_id: String,
    prompt: String,
) -> Result<(), String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("prompt cannot be empty".to_string());
    }
    start_follow_up_turn(&app, &run_id, prompt)
        .map_err(|message| format!("the next turn could not start: {message}"))?;
    emit_data_changed(&app);
    Ok(())
}

/// Starts another turn on an existing conversation, resuming its provider session. Shared by
/// a user's follow-up prompt and the loop's automatic continuation so both go through the
/// same readiness check (`begin_dispatch_turn` refuses a running or shut-down conversation).
fn start_follow_up_turn(app: &tauri::AppHandle, run_id: &str, prompt: &str) -> Result<(), String> {
    let db = app.state::<Db>();
    let (run, config, project_path) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let run = queries::get_dispatch_run(&conn, run_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("conversation {run_id} was not found"))?;
        if run.status == "shut_down" {
            return Err("this conversation has been shut down".to_string());
        }
        let task = queries::get_dispatch_task(&conn, &run.task_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("task {} was not found", run.task_id))?;
        let config = queries::get_agent_config(&conn, &run.agent)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("agent {} is not configured", run.agent))?;
        let project_path = queries::project_path(&conn, &task.project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("project {} was not found", task.project_id))?;
        // A PR-bound task keeps every turn in its worktree; the provider session it resumes
        // was started there too.
        let working_dir = queries::get_task_workspace(&conn, &task.id)
            .map_err(|e| e.to_string())?
            .map(|workspace| workspace.work_dir)
            .unwrap_or(project_path);
        (run, config, working_dir)
    };
    let command = configured_command(
        &config,
        &run.model,
        prompt,
        run.provider_session_id.as_deref(),
    )?;
    let (turn, user_event) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::begin_dispatch_turn(&conn, run_id, prompt, Utc::now().timestamp())
            .map_err(|e| e.to_string())?
    };
    dispatch::emit_dispatch_event(app, &user_event);
    if let Err(error) = app.state::<Runtime>().start_turn(
        app.clone(),
        run_id.to_string(),
        turn.id.clone(),
        run.agent,
        &project_path,
        command,
    ) {
        let message = error.to_string();
        mark_dispatch_start_failed(app, run_id, &turn.id, &message);
        return Err(message);
    }
    Ok(())
}

/// Called by the runtime once a turn has fully settled (DB updated, active handle removed).
/// Runs on the runtime's waiter thread, never under the DB lock: the gather step takes the
/// lock briefly, and starting the next turn re-acquires it on its own.
pub fn continue_dispatch_loop(
    app: &tauri::AppHandle,
    run_id: &str,
    turn_id: &str,
    turn_status: &str,
) {
    let db = app.state::<Db>();
    let gathered = {
        let Ok(conn) = db.0.lock() else { return };
        queries::get_dispatch_run(&conn, run_id).and_then(|run| {
            let last = queries::last_assistant_message_for_turn(&conn, turn_id)?;
            Ok(run.map(|run| (run, last)))
        })
    };
    let (run, last_message) = match gathered {
        Ok(Some(found)) => found,
        Ok(None) => return,
        Err(error) => {
            log::warn!("could not evaluate loop for run {run_id}: {error:#}");
            return;
        }
    };

    match looping::decide(
        turn_status,
        &run.status,
        run.loop_max_iterations,
        run.loop_iterations,
        last_message.as_deref(),
    ) {
        looping::LoopDecision::Stop => {
            // A non-looping turn that completed cleanly is the task's finish line. Every such
            // turn ships, so a follow-up's new commits are pushed onto the same PR.
            if turn_status == "completed"
                && run.status == "idle"
                && run.loop_max_iterations.is_none()
            {
                auto_ship(app, &run.task_id);
            }
        }
        looping::LoopDecision::Done => {
            append_loop_notice(
                app,
                run_id,
                turn_id,
                "completed",
                "Loop finished: the agent reported the task is done.",
            );
            auto_ship(app, &run.task_id);
        }
        looping::LoopDecision::CapReached { max } => {
            append_loop_notice(
                app,
                run_id,
                turn_id,
                "warning",
                &format!("Loop stopped after {max} continuation(s) without the agent reporting the task done."),
            );
        }
        looping::LoopDecision::Continue { iteration, max } => {
            let claimed = db.0.lock().map_err(|e| e.to_string()).and_then(|conn| {
                queries::claim_dispatch_loop_iteration(&conn, run_id, iteration)
                    .map_err(|e| e.to_string())
            });
            match claimed {
                Ok(true) => {}
                // Looping was stopped (or the step claimed) between gather and now.
                Ok(false) => return,
                Err(error) => {
                    log::warn!("could not claim loop iteration for run {run_id}: {error}");
                    return;
                }
            }
            let prompt = looping::continuation_prompt(iteration, max);
            if let Err(error) = start_follow_up_turn(app, run_id, &prompt) {
                append_loop_notice(
                    app,
                    run_id,
                    turn_id,
                    "error",
                    &format!("Loop could not continue: {error}"),
                );
            }
            emit_data_changed(app);
        }
    }
}

/// Ships a finished task if it opted into auto-shipping (see `ship::spawn_ship`).
fn auto_ship(app: &tauri::AppHandle, task_id: &str) {
    if let Err(error) = ship::spawn_ship(app, task_id, ShipTrigger::Auto) {
        log::warn!("could not start shipping task {task_id}: {error:#}");
    }
}

fn append_loop_notice(
    app: &tauri::AppHandle,
    run_id: &str,
    turn_id: &str,
    state: &str,
    content: &str,
) {
    let db = app.state::<Db>();
    let event = match db.0.lock() {
        Ok(conn) => queries::append_dispatch_event(
            &conn,
            run_id,
            Some(turn_id),
            if state == "error" { "error" } else { "status" },
            None,
            content,
            None,
            None,
            Some(state),
            Utc::now().timestamp(),
        ),
        Err(_) => return,
    };
    match event {
        Ok(event) => dispatch::emit_dispatch_event(app, &event),
        Err(error) => log::warn!("could not record loop notice for run {run_id}: {error:#}"),
    }
}

#[tauri::command]
pub fn stop_dispatch_loop(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    run_id: String,
) -> Result<(), String> {
    let stopped = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::stop_dispatch_loop(&conn, &run_id).map_err(|e| e.to_string())?
    };
    if stopped {
        emit_data_changed(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn interrupt_dispatch_turn(
    app: tauri::AppHandle,
    runtime: State<'_, Runtime>,
    run_id: String,
) -> Result<(), String> {
    runtime
        .interrupt_turn(&app, &run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_dispatch_approval(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    runtime: State<'_, Runtime>,
    run_id: String,
    event_id: i64,
    decision: String,
) -> Result<(), String> {
    if !matches!(
        decision.as_str(),
        "allowed_once" | "allowed_for_session" | "denied"
    ) {
        return Err("invalid approval decision".to_string());
    }
    let (agent, request_id) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let conversation = queries::get_dispatch_conversation(&conn, &run_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("conversation {run_id} was not found"))?;
        let approval = conversation
            .events
            .into_iter()
            .find(|event| event.id == event_id && event.kind == "approval_request")
            .ok_or_else(|| format!("approval event {event_id} was not found"))?;
        if approval.state.as_deref() != Some("pending") {
            return Ok(());
        }
        (conversation.run.agent, approval.provider_event_id)
    };
    runtime
        .resolve_approval(&run_id, &agent, request_id.as_deref(), &decision)
        .map_err(|e| e.to_string())?;
    {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::resolve_dispatch_event(&conn, &run_id, event_id, &decision)
            .map_err(|e| e.to_string())?;
        queries::set_dispatch_run_status(&conn, &run_id, "running", None, Utc::now().timestamp())
            .map_err(|e| e.to_string())?;
    }
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn shutdown_dispatch_conversation(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    runtime: State<'_, Runtime>,
    run_id: String,
) -> Result<(), String> {
    runtime
        .stop_for_shutdown(&run_id)
        .map_err(|e| e.to_string())?;
    let task_id = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::shutdown_dispatch_conversation(&conn, &run_id, Utc::now().timestamp())
            .map_err(|e| e.to_string())?;
        queries::clear_pending_launch_for_dispatch_run(&conn, &run_id)
            .map_err(|e| e.to_string())?;
        queries::sync_card_for_dispatch_run(&conn, &run_id, "review").map_err(|e| e.to_string())?;
        queries::get_dispatch_run(&conn, &run_id)
            .map_err(|e| e.to_string())?
            .map(|run| run.task_id)
    };
    // Nothing can run in the worktree any more, so free it. A retry recreates it.
    if let Some(task_id) = task_id {
        ship::spawn_cleanup(&app, &task_id);
    }
    emit_data_changed(&app);
    Ok(())
}

// --- Shipping (commit + push + PR) ---

#[tauri::command]
pub fn get_task_workspace(
    db: State<'_, Db>,
    task_id: String,
) -> Result<Option<queries::TaskWorkspace>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_task_workspace(&conn, &task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_card_ships(
    db: State<'_, Db>,
    project_id: String,
) -> Result<Vec<queries::CardShip>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_card_ships(&conn, &project_id).map_err(|e| e.to_string())
}

/// Manual "Ship": commit, push, and open (or update) the PR now, regardless of the task's
/// auto-ship toggle. Refused while a turn is running, since the agent may still be editing.
#[tauri::command]
pub fn ship_task(app: tauri::AppHandle, db: State<'_, Db>, task_id: String) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let workspace = queries::get_task_workspace(&conn, &task_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "this task was not dispatched with “Open PR when done”".to_string())?;
        if workspace.removed_at.is_some() {
            return Err("this task's worktree was cleaned up; retry the task to ship again".into());
        }
        let busy = queries::latest_run_for_task(&conn, &task_id)
            .map_err(|e| e.to_string())?
            .is_some_and(|run| {
                matches!(
                    run.status.as_str(),
                    "queued" | "starting" | "running" | "awaiting_approval" | "interrupting"
                )
            });
        if busy {
            return Err("wait for the agent's current turn to finish before shipping".into());
        }
    }
    let started =
        ship::spawn_ship(&app, &task_id, ShipTrigger::Manual).map_err(|e| format!("{e:#}"))?;
    if !started {
        return Err("this task is already being shipped".into());
    }
    Ok(())
}

// --- Planner & task-draft assistants ---

fn assist_context(db: &Db, project_id: &str) -> Result<(String, String, Vec<String>), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let config = queries::get_agent_config(&conn, "claude")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "the claude agent is not configured in Connections".to_string())?;
    if !config.enabled {
        return Err("the assistants use the claude CLI, which is disabled in Connections".into());
    }
    let project_path = queries::project_path(&conn, project_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("project {project_id} was not found"))?;
    let (_, _, cards) = queries::get_board(&conn, project_id).map_err(|e| e.to_string())?;
    let titles = cards.into_iter().map(|card| card.title).collect();
    Ok((config.executable, project_path, titles))
}

/// Turns a rough note into a precise agent task. Async + `spawn_blocking`: the CLI call takes
/// tens of seconds and must neither block the UI thread nor hold the DB lock.
#[tauri::command]
pub async fn draft_task(
    db: State<'_, Db>,
    project_id: String,
    rough: String,
    current_title: Option<String>,
) -> Result<assist::TaskDraft, String> {
    if rough.trim().is_empty() {
        return Err("write a rough note first".into());
    }
    let (executable, project_path, _) = assist_context(&db, &project_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        assist::draft_task(
            &executable,
            std::path::Path::new(&project_path),
            &rough,
            current_title.as_deref().unwrap_or(""),
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn plan_tasks(
    db: State<'_, Db>,
    project_id: String,
    goal: String,
) -> Result<assist::Plan, String> {
    if goal.trim().is_empty() {
        return Err("describe what you want to get done first".into());
    }
    let (executable, project_path, titles) = assist_context(&db, &project_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        assist::plan_tasks(
            &executable,
            std::path::Path::new(&project_path),
            &goal,
            &titles,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct NewPlannedCard {
    pub title: String,
    pub prompt: String,
}

/// Adds approved plan tasks to the board's Todo column, in plan order, as one transaction.
/// The prompt becomes the card description, which is what dispatching a card sends.
#[tauri::command]
pub fn create_planned_cards(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    project_id: String,
    tasks: Vec<NewPlannedCard>,
) -> Result<Vec<queries::Card>, String> {
    let tasks: Vec<_> = tasks
        .into_iter()
        .filter(|task| !task.title.trim().is_empty())
        .collect();
    if tasks.is_empty() {
        return Err("select at least one task".into());
    }
    let cards = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let (board_id, todo_id) = queries::todo_column_for_project(&transaction, &project_id)
            .map_err(|e| e.to_string())?;
        let mut cards = Vec::with_capacity(tasks.len());
        for task in &tasks {
            let prompt = task.prompt.trim();
            cards.push(
                queries::create_card(
                    &transaction,
                    &board_id,
                    &todo_id,
                    task.title.trim(),
                    (!prompt.is_empty()).then_some(prompt),
                )
                .map_err(|e| e.to_string())?,
            );
        }
        transaction.commit().map_err(|e| e.to_string())?;
        cards
    };
    emit_data_changed(&app);
    Ok(cards)
}

// --- Kanban board ---

/// Emits the same coarse `data-changed` event every other mutation path uses — the frontend
/// hook invalidates by query key, not by payload, so a new event *kind* isn't needed here.
fn emit_data_changed(app: &tauri::AppHandle) {
    let _ = app.emit(
        "data-changed",
        serde_json::json!({ "entity": "board", "kind": "updated" }),
    );
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardData {
    pub board: queries::Board,
    pub columns: Vec<queries::Column>,
    pub cards: Vec<queries::Card>,
}

#[tauri::command]
pub fn get_board(db: State<'_, Db>, project_id: String) -> Result<BoardData, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_board(&conn, &project_id)
        .map(|(board, columns, cards)| BoardData {
            board,
            columns,
            cards,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_card(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    board_id: String,
    column_id: String,
    title: String,
    description: Option<String>,
) -> Result<queries::Card, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let card = queries::create_card(&conn, &board_id, &column_id, &title, description.as_deref())
        .map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(card)
}

#[tauri::command]
pub fn move_card(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    card_id: String,
    column_id: String,
    position: i64,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::move_card(&conn, &card_id, &column_id, position).map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn update_card(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    card_id: String,
    title: String,
    description: Option<String>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::update_card(&conn, &card_id, &title, description.as_deref())
        .map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_card(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    card_id: String,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::delete_card(&conn, &card_id).map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn link_session_to_card(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    card_id: String,
    session_id: String,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::link_session_to_card(&conn, &card_id, &session_id).map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn create_column(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    board_id: String,
    name: String,
) -> Result<queries::Column, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let column = queries::create_column(&conn, &board_id, &name).map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(column)
}

#[tauri::command]
pub fn rename_column(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    column_id: String,
    name: String,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::rename_column(&conn, &column_id, &name).map_err(|e| e.to_string())?;
    drop(conn);
    emit_data_changed(&app);
    Ok(())
}

/// Called when a card with no linked session is dropped onto a board's seeded "in_progress"
/// column: attaches the card's title/description as a prompt to a live `claude` CLI session
/// already running for that project (if one's open in a Terminal.app tab), resumes the most
/// recently active session for that project in a new Terminal window (if Relay's own DB
/// shows one active but no matching tab was found), or starts a brand new session otherwise.
///
/// Returns a short outcome string (`"attached_existing_tab"`, `"resumed_in_new_window"`,
/// `"started_new_window"`, or a `"skipped: ..."` reason) rather than `()` — purely for the
/// frontend to log, not surfaced as an error, since "the card wasn't actually eligible" is
/// an expected outcome (e.g. a card already linked to a session, or the frontend racing a
/// second drop event) rather than a failure.
///
/// Deliberately releases the DB lock before calling `terminal::attach_or_launch` — that call
/// blocks for a couple of seconds (AppleScript delays while a new Terminal window's `claude`
/// boots up), and holding the single shared connection mutex across that would stall every
/// other DB access for the duration, the same lock-discipline concern documented on
/// `lib.rs`'s idle sweep.
#[tauri::command]
pub fn launch_or_attach_session(db: State<'_, Db>, card_id: String) -> Result<String, String> {
    let prepared = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;

        let Some(context) =
            queries::card_launch_context(&conn, &card_id).map_err(|e| e.to_string())?
        else {
            return Ok("skipped: card not found".to_string());
        };
        if context.session_id.is_some() {
            return Ok("skipped: card is already linked to a session".to_string());
        }
        if context.column_role.as_deref() != Some("in_progress") {
            return Ok("skipped: card is not on the in_progress column".to_string());
        }

        let resume_id =
            queries::most_recent_active_session_id_for_project(&conn, &context.project_id)
                .map_err(|e| e.to_string())?;

        // Stamp the card so the session Claude Code is about to create gets adopted into it by
        // the ingest path (queries::adopt_pending_card_for_session), rather than spawning a
        // duplicate auto-created card. Done here, before the lock is released and the terminal
        // opens, so the stamp is durable well before the first log line lands.
        queries::set_card_pending_launch(&conn, &card_id).map_err(|e| e.to_string())?;

        (context, resume_id)
    };
    let (context, resume_id) = prepared;

    let mut prompt = context.title;
    if let Some(description) = context.description.filter(|d| !d.trim().is_empty()) {
        prompt.push_str("\n\n");
        prompt.push_str(&description);
    }

    terminal::attach_or_launch(&context.project_path, resume_id.as_deref(), &prompt)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_document_includes_session_metadata_and_body() {
        let session = queries::Session {
            id: "session-1234".into(),
            project_id: "project-1".into(),
            agent: "claude".into(),
            model: Some("claude-sonnet-4-5".into()),
            started_at: Some(1_700_000_000),
            ended_at: Some(1_700_000_060),
            last_activity_at: 1_700_000_060,
            status: "ended".into(),
            duration_seconds: Some(60),
            summary: Some("Fallback summary".into()),
            title: Some("Copy button work".into()),
            prompt_tokens: 100,
            completion_tokens: 50,
            cache_read_tokens: 25,
            cache_creation_tokens: 10,
            cache_creation_1h_tokens: 0,
            cost_usd: 0.125,
            cost_unpriced: false,
            lines_added: 12,
            lines_removed: 3,
            tags: Some("feature".into()),
            raw_log_path: "/tmp/session.jsonl".into(),
        };

        let result = render_transcript_document(&session, "## User\n\nAdd a copy button.\n");

        assert_eq!(
            result,
            "# Copy button work\n\n- Session: session-1234\n- Model: claude-sonnet-4-5\n- Status: ended\n- Cost: $0.12\n\n---\n\n## User\n\nAdd a copy button.\n",
        );
    }

    #[test]
    fn dense_daily_activity_fills_gaps_and_keeps_range_inclusive() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap();
        let mut counts = std::collections::HashMap::new();
        counts.insert("2026-01-02".to_string(), 5);

        let result = dense_daily_activity(start, end, &counts);

        assert_eq!(
            result
                .iter()
                .map(|d| (d.date.as_str(), d.count))
                .collect::<Vec<_>>(),
            vec![("2026-01-01", 0), ("2026-01-02", 5), ("2026-01-03", 0)],
        );
    }

    #[test]
    fn agent_config_validation_trims_deduplicates_and_requires_the_default() {
        let models = validate_agent_config(
            "codex",
            " codex ",
            vec!["default".into(), " gpt-5 ".into(), "gpt-5".into()],
            "gpt-5",
        )
        .unwrap();
        assert_eq!(models, vec!["default", "gpt-5"]);

        assert!(
            validate_agent_config("custom", "agent", vec!["default".into()], "default").is_err()
        );
        assert!(validate_agent_config("claude", "", vec!["default".into()], "default").is_err());
        assert!(validate_agent_config("claude", "claude", vec!["sonnet".into()], "opus").is_err());
    }
}
