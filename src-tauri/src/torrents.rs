//! Torrent feeds: checking, filtering and downloading (Taiga v1's Torrents
//! page, `track/feed_aggregator.cpp`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use patchouli_core::feed::Preset;
use patchouli_core::{Anime, AnimeSettings, ListEntry};
use patchouli_db::{anime_settings, list};
use patchouli_feed::download::{self, SortBy};
use patchouli_feed::{Archive, Feed, FeedItem, Library, State as ItemState};
use patchouli_recognition::Recognizer;
use patchouli_settings::{Settings, TorrentApp, TorrentNewAction, TorrentSortBy};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::AppState;
use crate::error::{CommandError, CommandResult};

/// Event carrying the examined [`Feed`].
pub const TORRENTS_EVENT: &str = "torrents";
pub const ARCHIVE_FILE: &str = "feed_archive.json";

pub struct Torrents {
    /// The last feed as fetched, before filtering, so filters can be rerun.
    raw: Mutex<Option<Feed>>,
    /// The last feed after filtering.
    feed: Mutex<Option<Feed>>,
    archive: Mutex<Archive>,
    last_check: Mutex<Option<Instant>>,
    busy: tokio::sync::Mutex<()>,
}

impl Torrents {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            raw: Mutex::new(None),
            feed: Mutex::new(None),
            archive: Mutex::new(Archive::load(data_dir.join(ARCHIVE_FILE))),
            last_check: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The list and library, as filters see them.
struct AppLibrary {
    recognizer: Arc<Recognizer>,
    entries: HashMap<i32, ListEntry>,
    available: Arc<crate::available::EpisodeMap>,
}

impl Library for AppLibrary {
    fn anime(&self, id: i32) -> Option<&Anime> {
        self.recognizer.anime(id)
    }
    fn entry(&self, id: i32) -> Option<&ListEntry> {
        self.entries.get(&id).filter(|e| !e.pending_delete)
    }
    fn is_episode_available(&self, id: i32, episode: i32) -> bool {
        self.available
            .get(&id)
            .is_some_and(|e| e.contains(&episode))
    }
}

/// Runs recognition and the filters on a copy of the fetched feed.
async fn examine(app: &AppHandle, state: &AppState, raw: &Feed) -> CommandResult<Feed> {
    let recognizer = state.recognition.get(state)?;
    let entries = {
        let db = state.db.lock()?;
        list::all(&db)?
            .into_iter()
            .map(|e| (e.anime_id, e))
            .collect()
    };
    let settings = state.settings.get();
    // Only scan the library when a filter needs it.
    let needs_library = settings.torrent_filters_enabled
        && settings.torrent_filters.iter().any(|f| {
            f.enabled
                && f.conditions
                    .iter()
                    .any(|c| c.element == patchouli_core::feed::Element::LocalEpisodeAvailable)
        });
    // The library index is filled a few seconds after start; scan now if
    // a filter needs it before then.
    if needs_library
        && !settings.library_folders.is_empty()
        && state.available.last_scan().is_none()
    {
        crate::available::rescan(app).await?;
    }
    let available = state.available.episodes();
    let library = AppLibrary {
        recognizer: Arc::clone(&recognizer),
        entries,
        available,
    };
    let mut feed = raw.clone();
    {
        let archive = lock(&state.torrents.archive);
        let filters = settings
            .torrent_filters_enabled
            .then_some(settings.torrent_filters.as_slice());
        patchouli_feed::examine(&mut feed, &recognizer, &library, filters, &archive);
    }
    *lock(&state.torrents.feed) = Some(feed.clone());
    let _ = app.emit(TORRENTS_EVENT, &feed);
    Ok(feed)
}

async fn fetch(url: &str) -> CommandResult<Feed> {
    let response = patchouli_sync::http::client()
        .get(url)
        .header(reqwest::header::ACCEPT, "application/rss+xml, */*")
        .send()
        .await
        .map_err(|e| CommandError::new(format!("Couldn't check {}: {e}", host(url))))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CommandError::new(format!(
            "{} returned an error ({status}).",
            host(url)
        )));
    }
    let text = response.text().await?;
    Feed::parse(url, &text).map_err(|e| CommandError::new(format!("{}: {e}", host(url))))
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// Checks a feed: `url`, or the feed in settings.
async fn check(app: &AppHandle, state: &AppState, url: Option<String>) -> CommandResult<Feed> {
    let _busy = state.torrents.busy.lock().await;
    let url = url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| state.settings.get().torrent_source);
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(CommandError::new(
            "Enter an http:// or https:// feed address.",
        ));
    }
    let raw = fetch(url.trim()).await?;
    *lock(&state.torrents.last_check) = Some(Instant::now());
    *lock(&state.torrents.raw) = Some(raw.clone());
    let feed = examine(app, state, &raw).await?;
    log::info!(
        "Checked {}: {} items, {} selected",
        host(&url),
        feed.items.len(),
        feed.items
            .iter()
            .filter(|i| i.state == ItemState::Selected)
            .count()
    );
    Ok(feed)
}

