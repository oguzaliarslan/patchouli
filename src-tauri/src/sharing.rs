//! Sharing what's playing: Discord Rich Presence and the webhook (Taiga
//! `share/discord.cpp` and `share/http.cpp`).
//!
//! Players can report changes in quick succession, so updates wait 1.5 s for
//! things to settle. Anime whose list entry is private are never shared.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use patchouli_core::{Anime, ListEntry, ListStatus, ServiceId};
use patchouli_db::{anime, anime_settings, list};
use patchouli_recognition::Episode;
use patchouli_share::discord::{
    Activity, ActivityType, Button, Presence, PresenceStatus, StatusDisplayType,
};
use patchouli_share::webhook::{self, Event, EventAnime, EventEntry, EventType};
use patchouli_track::{Phase, UpdateState, watched_episode_number};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

const DEBOUNCE: Duration = Duration::from_millis(1500);

/// A file's parsed elements (kind, value): the same file if equal.
type FileSignature = Vec<(String, String)>;

/// Discord application ID, from `TRACKER_DISCORD_APP_ID` at run or build
/// time. Taiga's own ID would show "Taiga" on profiles, so it isn't used.
pub fn discord_app_id() -> Option<String> {
    std::env::var("TRACKER_DISCORD_APP_ID")
        .ok()
        .or(option_env!("TRACKER_DISCORD_APP_ID").map(str::to_owned))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

pub struct Sharing {
    presence: Option<Presence>,
    /// The last `playing` event sent: anime and episode.
    playing: Mutex<Option<(i32, i32)>>,
    /// The file being shown on Discord, and since when.
    discord_since: Mutex<Option<(FileSignature, i64)>>,
    generation: AtomicU64,
    webhook_error: Mutex<Option<String>>,
}

impl Sharing {
    /// Also returns the Discord connection task, for the caller to spawn.
    pub fn new() -> (Self, Option<impl std::future::Future<Output = ()> + Send>) {
        let (presence, task) = match discord_app_id() {
            Some(id) => {
                let (presence, task) = Presence::new(id, "discord-ipc-".into());
                (Some(presence), Some(task))
            }
            None => (None, None),
        };
        let sharing = Self {
            presence,
            playing: Mutex::new(None),
            discord_since: Mutex::new(None),
            generation: AtomicU64::new(0),
            webhook_error: Mutex::new(None),
        };
        (sharing, task)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The anime and its entry, unless it's unknown or the entry is private.
fn shareable(state: &AppState, anime_id: i32) -> Option<(Anime, Option<ListEntry>, String)> {
    let db = state.db.lock().ok()?;
    let item = anime::get(&db, anime_id).ok()??;
    let entry = list::get(&db, anime_id).ok().flatten();
    if entry.as_ref().is_some_and(|e| e.is_private) {
        return None;
    }
    let display = anime_settings::get(&db, anime_id)
        .ok()
        .flatten()
        .map(|s| s.display_title)
        .filter(|t| !t.is_empty());
    let title = display.unwrap_or_else(|| preferred_title(&item, state));
    Some((item, entry, title))
}

pub fn preferred_title(item: &Anime, state: &AppState) -> String {
    use patchouli_core::TitleLanguage;
    let t = &item.titles;
    match state.settings.get().app_title_language {
        TitleLanguage::English if !t.english.is_empty() => t.english.clone(),
        TitleLanguage::Native if !t.japanese.is_empty() => t.japanese.clone(),
        _ => t.romaji.clone(),
    }
}

fn episode_text(episode: &Episode, episode_count: i32) -> String {
    let Some((first, last)) = episode.episode_range() else {
        return String::new();
    };
    let number = if first == last {
        first.to_string()
    } else {
        format!("{first}-{last}")
    };
    if episode_count > 0 {
        format!("Episode {number}/{episode_count}")
    } else {
        format!("Episode {number}")
    }
}

/// Runs [`update`] once things have been quiet for [`DEBOUNCE`].
pub fn schedule_update(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let generation = state.sharing.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DEBOUNCE).await;
        let state = app.state::<AppState>();
        if state.sharing.generation.load(Ordering::SeqCst) == generation {
            update(&app, &state);
        }
    });
}

