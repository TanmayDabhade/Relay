use crate::db::{queries, Db};
use tauri::State;

#[tauri::command]
pub fn list_projects(db: State<'_, Db>) -> Result<Vec<queries::ProjectSummary>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::list_projects(&conn).map_err(|e| e.to_string())
}
