mod activity;
mod commands;
mod cost;
mod db;
mod parser;
mod summarize;
mod tags;
mod watcher;

use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Idle-session sweep: a session with no new activity for this long is considered ended.
/// Constant, not yet user-configurable (per the plan).
const IDLE_THRESHOLD_SECS: i64 = 120;
/// How often the sweep checks for idle sessions.
const SWEEP_INTERVAL_SECS: u64 = 20;

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
      // Backfill cost_usd once at startup for sessions ingested before cost calculation
      // existed (their cost_usd is stuck at 0 in the DB otherwise).
      backfill_session_costs(&conn);
      app.manage(db::Db(Mutex::new(conn)));

      // Resolved once at startup (env var, then app_data_dir/config.json), never re-read —
      // see `summarize::resolve_api_key`'s doc comment for the exact order and why this is
      // Tauri-managed state rather than a `OnceLock`.
      let api_key = summarize::resolve_api_key(app.handle());
      if api_key.is_none() {
        summarize::log_summarization_disabled_once(
          "no Anthropic API key found (set ANTHROPIC_API_KEY, or add \"api_key\" to app_data_dir/config.json)",
        );
      }
      app.manage(summarize::ApiKeyState(api_key));
      app.manage(summarize::InFlight(Mutex::new(std::collections::HashSet::new())));
      app.manage(summarize::HttpClient(reqwest::Client::new()));
      app.manage(activity::ActivityCache::new());

      watcher::start(app.handle().clone());
      spawn_idle_sweep(app.handle().clone());

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::list_projects,
      commands::list_sessions,
      commands::get_session_detail,
      commands::open_in_editor,
      commands::project_activity,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

/// Recomputes cost_usd for every session from its currently-stored token totals against the
/// bundled pricing table. Runs once at startup, synchronously, before the DB is handed off as
/// managed state — cheap enough at this scale that an equality check before writing isn't
/// worth the complexity.
fn backfill_session_costs(conn: &rusqlite::Connection) {
  let totals = match db::queries::all_session_token_totals(conn) {
    Ok(totals) => totals,
    Err(e) => {
      log::warn!("failed to load session token totals for cost backfill: {e:#}");
      return;
    }
  };

  for t in totals {
    let cost = cost::pricing::cost_usd(
      t.model.as_deref(),
      t.prompt_tokens,
      t.completion_tokens,
      t.cache_read_tokens,
      t.cache_creation_tokens,
    );
    if let Err(e) = db::queries::update_cost(conn, &t.id, cost) {
      log::warn!("failed to backfill cost for session {}: {e:#}", t.id);
    }
  }
}

/// Ticks every `SWEEP_INTERVAL_SECS`, finalizing any session that has gone idle for longer
/// than `IDLE_THRESHOLD_SECS`. Runs on Tauri's async runtime (not a raw OS thread, unlike the
/// watcher's blocking `notify` loop — this is a plain async interval loop).
fn spawn_idle_sweep(app_handle: tauri::AppHandle) {
  tauri::async_runtime::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(SWEEP_INTERVAL_SECS));
    loop {
      interval.tick().await;

      let db = app_handle.state::<db::Db>();
      let conn = db.0.lock().unwrap();
      let now = chrono::Utc::now().timestamp();

      let ids = match db::queries::sessions_to_finalize(&conn, IDLE_THRESHOLD_SECS, now) {
        Ok(ids) => ids,
        Err(e) => {
          log::warn!("idle sweep: failed to query sessions_to_finalize: {e:#}");
          continue;
        }
      };

      let mut any_finalized = false;
      for id in &ids {
        match db::queries::finalize_session(&conn, id) {
          Ok(()) => any_finalized = true,
          Err(e) => log::warn!("idle sweep: failed to finalize session {id}: {e:#}"),
        }
      }

      // Tag classification runs after finalize in this same tick, since `sessions_needing_tags`
      // only returns 'ended' sessions — a session finalized above becomes eligible immediately.
      // This also naturally backfills tags for any session finalized before this feature
      // existed, same pattern as `backfill_session_costs` at startup.
      let mut any_tagged = false;
      match db::queries::sessions_needing_tags(&conn) {
        Ok(tag_ids) => {
          for id in &tag_ids {
            if tag_session(&conn, id) {
              any_tagged = true;
            }
          }
        }
        Err(e) => log::warn!("idle sweep: failed to query sessions_needing_tags: {e:#}"),
      }

      // Summary generation runs after tag classification in this same tick, for the same
      // reason `tag_session` above does: `sessions_needing_summary` only returns 'ended'
      // sessions, so a session finalized earlier in this tick is eligible immediately. Unlike
      // finalize/tag (synchronous, DB-only), this step's work is a real network call, so each
      // eligible session's summarization runs as its own spawned task rather than inline here
      // — see `spawn_summary_tasks` for why, and for the DB-lock discipline that requires.
      // Its tasks emit their own `data-changed` events on success (they complete well after
      // this tick's synchronous work and its emit below), so they don't contribute to
      // `any_finalized`/`any_tagged` here.
      spawn_summary_tasks(&app_handle, &conn);

      drop(conn);

      if any_finalized || any_tagged {
        let _ = app_handle.emit(
          "data-changed",
          serde_json::json!({ "entity": "session", "kind": "updated" }),
        );
      }
    }
  });
}

