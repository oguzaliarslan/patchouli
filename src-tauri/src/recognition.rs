//! Recognition commands: test a file name, play episodes, open folders and
//! scan the library.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use patchouli_core::{Anime, ServiceId};
use patchouli_db::{anime, anime_settings, list};
use patchouli_recognition::scanner::{self, LibraryScan};
use patchouli_recognition::{Episode, Identification, Recognizer, Relations, play};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;
use crate::error::{CommandError, CommandResult};

const RELATIONS_FILE: &str = "anime-relations.txt";

/// The recognizer, built from the database on first use and dropped when the
/// list, per-anime settings or the service change.
pub struct RecognitionCache {
    recognizer: Mutex<Option<Arc<Recognizer>>>,
    data_dir: PathBuf,
    resource_dir: Option<PathBuf>,
}

impl RecognitionCache {
    pub fn new(data_dir: PathBuf, resource_dir: Option<PathBuf>) -> Self {
        Self {
            recognizer: Mutex::new(None),
            data_dir,
            resource_dir,
        }
    }

    pub fn invalidate(&self) {
        *self.recognizer.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// `anime-relations.txt` from the data folder if there is one (for
    /// updated copies), else the bundled one.
    fn relations(&self, service: ServiceId) -> Relations {
        let candidates = [
            Some(self.data_dir.join(RELATIONS_FILE)),
            Some(
                self.data_dir
                    .join(crate::resources::CACHE)
                    .join(RELATIONS_FILE),
            ),
            self.resource_dir.as_ref().map(|d| d.join(RELATIONS_FILE)),
        ];
        candidates
            .into_iter()
            .flatten()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .map_or_else(
                || Relations::parse(include_str!("../resources/anime-relations.txt"), service),
                |text| Relations::parse(&text, service),
            )
    }

    pub fn get(&self, state: &AppState) -> CommandResult<Arc<Recognizer>> {
        let mut cached = self.recognizer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(recognizer) = cached.as_ref() {
            return Ok(Arc::clone(recognizer));
        }
        let (items, settings) = {
            let db = state.db.lock()?;
            (anime::all(&db)?, anime_settings::all(&db)?)
        };
        let relations = self.relations(state.settings.get().sync_service);
        let recognizer = Arc::new(Recognizer::new(items, settings, relations));
        *cached = Some(Arc::clone(&recognizer));
        Ok(recognizer)
    }
}

fn library_folders(state: &AppState) -> Vec<String> {
    state.settings.get().library_folders
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionTest {
    /// Elements Anitomy found in the input, before redirection.
    elements: Vec<patchouli_anitomy::Element>,
    /// The title came from parent folders.
    title_from_path: bool,
    identification: Identification,
    /// Titles of the candidates and the result, by ID.
    titles: HashMap<i32, String>,
}

/// Parses and identifies a file name or full path, explaining the result.
#[tauri::command]
pub fn recognition_test(
    state: State<'_, AppState>,
    input: String,
) -> CommandResult<RecognitionTest> {
    let recognizer = state.recognition.get(&state)?;
    let input = input.trim().trim_matches('"');
    let is_path = input.contains(['/', '\\']);
    let episode = if is_path {
        patchouli_recognition::parse_file(Path::new(input), &library_folders(&state))
    } else {
        Episode::parse(input)
    };
    let parsed = Episode::parse(
        Path::new(input)
            .file_name()
            .map_or(input.into(), |n| n.to_string_lossy())
            .as_ref(),
    );
    let title_from_path = is_path && parsed.title().is_empty() && !episode.title().is_empty();
    let identification = recognizer.explain(episode);
    let ids = identification
        .candidates
        .iter()
        .map(|c| c.id)
        .chain(identification.anime_id);
    let titles = ids
        .filter_map(|id| recognizer.anime(id).map(|a| (id, a.titles.romaji.clone())))
        .collect();
    Ok(RecognitionTest {
        elements: parsed.elements,
        title_from_path,
        identification,
        titles,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayResult {
    episode: i32,
    path: String,
}

/// Finds an episode in the library folders and opens it in the default
/// player: `episode` if given, else a random one if `random`, else the next
/// unwatched one.
#[tauri::command]
pub async fn anime_play(
    app: AppHandle,
    state: State<'_, AppState>,
    anime_id: i32,
    episode: Option<i32>,
    random: bool,
) -> CommandResult<PlayResult> {
    let folders = library_folders(&state);
    if folders.is_empty() {
        return Err(CommandError::with_code(
            "no-library-folders",
            "Add your anime folders in Settings › Library first.",
        ));
    }
    let recognizer = state.recognition.get(&state)?;
    let entry = {
        let db = state.db.lock()?;
        list::get(&db, anime_id)?
    };
    let Some(item) = recognizer.anime(anime_id).cloned() else {
        return Err(CommandError::new("This anime isn't in the database."));
    };
    let number = match episode {
        Some(n) => n,
        None if random => play::random_episode(&item, entry.as_ref())
            .ok_or_else(|| CommandError::new("There's no episode to pick from."))?,
        None => play::next_episode(&item, entry.as_ref()),
    };
    let search = Arc::clone(&recognizer);
    let found = tauri::async_runtime::spawn_blocking(move || {
        folders.iter().find_map(|folder| {
            scanner::find_episode(Path::new(folder), anime_id, number, &search, &folders)
        })
    })
    .await?;
    let Some(path) = found else {
        return Err(CommandError::new(format!(
            "Couldn't find episode {number} of {} in your library folders.",
            item.titles.romaji
        )));
    };
    let path = path.display().to_string();
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| CommandError::new(format!("Couldn't open the file: {e}")))?;
    Ok(PlayResult {
        episode: number,
        path,
    })
}

/// Opens the anime's folder: the one set in its settings, else the first
/// folder in the library recognised as this anime.
#[tauri::command]
pub async fn anime_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    anime_id: i32,
) -> CommandResult<String> {
    let configured = {
        let db = state.db.lock()?;
        anime_settings::get(&db, anime_id)?
            .map(|s| s.folder)
            .filter(|f| !f.is_empty() && Path::new(f).is_dir())
    };
    let folder = match configured {
        Some(folder) => folder,
        None => {
            let folders = library_folders(&state);
            let recognizer = state.recognition.get(&state)?;
            tauri::async_runtime::spawn_blocking(move || {
                folders
                    .iter()
                    .find_map(|f| scanner::find_folder(Path::new(f), anime_id, &recognizer))
            })
            .await?
            .map(|p| p.display().to_string())
            .ok_or_else(|| {
                CommandError::new("Couldn't find a folder for this anime in your library folders.")
            })?
        }
    };
    app.opener()
        .open_path(&folder, None::<&str>)
        .map_err(|e| CommandError::new(format!("Couldn't open the folder: {e}")))?;
    Ok(folder)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    scan: LibraryScan,
    anime: HashMap<i32, Anime>,
}

/// Recognises every video file in the library folders.
#[tauri::command]
pub async fn library_scan(state: State<'_, AppState>) -> CommandResult<LibraryView> {
    let folders = library_folders(&state);
    let recognizer = state.recognition.get(&state)?;
    let search = Arc::clone(&recognizer);
    let scan =
        tauri::async_runtime::spawn_blocking(move || scanner::scan_library(&folders, &search))
            .await?;
    let anime = scan
        .anime
        .iter()
        .filter_map(|a| {
            recognizer
                .anime(a.anime_id)
                .map(|item| (a.anime_id, item.clone()))
        })
        .collect();
    Ok(LibraryView { scan, anime })
}
