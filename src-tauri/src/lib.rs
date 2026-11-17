use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

/// Shared backend state. Grows into settings, sync manager, detector, etc.
struct AppState {
    db: Mutex<tracker_db::Connection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
    data_dir: String,
    tables: Vec<String>,
}

#[tauri::command]
fn app_info(app: AppHandle, state: State<'_, AppState>) -> Result<AppInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let tables = tracker_db::table_names(&db).map_err(|e| e.to_string())?;
    let info = app.package_info();
    Ok(AppInfo {
        name: info.name.clone(),
        version: info.version.to_string(),
        data_dir: data_dir.display().to_string(),
        tables,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch focuses the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db = tracker_db::open(data_dir.join("tracker.db"))?;
            app.manage(AppState { db: Mutex::new(db) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
