//! Importing data from Taiga v1 or v2.

use std::path::PathBuf;

use patchouli_compat::{Found, ImportData, TaigaVersion};
use patchouli_core::ServiceId;
use patchouli_db::{anime, anime_settings, history, list, meta, queue};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::error::{CommandError, CommandResult};
use crate::sync::{META_SERVICE, SYNC_EVENT};

/// `meta` key: `done` after an import, `dismissed` when the user declined.
const META_TAIGA_IMPORT: &str = "taiga_import";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// Taiga data folders found in their default locations.
    found: Vec<Found>,
    /// Ask on start: Taiga data was found, the local list is empty and the
    /// user hasn't imported or declined yet.
    prompt: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    version: TaigaVersion,
    path: String,
    service: Option<ServiceId>,
    service_name: Option<&'static str>,
    username: Option<String>,
    anime: usize,
    entries: usize,
    history: usize,
    queue: usize,
    settings: usize,
    warnings: Vec<String>,
}

fn summary(found: &Found, data: &ImportData) -> Summary {
    Summary {
        version: found.version,
        path: found.path.display().to_string(),
        service: data.service,
        service_name: data.service.map(ServiceId::name),
        username: data.username.clone(),
        anime: data.anime.len(),
        entries: data.entries.iter().filter(|e| !e.pending_delete).count(),
        history: data.history.len(),
        queue: data.queue.len(),
        settings: data.settings.len(),
        warnings: data.warnings.clone(),
    }
}

fn locate(path: Option<String>) -> CommandResult<Found> {
    match path {
        Some(path) => patchouli_compat::locate(&PathBuf::from(&path))
            .ok_or_else(|| CommandError::new(format!("No Taiga data was found in \"{path}\"."))),
        None => patchouli_compat::default_locations()
            .into_iter()
            .next()
            .ok_or_else(|| CommandError::new("No Taiga data was found.")),
    }
}

#[tauri::command]
pub fn taiga_detect(state: State<'_, AppState>) -> CommandResult<Detection> {
    let found = patchouli_compat::default_locations();
    let db = state.db.lock()?;
    let prompt = !found.is_empty()
        && meta::get(&db, META_TAIGA_IMPORT)?.is_none()
        && !patchouli_db::has_list_data(&db)?;
    Ok(Detection { found, prompt })
}

/// Reads Taiga's data without importing it. `path` is a data folder or the
/// folder of a portable Taiga; `None` uses the default location.
#[tauri::command]
pub async fn taiga_preview(path: Option<String>) -> CommandResult<Summary> {
    tauri::async_runtime::spawn_blocking(move || {
        let found = locate(path)?;
        let data = patchouli_compat::read(&found).map_err(CommandError::new)?;
        Ok(summary(&found, &data))
    })
    .await
    .map_err(|e| CommandError::new(e.to_string()))?
}

