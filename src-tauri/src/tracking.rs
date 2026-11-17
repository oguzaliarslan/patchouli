//! Media detection and automatic list updates.
//!
//! A background thread polls the players (Taiga `track/media.cpp`), and a
//! one-second tick drives the update session (`track/update_session.cpp`).
//! Updates go through the synchronizer like manual edits, so they're queued
//! and sent to the service.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tracker_anitomy::{Element, ElementKind};
use tracker_core::{Anime, ListEntry};
use tracker_db::{anime, list};
use tracker_detect::{Player, PlayerType};
use tracker_recognition::scanner::is_inside_library_folders;
use tracker_settings::Settings;
use tracker_track::media::{pick, resolve};
use tracker_track::session::{Current, Library};
use tracker_track::{
    Effect, UpdateSession, UpdateSettings, UpdateState, UpdateTrigger, entry_with_episode_watched,
    today,
};

use crate::AppState;
use crate::error::CommandResult;

pub const NOW_PLAYING_EVENT: &str = "now-playing";
pub const UPDATE_STATE_EVENT: &str = "update-state";

/// What's playing, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub(crate) player: String,
    pub(crate) browser: bool,
    pub(crate) file: Option<String>,
    pub(crate) title: String,
    /// Episode numbers after redirection (`["12"]`, `["1", "2"]`).
    pub(crate) episodes: Vec<String>,
    pub(crate) anime_id: Option<i32>,
    pub(crate) elements: Vec<Element>,
}

pub struct Tracker {
    players: Vec<Player>,
    session: Mutex<UpdateSession>,
    now_playing: Mutex<Option<NowPlaying>>,
    /// The player window being followed.
    window: Mutex<Option<isize>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn update_settings(settings: &Settings) -> UpdateSettings {
    UpdateSettings {
        delay_secs: settings.update_delay_secs,
        trigger: match settings.update_trigger {
            tracker_settings::UpdateTrigger::AfterDelay => UpdateTrigger::AfterDelay,
            tracker_settings::UpdateTrigger::OnPlayerClose => UpdateTrigger::OnPlayerClose,
        },
        pause_when_unfocused: settings.update_pause_when_unfocused,
    }
}

impl Tracker {
    pub fn new(players: Vec<Player>, settings: &Settings) -> Self {
        Self {
            players,
            session: Mutex::new(UpdateSession::new(update_settings(settings))),
            now_playing: Mutex::new(None),
            window: Mutex::new(None),
        }
    }

    pub fn now_playing(&self) -> Option<NowPlaying> {
        lock(&self.now_playing).clone()
    }

    /// Players to look for: not disabled, and browsers only with streaming on.
    fn enabled_players(&self, settings: &Settings) -> Vec<Player> {
        self.players
            .iter()
            .filter(|p| p.r#type != PlayerType::WebBrowser || settings.streaming_detection_enabled)
            .filter(|p| {
                !settings
                    .disabled_players
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(&p.name))
            })
            .cloned()
            .collect()
    }
}

/// List data for the update session.
struct DbLibrary<'a>(&'a AppState);

impl Library for DbLibrary<'_> {
    fn anime(&self, id: i32) -> Option<Anime> {
        let db = self.0.db.lock().ok()?;
        anime::get(&db, id).ok().flatten()
    }

    fn entry(&self, id: i32) -> Option<ListEntry> {
        let db = self.0.db.lock().ok()?;
        list::get(&db, id).ok().flatten()
    }
}

/// Starts polling players and ticking the update session.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::Builder::new()
        .name("media-detection".into())
        .spawn(move || {
            loop {
                let interval = {
                    let state = handle.state::<AppState>();
                    let settings = state.settings.get();
                    if settings.detection_enabled {
                        poll(&handle, &state, &settings);
                    } else {
                        set_now_playing(&handle, &state, None);
                    }
                    settings.detection_interval_ms.max(1000)
                };
                std::thread::sleep(Duration::from_millis(u64::from(interval)));
            }
        })
        .expect("spawn detection thread");

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let state = handle.state::<AppState>();
            let window = *lock(&state.tracker.window);
            let focused = window.is_some() && tracker_detect::foreground_window() == window;
            let effects = {
                let mut session = lock(&state.tracker.session);
                if !session.is_ticking() {
                    continue;
                }
                session.tick(focused, &DbLibrary(&state))
            };
            handle_effects(&handle, &state, effects);
        }
    });
}

