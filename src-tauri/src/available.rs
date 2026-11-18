//! Episodes available in the library folders (Taiga v1's "available
//! episodes": `ScanAvailableEpisodes`, the 30-minute library timer and the
//! folder monitor in `track/monitor.cpp`).
//!
//! The library is scanned a few seconds after start, every 30 minutes, and
//! shortly after files change in a watched folder. Every scan replaces the
//! shared result, which the list, the Library page and torrent filters read.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use patchouli_recognition::Recognizer;
use patchouli_recognition::scanner::{self, LibraryScan};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;
use crate::error::CommandResult;

/// Event carrying the available episodes (`{ [animeId]: number[] }`).
pub const AVAILABLE_EVENT: &str = "library-available";
const PERIOD: Duration = Duration::from_secs(30 * 60);
/// Changes come in bursts (copying a season); wait for them to settle.
const DEBOUNCE: Duration = Duration::from_secs(5);

pub type EpisodeMap = HashMap<i32, BTreeSet<i32>>;

#[derive(Default)]
pub struct Available {
    scan: RwLock<Option<Arc<LibraryScan>>>,
    episodes: RwLock<Arc<EpisodeMap>>,
    wake: tokio::sync::Notify,
    watcher: Mutex<Option<RecommendedWatcher>>,
    scanning: tokio::sync::Mutex<()>,
}

impl Available {
    pub fn episodes(&self) -> Arc<EpisodeMap> {
        Arc::clone(&self.episodes.read().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn last_scan(&self) -> Option<Arc<LibraryScan>> {
        self.scan.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Episode numbers per anime. A file without a number counts as episode 1
/// of a single-episode anime (a movie); for others it's a batch whose
/// episodes aren't known.
pub fn episode_map(scan: &LibraryScan, recognizer: &Recognizer) -> EpisodeMap {
    let mut map = EpisodeMap::new();
    for anime in &scan.anime {
        let single = recognizer
            .anime(anime.anime_id)
            .is_some_and(|a| a.episode_count == 1);
        let episodes: BTreeSet<i32> = anime
            .files
            .iter()
            .flat_map(|f| {
                if f.episodes.is_empty() && single {
                    vec![1]
                } else {
                    f.episodes.clone()
                }
            })
            .filter(|&e| e > 0)
            .collect();
        if !episodes.is_empty() {
            map.insert(anime.anime_id, episodes);
        }
    }
    map
}

fn as_json(map: &EpisodeMap) -> HashMap<i32, Vec<i32>> {
    map.iter()
        .map(|(id, eps)| (*id, eps.iter().copied().collect()))
        .collect()
}

/// Scans now and publishes the result.
pub async fn rescan(app: &AppHandle) -> CommandResult<Arc<LibraryScan>> {
    let state = app.state::<AppState>();
    let _scanning = state.available.scanning.lock().await;
    let settings = state.settings.get();
    let recognizer = state.recognition.get(&state)?;
    let folders = settings.library_folders.clone();
    let min_size = u64::from(settings.library_min_file_size_mb) * 1024 * 1024;
    let search = Arc::clone(&recognizer);
    let scan = tauri::async_runtime::spawn_blocking(move || {
        scanner::scan_library(&folders, &search, min_size)
    })
    .await?;
    let map = episode_map(&scan, &recognizer);
    log::debug!(
        "Library: {} files, {} anime with episodes",
        scan.files_scanned,
        map.len()
    );
    let scan = Arc::new(scan);
    *state
        .available
        .scan
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&scan));
    let _ = app.emit(AVAILABLE_EVENT, as_json(&map));
    *state
        .available
        .episodes
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Arc::new(map);
    Ok(scan)
}

/// Asks for a scan soon (after the debounce).
pub fn schedule(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.available.wake.notify_one();
    }
}

/// Watches the library folders, or stops watching, per settings.
pub fn rewatch(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings.get();
    let mut slot = state
        .available
        .watcher
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *slot = None;
    if !settings.library_watch_folders || settings.library_folders.is_empty() {
        return;
    }
    let handle = app.clone();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = &event {
            log::trace!("Library folder event: {:?} {:?}", event.kind, event.paths);
        }
        if let Ok(event) = event
            && matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(notify::event::ModifyKind::Name(_))
            )
        {
            schedule(&handle);
        }
    });
    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Can't watch the library folders: {e}");
            return;
        }
    };
    for folder in &settings.library_folders {
        match watcher.watch(std::path::Path::new(folder), RecursiveMode::Recursive) {
            Ok(()) => log::debug!("Watching {folder}"),
            Err(e) => log::warn!("Can't watch {folder}: {e}"),
        }
    }
    *slot = Some(watcher);
}

pub fn start(app: &AppHandle) {
    rewatch(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the window and the first sync start first.
        tokio::time::sleep(DEBOUNCE).await;
        loop {
            if !app
                .state::<AppState>()
                .settings
                .get()
                .library_folders
                .is_empty()
                && let Err(e) = rescan(&app).await
            {
                log::warn!("Library scan: {}", e.message);
            }
            let wake = &app.state::<AppState>().available.wake;
            tokio::select! {
                () = wake.notified() => tokio::time::sleep(DEBOUNCE).await,
                () = tokio::time::sleep(PERIOD) => {}
            }
        }
    });
}

#[tauri::command]
pub fn library_available(state: State<'_, AppState>) -> HashMap<i32, Vec<i32>> {
    as_json(&state.available.episodes())
}

#[cfg(test)]
mod tests {
    use patchouli_core::Anime;
    use patchouli_recognition::Relations;
    use patchouli_recognition::scanner::{LibraryAnime, LibraryFile};

    use super::*;

    #[test]
    fn maps_files_to_episodes() {
        let anime = |id, episode_count| Anime {
            id,
            episode_count,
            ..Anime::default()
        };
        let recognizer = Recognizer::new(
            vec![anime(1, 12), anime(2, 1), anime(3, 24)],
            vec![],
            Relations::default(),
        );
        let file = |episodes: &[i32]| LibraryFile {
            path: String::new(),
            episodes: episodes.to_vec(),
        };
        let scan = LibraryScan {
            anime: vec![
                LibraryAnime {
                    anime_id: 1,
                    files: vec![file(&[1]), file(&[2, 3]), file(&[3])],
                },
                // A movie file has no number.
                LibraryAnime {
                    anime_id: 2,
                    files: vec![file(&[])],
                },
                // A batch folder of a series: its episodes aren't known.
                LibraryAnime {
                    anime_id: 3,
                    files: vec![file(&[])],
                },
            ],
            ..LibraryScan::default()
        };
        let map = episode_map(&scan, &recognizer);
        assert_eq!(map[&1].iter().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(map[&2].iter().copied().collect::<Vec<_>>(), [1]);
        assert!(!map.contains_key(&3));
    }
}