/// Replaces the local list, history and unsent changes with Taiga's, and
/// merges its settings. Fails with code `replace-local-data` if there is a
/// local list, unless `replace_local` is set.
#[tauri::command]
pub async fn taiga_import(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
    replace_local: bool,
) -> CommandResult<Summary> {
    let (found, data) = tauri::async_runtime::spawn_blocking(move || {
        let found = locate(path)?;
        let data = patchouli_compat::read(&found).map_err(CommandError::new)?;
        Ok::<_, CommandError>((found, data))
    })
    .await
    .map_err(|e| CommandError::new(e.to_string()))??;
    let service = data
        .service
        .ok_or_else(|| CommandError::new("Taiga's service is unknown."))?;
    if matches!(service, ServiceId::Kitsu) {
        return Err(CommandError::new(
            "Taiga was using Kitsu, which isn't supported yet.",
        ));
    }

    {
        let db = state.db.lock()?;
        if patchouli_db::has_list_data(&db)? && !replace_local {
            return Err(CommandError::with_code(
                "replace-local-data",
                "Importing from Taiga replaces your local list, \
                 including history and unsent changes.",
            ));
        }
        write(&db, service, &data)?;
    }

    // Log out of a session for another service; its list is gone.
    if state
        .sync
        .session()
        .is_some_and(|(client, _)| client.service() != service)
    {
        state.sync.set_session(None);
    }
    state.settings.merge(&data.settings)?;
    state.settings.update(|s| s.sync_service = service)?;
    if state.sync.session().is_none() {
        crate::sync::restore_session(&state);
    }
    state.recognition.invalidate();
    crate::sharing::schedule_update(&app);
    let _ = app.emit(SYNC_EVENT, patchouli_sync::SyncEvent::ListChanged);
    let _ = app.emit(SYNC_EVENT, patchouli_sync::SyncEvent::QueueChanged);
    log::info!(
        "Imported from Taiga {:?} at {}: {} entries, {} history items, {} queued",
        found.version,
        found.path.display(),
        data.entries.len(),
        data.history.len(),
        data.queue.len()
    );
    Ok(summary(&found, &data))
}

fn write(
    db: &patchouli_db::Connection,
    service: ServiceId,
    data: &ImportData,
) -> patchouli_db::Result<()> {
    patchouli_db::in_transaction(db, |tx| {
        patchouli_db::clear_service_data(tx)?;
        anime::upsert_many(tx, &data.anime)?;
        list::upsert_many(tx, &data.entries)?;
        for settings in &data.anime_settings {
            anime_settings::upsert(tx, settings)?;
        }
        for item in &data.history {
            history::add(tx, item.anime_id, item.episode, item.time)?;
        }
        for item in &data.queue {
            queue::upsert(tx, item)?;
        }
        meta::set(tx, META_SERVICE, service.slug())?;
        meta::set(tx, META_TAIGA_IMPORT, "done")?;
        Ok(())
    })
}

/// Stops asking to import on start.
#[tauri::command]
pub fn taiga_dismiss(state: State<'_, AppState>) -> CommandResult<()> {
    let db = state.db.lock()?;
    meta::set(&db, META_TAIGA_IMPORT, "dismissed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use patchouli_core::{Fields, HistoryItem, ListEntry, QueueItem};

    use super::*;

    #[test]
    fn write_replaces_service_data_and_keeps_times() {
        let db = patchouli_db::open_in_memory().unwrap();
        list::upsert(
            &db,
            &ListEntry {
                anime_id: 1,
                ..ListEntry::default()
            },
        )
        .unwrap();
        history::add(&db, 1, 1, 100).unwrap();

        let data = ImportData {
            anime: vec![patchouli_core::Anime {
                id: 21,
                ..Default::default()
            }],
            service: Some(ServiceId::MyAnimeList),
            entries: vec![ListEntry {
                anime_id: 21,
                watched_episodes: 5,
                ..ListEntry::default()
            }],
            history: vec![HistoryItem {
                id: 0,
                anime_id: 21,
                episode: 5,
                time: 1_714_000_000,
            }],
            queue: vec![QueueItem {
                anime_id: 21,
                dirty: Fields::EPISODE,
                time: 1,
                retry_count: 0,
                last_error: None,
            }],
            ..ImportData::default()
        };
        write(&db, ServiceId::MyAnimeList, &data).unwrap();

        let entries = list::all(&db).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].anime_id, 21);
        assert_eq!(anime::count(&db).unwrap(), 1);
        let history = history::all(&db).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].time, 1_714_000_000);
        assert_eq!(queue::all(&db).unwrap()[0].dirty, Fields::EPISODE);
        assert_eq!(
            meta::get(&db, META_SERVICE).unwrap().as_deref(),
            Some("myanimelist")
        );
        assert_eq!(
            meta::get(&db, META_TAIGA_IMPORT).unwrap().as_deref(),
            Some("done")
        );
    }
}