fn update(app: &AppHandle, state: &AppState) {
    let settings = state.settings.get();
    let service = settings.sync_service;
    let playing = state.tracker.now_playing();
    let current = playing
        .as_ref()
        .and_then(|p| p.anime_id.map(|id| (p, id)))
        .and_then(|(p, id)| shareable(state, id).map(|s| (p, s)));

    // Discord
    if let Some(presence) = &state.sharing.presence {
        let enabled = settings.sharing_enabled && settings.discord_enabled;
        let activity = current
            .as_ref()
            .filter(|_| enabled)
            .map(|(p, (item, _, title))| {
                let signature: FileSignature = p
                    .elements
                    .iter()
                    .map(|e| (e.kind.as_str().to_owned(), e.value.clone()))
                    .collect();
                let start = {
                    let mut since = lock(&state.sharing.discord_since);
                    match since.as_ref() {
                        // Identifying the same file differently isn't a new viewing.
                        Some((s, start)) if *s == signature => *start,
                        _ => {
                            let start = now();
                            *since = Some((signature, start));
                            start
                        }
                    }
                };
                let page = patchouli_sync::anime_page_url(service, item.id).unwrap_or_default();
                let episode = Episode {
                    elements: p.elements.clone(),
                };
                patchouli_share::discord::to_json(&Activity {
                    kind: ActivityType::Watching,
                    status_display_type: StatusDisplayType::Details, // "Watching <title>"
                    details: title.clone(),
                    details_url: page.clone(),
                    state: episode_text(&episode, item.episode_count),
                    start_time: start,
                    large_image: if item.image_url.starts_with("https://") {
                        item.image_url.clone()
                    } else {
                        "default".into()
                    },
                    large_text: title.clone(),
                    large_url: page.clone(),
                    small_image: service.slug().into(),
                    small_text: service.name().into(),
                    buttons: vec![Button {
                        label: "View Anime".into(),
                        url: page,
                    }],
                    ..Activity::default()
                })
            });
        if activity.is_none() {
            *lock(&state.sharing.discord_since) = None;
        }
        presence.set(enabled, activity);
    }

    // Webhook: `playing` once per episode, `stopped` when it ends.
    let webhook_on = settings.sharing_enabled && settings.webhook_enabled;
    let next = current
        .as_ref()
        .filter(|_| webhook_on)
        .map(|(p, (item, _, _))| {
            let episode = Episode {
                elements: p.elements.clone(),
            };
            (item.id, watched_episode_number(&episode, item).unwrap_or(0))
        });
    let previous = {
        let mut playing = lock(&state.sharing.playing);
        if *playing == next {
            return;
        }
        std::mem::replace(&mut *playing, next)
    };
    match (next, previous) {
        (Some((id, episode)), _) => send_event(app, state, EventType::Playing, id, episode),
        (None, Some((id, episode))) => send_event(app, state, EventType::Stopped, id, episode),
        (None, None) => {}
    }
}

/// The list was updated after watching: send `updated`.
pub fn on_update_state(app: &AppHandle, state: &AppState, update: UpdateState) {
    let settings = state.settings.get();
    if update.phase != Phase::Committed || !settings.sharing_enabled || !settings.webhook_enabled {
        return;
    }
    if shareable(state, update.anime_id).is_some() {
        send_event(
            app,
            state,
            EventType::Updated,
            update.anime_id,
            update.episode,
        );
    }
}

fn status_name(status: ListStatus) -> Option<&'static str> {
    Some(match status {
        ListStatus::Watching => "watching",
        ListStatus::Completed => "completed",
        ListStatus::OnHold => "onHold",
        ListStatus::Dropped => "dropped",
        ListStatus::PlanToWatch => "planToWatch",
        ListStatus::NotInList => return None,
    })
}

fn send_event(app: &AppHandle, state: &AppState, kind: EventType, anime_id: i32, episode: i32) {
    let settings = state.settings.get();
    let Some(url) = webhook::target_url(&settings.webhook_url) else {
        return;
    };
    let service: ServiceId = settings.sync_service;
    // `anime` and `entry` reflect the database now, not when the event happened.
    let (event_anime, event_entry) = match shareable(state, anime_id) {
        Some((item, entry, title)) => (
            Some(EventAnime {
                id: item.id,
                title,
                url: patchouli_sync::anime_page_url(service, item.id).unwrap_or_default(),
                episodes: item.episode_count,
                image: item.image_url.clone(),
            }),
            entry.filter(|e| !e.pending_delete).and_then(|e| {
                status_name(e.status).map(|status| EventEntry {
                    status,
                    progress: e.watched_episodes,
                    score: e.score,
                    rewatching: e.rewatching,
                })
            }),
        ),
        None => (None, None),
    };
    let event = Event {
        kind,
        timestamp: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        service: service.slug(),
        anime: event_anime,
        entry: event_entry,
        episode,
    };
    let headers = webhook::parse_headers(&settings.webhook_headers);
    let http = patchouli_sync::http::client();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = webhook::send(&http, url, &headers, &event).await;
        if let Err(error) = &result {
            log::warn!("Webhook: {error}");
        }
        *lock(&app.state::<AppState>().sharing.webhook_error) = result.err();
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingStatus {
    discord_configured: bool,
    /// `disabled`, `connecting`, `connected` or `invalidClientId`.
    discord: &'static str,
    webhook_error: Option<String>,
}

#[tauri::command]
pub fn sharing_status(state: State<'_, AppState>) -> SharingStatus {
    let discord = match state.sharing.presence.as_ref().map(Presence::status) {
        None | Some(PresenceStatus::Disabled) => "disabled",
        Some(PresenceStatus::Connecting) => "connecting",
        Some(PresenceStatus::Connected) => "connected",
        Some(PresenceStatus::InvalidClientId) => "invalidClientId",
    };
    SharingStatus {
        discord_configured: state.sharing.presence.is_some(),
        discord,
        webhook_error: lock(&state.sharing.webhook_error).clone(),
    }
}