#[tauri::command]
pub fn torrents_get(state: State<'_, AppState>) -> Option<Feed> {
    lock(&state.torrents.feed).clone()
}

#[tauri::command]
pub async fn torrents_check(
    app: AppHandle,
    state: State<'_, AppState>,
    url: Option<String>,
) -> CommandResult<Feed> {
    check(&app, &state, url).await
}

/// Percent-encodes a query value.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Searches with the search address in settings (`%title%` replaced).
#[tauri::command]
pub async fn torrents_search(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> CommandResult<Feed> {
    let template = state.settings.get().torrent_search_url;
    let url = template.replace("%title%", &encode(text.trim()));
    check(&app, &state, Some(url)).await
}

/// Runs the filters again, after filters or the list changed.
pub fn refilter(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let raw = lock(&state.torrents.raw).clone();
        if let Some(raw) = raw
            && let Err(e) = examine(&app, &state, &raw).await
        {
            log::warn!("Torrent filters: {}", e.message);
        }
    });
}

#[tauri::command]
pub fn torrents_refilter(app: AppHandle) {
    refilter(&app);
}

/// Marks an item selected or discarded by hand.
#[tauri::command]
pub fn torrents_set_state(
    app: AppHandle,
    state: State<'_, AppState>,
    link: String,
    selected: bool,
) -> Option<Feed> {
    let mut guard = lock(&state.torrents.feed);
    let feed = guard.as_mut()?;
    for item in feed.items.iter_mut().filter(|i| i.link == link) {
        item.state = if selected {
            ItemState::Selected
        } else {
            ItemState::DiscardedNormal
        };
    }
    let _ = app.emit(TORRENTS_EVENT, &*feed);
    Some(feed.clone())
}

#[tauri::command]
pub fn torrents_presets() -> Vec<Preset> {
    patchouli_core::feed::presets()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    count: usize,
}

#[tauri::command]
pub fn torrents_archive_info(state: State<'_, AppState>) -> ArchiveInfo {
    ArchiveInfo {
        count: lock(&state.torrents.archive).len(),
    }
}

/// Forgets downloaded items, so they can be selected again.
#[tauri::command]
pub fn torrents_archive_clear(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    let max = state.settings.get().torrent_archive_max_count as usize;
    {
        let mut archive = lock(&state.torrents.archive);
        archive.clear();
        archive.save(max)?;
    }
    refilter(&app);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    downloaded: Vec<String>,
    errors: Vec<String>,
}

/// Where the client should save an anime: its folder, else (if allowed) the
/// fallback folder, in a subfolder named after it.
fn download_folder(state: &AppState, settings: &Settings, item: &FeedItem) -> Option<String> {
    if !settings.torrent_use_anime_folder {
        return None;
    }
    let id = item.anime_id?;
    let db = state.db.lock().ok()?;
    let existing = anime_settings::get(&db, id).ok().flatten();
    if let Some(folder) = existing.as_ref().map(|s| s.folder.clone())
        && !folder.is_empty()
        && Path::new(&folder).is_dir()
    {
        return Some(folder);
    }
    if !settings.torrent_fallback_to_folder || settings.torrent_download_folder.is_empty() {
        return None;
    }
    let mut path = PathBuf::from(&settings.torrent_download_folder);
    if settings.torrent_create_subfolder {
        let title = patchouli_db::anime::get(&db, id)
            .ok()
            .flatten()
            .map_or_else(|| item.episode.title.clone(), |a| a.titles.romaji);
        path.push(download::file_name(&title));
        if std::fs::create_dir_all(&path).is_ok() {
            // Remember it, as Taiga does.
            let mut saved = existing.unwrap_or(AnimeSettings {
                id,
                ..AnimeSettings::default()
            });
            saved.folder = path.display().to_string();
            let _ = anime_settings::upsert(&db, &saved);
        }
    }
    Some(path.display().to_string())
}

