mod available;
mod credentials;
mod error;
mod export;
mod library;
mod logging;
mod paths;
mod recognition;
mod resources;
mod sharing;
mod stats;
mod sync;
mod taiga;
mod torrents;
mod tracking;
mod update;
mod window;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use patchouli_db::{anime, history, list, meta, migrations, queue};
use patchouli_settings::{Settings, SettingsStore};
use patchouli_sync::oauth::OAuthConfig;
use patchouli_sync::{AccountsFile, Synchronizer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_window_state::StateFlags;

use crate::credentials::KeyringStore;
use crate::error::CommandResult;
use crate::recognition::RecognitionCache;
use crate::sharing::Sharing;
use crate::tracking::Tracker;

/// Event emitted with the changed setting keys (`string[]`).
const SETTINGS_CHANGED: &str = "settings-changed";

/// Shared backend state.
pub(crate) struct AppState {
    data_dir: PathBuf,
    portable: bool,
    db: Arc<Mutex<patchouli_db::Connection>>,
    settings: Arc<SettingsStore>,
    accounts: AccountsFile,
    credentials: Arc<KeyringStore>,
    oauth: OAuthConfig,
    sync: Arc<Synchronizer>,
    recognition: Arc<RecognitionCache>,
    tracker: Tracker,
    sharing: Sharing,
    torrents: torrents::Torrents,
    available: available::Available,
    updates: update::Updates,
    /// Held while a login is in progress.
    login: tokio::sync::Mutex<()>,
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

    let recognition = Arc::new(RecognitionCache::new(
        data_dir.path.clone(),
        app.path().resource_dir().ok().map(|d| d.join("resources")),
    ));

    let settings = SettingsStore::load(data_dir.path.join(patchouli_settings::FILE_NAME))?;
    settings.update(|s| s.meta_version = version.clone())?;
    let handle = app.clone();
    let cache = Arc::clone(&recognition);
    settings.subscribe(move |new, keys| {
        // Relation rules depend on the service's IDs.
        if keys.contains("sync.service") {
            cache.invalidate();
        }
        if keys
            .iter()
            .any(|k| k.starts_with("sharing.") || k == "app.titleLanguage")
        {
            sharing::schedule_update(&handle);
        }
        if keys.iter().any(|k| k.starts_with("track.update."))
            && handle.try_state::<AppState>().is_some()
        {
            tracking::settings_changed(&handle, new);
        }
        if keys.iter().any(|k| k.starts_with("network.proxy.")) {
            apply_proxy(new);
            // The sessions hold clients made with the old proxy.
            if let Some(state) = handle.try_state::<AppState>() {
                sync::restore_session(&state);
            }
        }
        if keys.contains("app.startWithWindows") {
            window::apply_autostart(&handle, new.app_start_with_windows);
        }
        if handle.try_state::<AppState>().is_some() {
            if keys.contains("library.folders") || keys.contains("library.watchFolders") {
                available::rewatch(&handle);
            }
            if keys.contains("library.folders") || keys.contains("library.minFileSize") {
                available::schedule(&handle);
            }
        }
        if keys.iter().any(|k| k.starts_with("torrents.filter"))
            && handle.try_state::<AppState>().is_some()
        {
            torrents::refilter(&handle);
        }
        let _ = handle.emit(SETTINGS_CHANGED, keys);
    });

    apply_proxy(&settings.get());

    let players = resources::players(&data_dir.path);
    let tracker = Tracker::new(players, &settings.get());
    let (sharing, discord) = Sharing::new();
    if let Some(discord) = discord {
        tauri::async_runtime::spawn(discord);
    }

    let db = patchouli_db::open(data_dir.path.join(patchouli_db::FILE_NAME))?;
    meta::set(&db, "version", &version)?;
    let db = Arc::new(Mutex::new(db));

    let handle = app.clone();
    let cache = Arc::clone(&recognition);
    let sync = Synchronizer::new(Arc::clone(&db), move |event| {
        if event == patchouli_sync::SyncEvent::ListChanged {
            cache.invalidate();
            sharing::schedule_update(&handle);
            if handle.try_state::<AppState>().is_some() {
                torrents::refilter(&handle);
                // New anime in the list can match more files.
                available::schedule(&handle);
            }
        }
        if let patchouli_sync::SyncEvent::Error { message, .. } = &event {
            log::warn!("Sync: {message}");
        }
        let _ = handle.emit(sync::SYNC_EVENT, event);
    });

    let torrents = torrents::Torrents::new(&data_dir.path);
    Ok(AppState {
        accounts: AccountsFile::load(data_dir.path.join(patchouli_sync::accounts::FILE_NAME)),
        credentials: Arc::new(KeyringStore::new(app.config().identifier.clone())),
        oauth: OAuthConfig::from_env(),
        data_dir: data_dir.path,
        portable: data_dir.portable,
        db,
        settings: Arc::new(settings),
        sync: Arc::new(sync),
        recognition,
        tracker,
        sharing,
        torrents,
        available: available::Available::default(),
        updates: update::Updates::default(),
        login: tokio::sync::Mutex::new(()),
    })
}

/// Uses the proxy from settings for every request; without one, the system
/// proxy. The web view (posters) always uses the system proxy.
fn apply_proxy(settings: &Settings) {
    use patchouli_sync::http::{ProxyConfig, ProxyKind};
    let host = settings.proxy_host.trim();
    let proxy = (!host.is_empty()).then(|| ProxyConfig {
        kind: match settings.proxy_type {
            patchouli_settings::ProxyType::Http => ProxyKind::Http,
            patchouli_settings::ProxyType::Socks5 => ProxyKind::Socks5,
        },
        host: host.to_owned(),
        port: u16::try_from(settings.proxy_port).ok().filter(|&p| p > 0),
        username: settings.proxy_username.clone(),
        password: settings.proxy_password.clone(),
    });
    match proxy.as_ref().and_then(ProxyConfig::url) {
        Some(url) => log::info!("Using proxy {url}"),
        None if !host.is_empty() => log::warn!("Ignoring invalid proxy host \"{host}\""),
        None => {}
    }
    patchouli_sync::http::set_proxy(proxy);
}

/// Restores the login, syncs once, and keeps sending queued changes.
fn start_sync(app: &AppHandle, state: &AppState) {
    sync::restore_session(state);
    let settings = Arc::clone(&state.settings);
    if state.sync.is_logged_in() && settings.get().sync_enabled {
        sync::spawn_synchronize(app, Arc::clone(&state.sync));
    }
    let background = Arc::clone(&state.sync);
    tauri::async_runtime::spawn(background.run_background(move || settings.get().sync_enabled));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch focuses the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([window::AUTOSTART_ARG])
                .build(),
        )
        // Visibility is decided by "start in tray", not restored.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .setup(|app| {
            logging::install_panic_hook(paths::data_dir(app.handle())?.path.join("logs"));
            logging::init(app.handle())?;
            let state = init_state(app.handle())?;
            start_sync(app.handle(), &state);
            window::create_tray(app.handle())?;
            window::show_on_start(app.handle(), &state);
            window::apply_autostart(app.handle(), state.settings.get().app_start_with_windows);
            app.manage(state);
            tracking::start(app.handle());
            torrents::start(app.handle());
            available::start(app.handle());
            update::start(app.handle());
            resources::start(app.handle());
            Ok(())
        })
        .on_window_event(window::on_window_event)
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_get,
            settings_set,
            sync::accounts_get,
            sync::account_login,
            sync::account_logout,
            sync::sync_now,
            sync::list_get,
            sync::list_save_entry,
            sync::list_remove_entry,
            sync::queue_get,
            sync::anime_search,
            library::anime_get,
            library::anime_refresh,
            library::anime_settings_save,
            library::history_get,
            library::history_remove,
            library::history_clear,
            library::players_get,
            recognition::recognition_test,
            recognition::anime_play,
            recognition::anime_open_folder,
            recognition::library_scan,
            tracking::tracking_get,
            tracking::update_accept,
            tracking::update_cancel,
            sharing::sharing_status,
            export::list_export,
            stats::stats_get,
            stats::season_get,
            sync::profile_refresh,
            taiga::taiga_detect,
            taiga::taiga_preview,
            taiga::taiga_import,
            taiga::taiga_dismiss,
            torrents::torrents_get,
            torrents::torrents_check,
            torrents::torrents_search,
            torrents::torrents_refilter,
            torrents::torrents_set_state,
            torrents::torrents_presets,
            torrents::torrents_archive_info,
            torrents::torrents_archive_clear,
            torrents::torrents_download,
            available::library_available,
            update::update_status,
            update::update_check,
            update::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
