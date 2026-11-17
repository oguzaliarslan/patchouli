//! Commands for anime details, per-anime settings, history and the player list.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracker_core::{Anime, AnimeSettings, HistoryItem, ListEntry, QueueItem};
use tracker_db::{anime, anime_settings, history, list, queue};

use crate::AppState;
use crate::error::{CommandError, CommandResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimeDetails {
    anime: Option<Anime>,
    entry: Option<ListEntry>,
    settings: Option<AnimeSettings>,
    queued: Option<QueueItem>,
    /// Page on the current service.
    page_url: Option<String>,
}

fn details(state: &AppState, id: i32) -> CommandResult<AnimeDetails> {
    let db = state.db.lock()?;
    let service = state.settings.get().sync_service;
    Ok(AnimeDetails {
        anime: anime::get(&db, id)?,
        entry: list::get(&db, id)?.filter(|e| !e.pending_delete),
        settings: anime_settings::get(&db, id)?,
        queued: queue::get(&db, id)?,
        page_url: tracker_sync::anime_page_url(service, id).ok(),
    })
}

#[tauri::command]
pub fn anime_get(state: State<'_, AppState>, id: i32) -> CommandResult<AnimeDetails> {
    details(&state, id)
}

/// Fetches the anime from the service again. If the service no longer has it,
/// it's removed locally unless it has unsent changes (as in Taiga).
#[tauri::command]
pub async fn anime_refresh(state: State<'_, AppState>, id: i32) -> CommandResult<AnimeDetails> {
    let (client, _) = state
        .sync
        .session()
        .ok_or(tracker_sync::Error::NotLoggedIn)?;
    let fetched = client.fetch_anime(id).await?;
    {
        let db = state.db.lock()?;
        match &fetched {
            Some(item) => anime::upsert(&db, item)?,
            None if queue::get(&db, id)?.is_none() => {
                list::delete(&db, id)?;
                anime::delete(&db, id)?;
            }
            None => {}
        }
    }
    details(&state, id)
}

#[tauri::command]
pub fn anime_settings_save(
    state: State<'_, AppState>,
    settings: AnimeSettings,
) -> CommandResult<()> {
    let db = state.db.lock()?;
    let empty = settings.display_title.is_empty()
        && settings.folder.is_empty()
        && settings.synonyms.is_empty();
    if empty {
        anime_settings::delete(&db, settings.id)?;
    } else {
        anime_settings::upsert(&db, &settings)?;
    }
    // Alternative titles are recognition titles.
    state.recognition.invalidate();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    /// Newest first.
    items: Vec<HistoryItem>,
    /// The anime the items refer to, by ID.
    anime: HashMap<i32, Anime>,
    display_titles: HashMap<i32, String>,
}

#[tauri::command]
pub fn history_get(state: State<'_, AppState>) -> CommandResult<HistoryView> {
    let db = state.db.lock()?;
    let mut items = history::all(&db)?;
    items.reverse();
    let mut anime = HashMap::new();
    for item in &items {
        if let std::collections::hash_map::Entry::Vacant(slot) = anime.entry(item.anime_id)
            && let Some(found) = anime::get(&db, item.anime_id)?
        {
            slot.insert(found);
        }
    }
    let display_titles = display_titles(&db)?;
    Ok(HistoryView {
        items,
        anime,
        display_titles,
    })
}

#[tauri::command]
pub fn history_remove(state: State<'_, AppState>, id: i64) -> CommandResult<bool> {
    let db = state.db.lock()?;
    Ok(history::remove(&db, id)?)
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> CommandResult<()> {
    let db = state.db.lock()?;
    Ok(history::clear(&db)?)
}

/// Custom display titles set in per-anime settings.
pub fn display_titles(db: &tracker_db::Connection) -> tracker_db::Result<HashMap<i32, String>> {
    Ok(anime_settings::all(db)?
        .into_iter()
        .filter(|s| !s.display_title.is_empty())
        .map(|s| (s.id, s.display_title))
        .collect())
}

/// Names of the media players in `players.anisthesia`, sorted.
#[tauri::command]
pub fn players_get(app: AppHandle) -> CommandResult<Vec<String>> {
    let path = app.path().resolve(
        "resources/players.anisthesia",
        tauri::path::BaseDirectory::Resource,
    )?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| CommandError::new(format!("Couldn't read the player list: {e}")))?;
    Ok(player_names(&text))
}

/// Player names are the unindented, non-comment lines.
fn player_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with(['\t', ' ', '#']))
        .map(|line| line.trim().to_owned())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_player_names() {
        let text = "# comment\n\nmpv\n\twindows:\n\t\tmpv\nMPC-HC\n\tstrategies:\n";
        assert_eq!(player_names(text), ["MPC-HC", "mpv"]);
    }

    #[test]
    fn bundled_player_list_parses() {
        let text = include_str!("../resources/players.anisthesia");
        let names = player_names(text);
        assert!(names.len() > 40, "{}", names.len());
        assert!(names.iter().any(|n| n == "mpv"));
    }
}