/// Opens a torrent file or magnet link with the configured client.
fn open(
    app: &AppHandle,
    settings: &Settings,
    target: &str,
    folder: Option<&str>,
) -> Result<(), String> {
    if !settings.torrent_app_open {
        return Ok(());
    }
    let custom = (settings.torrent_app == TorrentApp::Custom)
        .then_some(settings.torrent_app_path.trim())
        .filter(|p| !p.is_empty());
    let Some(client) = custom else {
        let result = if target.starts_with("magnet:") {
            app.opener().open_url(target, None::<&str>)
        } else {
            app.opener().open_path(target, None::<&str>)
        };
        return result.map_err(|e| format!("Couldn't open {target}: {e}"));
    };
    let (args, hidden) = download::client_arguments(client, target, folder);
    let mut command = std::process::Command::new(client);
    command.args(&args);
    #[cfg(windows)]
    if hidden {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = hidden;
    command
        .spawn()
        .map(drop)
        .map_err(|e| format!("Couldn't start {client}: {e}"))
}

async fn download_item(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    item: &FeedItem,
) -> Result<(), String> {
    let folder = download_folder(state, settings, item);
    if download::use_magnet(item, settings.torrent_use_magnet) {
        return open(
            app,
            settings,
            download::magnet_link(item),
            folder.as_deref(),
        );
    }
    let response = patchouli_sync::http::client()
        .get(&item.link)
        .header(reqwest::header::ACCEPT, "application/x-bittorrent, */*")
        .send()
        .await
        .map_err(|e| format!("Couldn't download \"{}\": {e}", item.title))?;
    let status = response.status().as_u16();
    let url = response.url().to_string();
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let content_type = header(reqwest::header::CONTENT_TYPE);
    let disposition = header(reqwest::header::CONTENT_DISPOSITION).is_some();
    let body = response.bytes().await.map_err(|e| e.to_string())?;
    download::validate(status, &url, content_type.as_deref(), disposition, &body)?;

    let dir = if settings.torrent_file_folder.trim().is_empty() {
        state.data_dir.join("torrents")
    } else {
        PathBuf::from(settings.torrent_file_folder.trim())
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(format!("{}.torrent", download::file_name(&item.title)));
    std::fs::write(&file, &body).map_err(|e| format!("{}: {e}", file.display()))?;
    open(
        app,
        settings,
        &file.display().to_string(),
        folder.as_deref(),
    )
}

/// Downloads the given items (by link), or every selected item.
async fn download_items(app: &AppHandle, state: &AppState, links: Vec<String>) -> DownloadResult {
    let settings = state.settings.get();
    let mut items: Vec<FeedItem> = lock(&state.torrents.feed)
        .as_ref()
        .map(|f| {
            f.items
                .iter()
                .filter(|i| {
                    if links.is_empty() {
                        i.state == ItemState::Selected
                    } else {
                        links.contains(&i.link)
                    }
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let by = match settings.torrent_sort_by {
        TorrentSortBy::EpisodeNumber => SortBy::EpisodeNumber,
        TorrentSortBy::ReleaseDate => SortBy::ReleaseDate,
    };
    items.sort_by(|a, b| download::compare(a, b, by, settings.torrent_sort_descending));

    let mut result = DownloadResult {
        downloaded: Vec::new(),
        errors: Vec::new(),
    };
    for item in &items {
        match download_item(app, state, &settings, item).await {
            Ok(()) => {
                lock(&state.torrents.archive).add(&item.title);
                result.downloaded.push(item.title.clone());
            }
            Err(e) => {
                log::warn!("Torrent download: {e}");
                result.errors.push(e);
            }
        }
    }
    if !result.downloaded.is_empty() {
        if let Err(e) =
            lock(&state.torrents.archive).save(settings.torrent_archive_max_count as usize)
        {
            log::warn!("Torrent archive: {e}");
        }
        let mut guard = lock(&state.torrents.feed);
        if let Some(feed) = guard.as_mut() {
            for item in feed
                .items
                .iter_mut()
                .filter(|i| result.downloaded.contains(&i.title))
            {
                item.state = ItemState::DiscardedNormal;
            }
            let _ = app.emit(TORRENTS_EVENT, &*feed);
        }
    }
    result
}

#[tauri::command]
pub async fn torrents_download(
    app: AppHandle,
    state: State<'_, AppState>,
    links: Vec<String>,
) -> CommandResult<DownloadResult> {
    let result = download_items(&app, &state, links).await;
    if result.downloaded.is_empty() && !result.errors.is_empty() {
        return Err(CommandError::new(result.errors.join("\n")));
    }
    Ok(result)
}

/// Checks the feed on schedule when automatic checks are on.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            let state = app.state::<AppState>();
            let settings = state.settings.get();
            if !settings.torrent_auto_check {
                continue;
            }
            let interval =
                Duration::from_secs(u64::from(settings.torrent_auto_check_interval) * 60);
            let due = lock(&state.torrents.last_check).is_none_or(|t| t.elapsed() >= interval);
            if !due {
                continue;
            }
            match check(&app, &state, None).await {
                Ok(feed) => {
                    let selected: Vec<&FeedItem> = feed
                        .items
                        .iter()
                        .filter(|i| i.state == ItemState::Selected)
                        .collect();
                    if selected.is_empty() {
                        continue;
                    }
                    match settings.torrent_new_action {
                        TorrentNewAction::Notify => {
                            let names: Vec<&str> = selected
                                .iter()
                                .take(5)
                                .map(|i| i.episode.title.as_str())
                                .collect();
                            let _ = app
                                .notification()
                                .builder()
                                .title(format!("{} new torrents", selected.len()))
                                .body(names.join(", "))
                                .show();
                        }
                        TorrentNewAction::Download => {
                            download_items(&app, &state, Vec::new()).await;
                        }
                    }
                }
                Err(e) => {
                    // Try again at the next interval.
                    *lock(&state.torrents.last_check) = Some(Instant::now());
                    log::warn!("Automatic torrent check: {}", e.message);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_queries() {
        assert_eq!(encode("Frieren: Beyond 2"), "Frieren%3A+Beyond+2");
        assert_eq!(host("https://nyaa.si/?page=rss"), "nyaa.si");
    }
}
