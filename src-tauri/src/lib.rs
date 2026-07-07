mod commands;
mod db;
mod parser;
mod watcher;

use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      let app_data_dir = app.path().app_data_dir()?;
      let db_path = app_data_dir.join("manageai.db");
      let conn = db::open(&db_path)?;
      app.manage(db::Db(Mutex::new(conn)));

      watcher::start(app.handle().clone());

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![commands::list_projects])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
