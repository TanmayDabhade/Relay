use crate::db::Db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// Free-tier ceilings enforced by `parser::session_builder::ingest_record`. Not
/// user-configurable — same rationale as `IDLE_THRESHOLD_SECS` in lib.rs.
pub const FREE_PROJECT_LIMIT: i64 = 3;
pub const FREE_SESSION_LIMIT: i64 = 50;

#[derive(Debug, Clone, Serialize)]
pub struct PlanSnapshot {
    pub email: Option<String>,
    pub plan: String,
}

impl Default for PlanSnapshot {
    fn default() -> Self {
        PlanSnapshot { email: None, plan: "free".to_string() }
    }
}

/// Caches the signed-in user's plan in memory, backed by the `auth_state` table so it
/// survives restarts and stays readable offline. The frontend owns the actual Supabase
/// session (see `src/hooks/useAuth.ts`) and pushes plan changes down via `set_current_plan`
/// — this state exists so the watcher's ingest pipeline and report/summary commands can
/// check the plan without depending on the frontend being mounted or online.
pub struct PlanState(pub Mutex<PlanSnapshot>);

pub fn is_paid(plan_state: &PlanState) -> bool {
    plan_state.0.lock().unwrap().plan == "paid"
}

/// Reads the cached plan at startup. Absence of a row (never signed in on this machine)
/// resolves to the free-tier default rather than an error.
pub fn load_plan_state(conn: &Connection) -> PlanSnapshot {
    conn.query_row(
        "SELECT email, plan FROM auth_state WHERE id = 1",
        [],
        |row| Ok(PlanSnapshot { email: row.get(0)?, plan: row.get(1)? }),
    )
    .optional()
    .unwrap_or(None)
    .unwrap_or_default()
}

fn persist_plan_state(conn: &Connection, snapshot: &PlanSnapshot) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO auth_state (id, email, plan, updated_at) VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET email = excluded.email, plan = excluded.plan, updated_at = excluded.updated_at",
        params![snapshot.email, snapshot.plan, chrono::Utc::now().timestamp()],
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_current_plan(plan_state: State<'_, PlanState>) -> PlanSnapshot {
    plan_state.0.lock().unwrap().clone()
}

/// Called by the frontend right after sign-in, on sign-out, and whenever it re-fetches
/// `profiles.plan` from Supabase (e.g. on window focus, or after the user returns from a
/// checkout flow) — see `src/hooks/useAuth.ts`. Persists the new snapshot before emitting
/// `data-changed` so any query that re-fetches in response (e.g. Reports, previously
/// erroring on a free plan) sees the update immediately rather than racing the DB write.
#[tauri::command]
pub fn set_current_plan(
    db: State<'_, Db>,
    plan_state: State<'_, PlanState>,
    app_handle: AppHandle,
    email: Option<String>,
    plan: String,
) -> Result<(), String> {
    let snapshot = PlanSnapshot { email, plan };

    let conn = db.0.lock().map_err(|e| e.to_string())?;
    persist_plan_state(&conn, &snapshot).map_err(|e| e.to_string())?;
    drop(conn);

    *plan_state.0.lock().unwrap() = snapshot;

    let _ = app_handle.emit("data-changed", serde_json::json!({ "entity": "plan", "kind": "updated" }));

    Ok(())
}
