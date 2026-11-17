mod error;
mod paths;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracker_db::{anime, history, list, meta, migrations, queue};
use tracker_settings::{Settings, SettingsStore};

use crate::error::CommandResult;

/// Event emitted with the changed setting keys (`string[]`).
const SETTINGS_CHANGED: &str = "settings-changed";

/// Shared backend state. Grows into the sync manager, detector, etc.
struct AppState {
    data_dir: PathBuf,
    portable: bool,
    db: Mutex<tracker_db::Connection>,
    settings: SettingsStore,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
    data_dir: String,
    portable: bool,
    schema_version: i64,
    anime_count: i64,
    list_count: usize,
    history_count: usize,
    queue_count: usize,
}

#[tauri::command]
fn app_info(app: AppHandle, state: State<'_, AppState>) -> CommandResult<AppInfo> {
    let db = state.db.lock()?;
    let info = app.package_info();
    Ok(AppInfo {
        name: info.name.clone(),
        version: info.version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        portable: state.portable,
        schema_version: migrations::version(&db)?,
        anime_count: anime::count(&db)?,
        list_count: list::all(&db)?.len(),
        history_count: history::all(&db)?.len(),
        queue_count: queue::all(&db)?.len(),
    })
}

#[tauri::command]
fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Saves `settings` and returns the keys that changed.
#[tauri::command]
fn settings_set(state: State<'_, AppState>, settings: Settings) -> CommandResult<BTreeSet<String>> {
    Ok(state.settings.replace(settings)?)
}

fn init_state(app: &AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
    let data_dir = paths::data_dir(app)?;
    std::fs::create_dir_all(&data_dir.path)?;
    let version = app.package_info().version.to_string();

    let settings = SettingsStore::load(data_dir.path.join(tracker_settings::FILE_NAME))?;
    settings.update(|s| s.meta_version = version.clone())?;
    let handle = app.clone();
    settings.subscribe(move |_, keys| {
        let _ = handle.emit(SETTINGS_CHANGED, keys);
    });

    let db = tracker_db::open(data_dir.path.join(tracker_db::FILE_NAME))?;
    meta::set(&db, "version", &version)?;

    Ok(AppState {
        data_dir: data_dir.path,
        portable: data_dir.portable,
        db: Mutex::new(db),
        settings,
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
            let state = init_state(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_get,
            settings_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