/// Computes and stores tags for one session: reads its raw log, extracts the first user
/// prompt's text, classifies it, and writes the result back. On any failure along the way
/// (missing raw_log_path row, unreadable/missing log file, no first_user_text at all) still
/// writes an empty tag list (`"[]"`) rather than leaving `tags` `NULL` — otherwise a session
/// with an unreadable log would be retried, and logged about, on every single sweep tick
/// forever. Returns whether a write actually happened (it always does, barring a DB error
/// on the `update_tags` call itself) so the caller knows whether to emit `data-changed`.
fn tag_session(conn: &rusqlite::Connection, session_id: &str) -> bool {
  let raw_log_path = match db::queries::session_raw_log_path(conn, session_id) {
    Ok(Some(path)) => path,
    Ok(None) => {
      log::warn!("idle sweep: session {session_id} has no raw_log_path row, tagging as empty");
      String::new()
    }
    Err(e) => {
      log::warn!("idle sweep: failed to look up raw_log_path for session {session_id}: {e:#}");
      String::new()
    }
  };

  let first_user_text = if raw_log_path.is_empty() {
    None
  } else {
    match parser::extract_excerpts(&raw_log_path) {
      Ok(excerpts) => excerpts.first_user_text,
      Err(e) => {
        log::warn!(
          "idle sweep: failed to extract transcript excerpts for session {session_id} at {raw_log_path}: {e:#}"
        );
        None
      }
    }
  };

  let tags = tags::classify(&first_user_text.unwrap_or_default());
  let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

  match db::queries::update_tags(conn, session_id, &tags_json) {
    Ok(()) => true,
    Err(e) => {
      log::warn!("idle sweep: failed to write tags for session {session_id}: {e:#}");
      false
    }
  }
}

/// Looks at every session with `status='ended' AND summary IS NULL`, skips any session
/// already being summarized from a prior tick (the in-flight set — `summarize::InFlight`),
/// and spawns one task per remaining session to build its prompt, call the Anthropic API, and
/// write the result back on success.
///
/// Lock discipline: this function itself only ever holds `conn` (passed in already locked by
/// the caller) for the synchronous `sessions_needing_summary` query below — it never awaits
/// anything, so holding the caller's lock through this call is fine. Each *spawned* task,
/// however, re-acquires the DB lock independently via its own `app_handle.state::<db::Db>()`
/// call, only for its own brief synchronous parts (building the prompt, and later writing the
/// result) — never across the `.await` on `summarize::call_anthropic_api` in between. Holding
/// a `MutexGuard<Connection>` across that `.await` would be the bug this function exists to
/// avoid: a slow/hanging network call would hold the single shared DB connection hostage,
/// blocking every other DB access (including this same sweep's next tick, and any UI command)
/// for as long as the request takes.
fn spawn_summary_tasks(app_handle: &tauri::AppHandle, conn: &rusqlite::Connection) {
  let api_key = app_handle.state::<summarize::ApiKeyState>().0.clone();
  let Some(api_key) = api_key else {
    // Already logged once at startup (see `setup()`) — nothing further to log here, and
    // logging again per-tick is exactly what `log_summarization_disabled_once` prevents.
    return;
  };

  let Some(model) = cost::pricing::haiku_model_id() else {
    summarize::log_summarization_disabled_once(
      "bundled pricing.json has no model entry containing \"haiku\"",
    );
    return;
  };

  let ids = match db::queries::sessions_needing_summary(conn) {
    Ok(ids) => ids,
    Err(e) => {
      log::warn!("idle sweep: failed to query sessions_needing_summary: {e:#}");
      return;
    }
  };

  if ids.is_empty() {
    return;
  }

  let to_spawn: Vec<String> = {
    let in_flight = app_handle.state::<summarize::InFlight>();
    let mut set = in_flight.0.lock().unwrap();
    ids.into_iter().filter(|id| set.insert(id.clone())).collect()
  };

  if to_spawn.is_empty() {
    return; // every eligible session is already being summarized from an earlier tick
  }

  let client = app_handle.state::<summarize::HttpClient>().0.clone();

  for session_id in to_spawn {
    let app_handle = app_handle.clone();
    let api_key = api_key.clone();
    let model = model.to_string();
    let client = client.clone();

    tauri::async_runtime::spawn(async move {
      // Removes `session_id` from the in-flight set when this async block ends, on every
      // exit path — normal completion below, or any of the early `return`s on error/skip
      // branches. See `InFlightGuard`'s doc comment.
      let _guard = summarize::InFlightGuard::new(app_handle.clone(), session_id.clone());

      let prompt_result = {
        let db = app_handle.state::<db::Db>();
        let conn = db.0.lock().unwrap();
        // `conn` (the MutexGuard) is dropped at the end of this block, before the `.await`
        // below — never held across it.
        summarize::prompts::prompt_for_session(&conn, &session_id)
      };

      let prompt = match prompt_result {
        Ok(Some(prompt)) => prompt,
        Ok(None) => {
          log::debug!(
            "idle sweep: session {session_id} has nothing to summarize (no captured user text); leaving summary NULL"
          );
          return;
        }
        Err(e) => {
          log::warn!("idle sweep: failed to build summary prompt for session {session_id}: {e:#}");
          return;
        }
      };

      match summarize::call_anthropic_api(&client, &api_key, &model, prompt).await {
        Ok(summary) => {
          let write_result = {
            let db = app_handle.state::<db::Db>();
            let conn = db.0.lock().unwrap();
            db::queries::update_summary(&conn, &session_id, &summary)
          };
          match write_result {
            Ok(()) => {
              let _ = app_handle.emit(
                "data-changed",
                serde_json::json!({ "entity": "session", "kind": "updated" }),
              );
            }
            Err(e) => {
              log::warn!("idle sweep: failed to write summary for session {session_id}: {e:#}")
            }
          }
        }
        Err(e) => {
          log::warn!("idle sweep: summarization API call failed for session {session_id}: {e:#}");
        }
      }
      // `_guard` drops here (or at whichever `return` above fired), removing session_id from
      // the in-flight set either way.
    });
  }
}