fn poll(app: &AppHandle, state: &AppState, settings: &Settings) {
    let players = state.tracker.enabled_players(settings);
    let mut detections = tracker_detect::detect(&players);
    // Linux and macOS find players without the player list.
    detections.retain(|d| !settings.disabled_players.contains(&d.player));
    let previous = *lock(&state.tracker.window);
    let folders = &settings.library_folders;
    let found = pick(&detections, previous).and_then(|d| resolve(d, folders).map(|r| (d, r)));
    let Some((detection, resolved)) = found else {
        *lock(&state.tracker.window) = None;
        set_now_playing(app, state, None);
        return;
    };
    *lock(&state.tracker.window) = Some(detection.window);

    let mut episode = resolved.episode;
    let anime_id = state
        .recognition
        .get(state)
        .ok()
        .and_then(|r| r.identify(&mut episode));
    let outside_library = settings.update_library_only
        && !folders.is_empty()
        && resolved
            .file
            .as_deref()
            .is_some_and(|f| !is_inside_library_folders(std::path::Path::new(f), folders));
    let now_playing = NowPlaying {
        player: detection.player.clone(),
        browser: detection.player_type == PlayerType::WebBrowser,
        file: resolved.file,
        title: episode.title().to_owned(),
        episodes: episode
            .values(ElementKind::Episode)
            .map(str::to_owned)
            .collect(),
        anime_id,
        elements: episode.elements.clone(),
    };
    set_now_playing(
        app,
        state,
        Some((
            now_playing,
            Current {
                episode,
                anime_id,
                outside_library,
            },
        )),
    );
}

/// Updates what's playing and tells the session, if it changed (another
/// anime or episode, or identified differently).
fn set_now_playing(app: &AppHandle, state: &AppState, playing: Option<(NowPlaying, Current)>) {
    {
        let mut now = lock(&state.tracker.now_playing);
        let changed = match (&*now, &playing) {
            (None, None) => false,
            (Some(a), Some((b, _))) => a.anime_id != b.anime_id || a.episodes != b.episodes,
            _ => true,
        };
        if !changed {
            return;
        }
        *now = playing.as_ref().map(|(n, _)| n.clone());
    }
    match &playing {
        Some((n, _)) => log::info!(
            "Now playing in {}: {} {:?} (anime {:?})",
            n.player,
            n.title,
            n.episodes,
            n.anime_id
        ),
        None => log::info!("Nothing playing"),
    }
    let _ = app.emit(NOW_PLAYING_EVENT, playing.as_ref().map(|(n, _)| n));
    crate::sharing::schedule_update(app);
    let effects =
        lock(&state.tracker.session).on_media_changed(playing.map(|(_, c)| c), &DbLibrary(state));
    handle_effects(app, state, effects);
}

fn handle_effects(app: &AppHandle, state: &AppState, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::StateChanged(update) => {
                let _ = app.emit(UPDATE_STATE_EVENT, update);
                crate::sharing::on_update_state(app, state, update);
            }
            Effect::Commit { anime_id, episode } => commit(app, state, anime_id, episode),
            Effect::ConfirmationRequested(update) => notify_confirmation(app, state, update),
        }
    }
}

fn commit(app: &AppHandle, state: &AppState, anime_id: i32, episode: i32) {
    let library = DbLibrary(state);
    let Some(item) = library.anime(anime_id) else {
        return;
    };
    let entry = library.entry(anime_id);
    let updated = entry_with_episode_watched(&item, entry.as_ref(), episode, today());
    log::info!("Updating list: anime {anime_id}, episode {episode}");
    if let Err(error) = state.sync.save_entry(updated) {
        log::warn!("Couldn't update the list: {error}");
        let _ = app.emit(
            crate::sync::SYNC_EVENT,
            tracker_sync::SyncEvent::Error {
                message: format!("Couldn't update the list: {error}"),
                unauthorized: false,
            },
        );
    }
}

fn notify_confirmation(app: &AppHandle, state: &AppState, update: UpdateState) {
    let title = DbLibrary(state)
        .anime(update.anime_id)
        .map(|a| a.titles.romaji)
        .unwrap_or_default();
    let _ = app
        .notification()
        .builder()
        .title("Update your list?")
        .body(format!(
            "Episode {} of {title} skips ahead of episode {}. Open Tracker to update.",
            update.episode, update.previous_episode
        ))
        .show();
}

/// Settings changed: the session uses the new delay and trigger right away.
pub fn settings_changed(app: &AppHandle, settings: &Settings) {
    let state = app.state::<AppState>();
    let effects =
        lock(&state.tracker.session).set_settings(update_settings(settings), &DbLibrary(&state));
    handle_effects(app, &state, effects);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingState {
    now_playing: Option<NowPlaying>,
    update: UpdateState,
}

#[tauri::command]
pub fn tracking_get(state: State<'_, AppState>) -> TrackingState {
    TrackingState {
        now_playing: lock(&state.tracker.now_playing).clone(),
        update: lock(&state.tracker.session).state(),
    }
}

/// Updates the list now (e.g. for an episode that skips ahead).
#[tauri::command]
pub fn update_accept(app: AppHandle, state: State<'_, AppState>) -> CommandResult<UpdateState> {
    let effects = lock(&state.tracker.session).accept(&DbLibrary(&state));
    handle_effects(&app, &state, effects);
    Ok(lock(&state.tracker.session).state())
}

/// Doesn't update the list for the episode that's playing.
#[tauri::command]
pub fn update_cancel(app: AppHandle, state: State<'_, AppState>) -> CommandResult<UpdateState> {
    let effects = lock(&state.tracker.session).cancel(&DbLibrary(&state));
    handle_effects(&app, &state, effects);
    Ok(lock(&state.tracker.session).state())
}
