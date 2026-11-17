//! Accounts, login and sync commands.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tracker_core::{Anime, ListEntry, QueueItem, ServiceId};
use tracker_db::{anime, list, meta, queue};
use tracker_sync::anilist::AniList;
use tracker_sync::myanimelist::MyAnimeList;
use tracker_sync::oauth::{OAuthConfig, PendingLogin};
use tracker_sync::{Client, CredentialStore, Credentials, SearchParams, Synchronizer};

use crate::AppState;
use crate::error::{CommandError, CommandResult};

/// Event carrying a [`tracker_sync::SyncEvent`].
pub const SYNC_EVENT: &str = "sync";

/// `meta` key naming the service the local list came from.
const META_SERVICE: &str = "service";

/// Services that can be logged in to.
const SERVICES: [ServiceId; 2] = [ServiceId::AniList, ServiceId::MyAnimeList];

pub fn make_client(
    service: ServiceId,
    credentials: Credentials,
    config: &OAuthConfig,
    store: Arc<dyn CredentialStore>,
) -> tracker_sync::Result<Client> {
    match service {
        ServiceId::AniList => Ok(Client::AniList(AniList::new(credentials.access_token))),
        ServiceId::MyAnimeList => Ok(Client::MyAnimeList(MyAnimeList::new(
            config.client_id(service)?,
            credentials,
            Some(store),
        ))),
        ServiceId::Kitsu => Err(tracker_sync::Error::NotConfigured(
            "Kitsu is not supported yet.".into(),
        )),
    }
}

/// Restores the session for the selected service, if its account and tokens
/// are saved.
pub fn restore_session(state: &AppState) {
    let service = state.settings.get().sync_service;
    let Some(account) = state.accounts.get(service) else {
        return;
    };
    let Ok(Some(credentials)) = state.credentials.load(service) else {
        return;
    };
    let store: Arc<dyn CredentialStore> = state.credentials.clone();
    if let Ok(client) = make_client(service, credentials, &state.oauth, store) {
        state.sync.set_session(Some((client, account)));
    }
}

/// Runs a full sync in the background, reporting failure as a sync event.
pub fn spawn_synchronize(app: &AppHandle, sync: Arc<Synchronizer>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = sync.synchronize().await {
            let _ = app.emit(
                SYNC_EVENT,
                tracker_sync::SyncEvent::Error {
                    unauthorized: error.is_unauthorized(),
                    message: error.to_string(),
                },
            );
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    service: ServiceId,
    name: &'static str,
    /// A client ID is set for this service.
    configured: bool,
    username: Option<String>,
    /// The selected service, whose list is shown.
    current: bool,
    /// Logged in with a working session.
    connected: bool,
}

fn status(state: &AppState, service: ServiceId) -> AccountStatus {
    let current = state.settings.get().sync_service == service;
    AccountStatus {
        service,
        name: service.name(),
        configured: state.oauth.is_configured(service),
        username: state.accounts.get(service).map(|a| a.username),
        current,
        connected: current
            && state
                .sync
                .session()
                .is_some_and(|(client, _)| client.service() == service),
    }
}

#[tauri::command]
pub fn accounts_get(state: State<'_, AppState>) -> Vec<AccountStatus> {
    SERVICES.iter().map(|&s| status(&state, s)).collect()
}

/// Logs in through the browser. If the local list belongs to another service,
/// fails with code `replace-local-data` unless `replace_local` is set, in which
/// case the local list is replaced once login succeeds.
#[tauri::command]
pub async fn account_login(
    app: AppHandle,
    state: State<'_, AppState>,
    service: ServiceId,
    replace_local: bool,
) -> CommandResult<AccountStatus> {
    let _login = state
        .login
        .try_lock()
        .map_err(|_| CommandError::new("A login is already in progress."))?;

    let owner = {
        let db = state.db.lock()?;
        let owner = meta::get(&db, META_SERVICE)?.and_then(|s| ServiceId::from_slug(&s));
        let replaces = owner.is_some_and(|o| o != service) && tracker_db::has_list_data(&db)?;
        if replaces && !replace_local {
            let owner = owner.map(ServiceId::name).unwrap_or_default();
            return Err(CommandError::with_code(
                "replace-local-data",
                format!(
                    "Your local list is from {owner}. Logging in to {} replaces it, \
                     including history and unsent changes.",
                    service.name()
                ),
            ));
        }
        owner
    };

    let login = PendingLogin::start(&state.oauth, service).await?;
    app.opener()
        .open_url(login.authorize_url(), None::<&str>)
        .map_err(|e| CommandError::new(format!("Couldn't open the browser: {e}")))?;
    let credentials = login.finish().await?;

    let store: Arc<dyn CredentialStore> = state.credentials.clone();
    let client = make_client(service, credentials.clone(), &state.oauth, store)?;
    let account = client.user().await?;
    state.credentials.save(service, &credentials)?;
    state.accounts.set(service, account.clone())?;

    {
        let db = state.db.lock()?;
        if owner.is_some_and(|o| o != service) {
            tracker_db::clear_service_data(&db)?;
        }
        meta::set(&db, META_SERVICE, service.slug())?;
    }
    state.settings.update(|s| s.sync_service = service)?;
    state.sync.set_session(Some((client, account)));
    spawn_synchronize(&app, state.sync.clone());
    Ok(status(&state, service))
}

/// Forgets the tokens and account. The local list is kept.
#[tauri::command]
pub fn account_logout(
    state: State<'_, AppState>,
    service: ServiceId,
) -> CommandResult<AccountStatus> {
    state.credentials.delete(service)?;
    state.accounts.remove(service)?;
    if state
        .sync
        .session()
        .is_some_and(|(client, _)| client.service() == service)
    {
        state.sync.set_session(None);
    }
    Ok(status(&state, service))
}

/// Sends unsent changes, then fetches the list.
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> CommandResult<()> {
    Ok(state.sync.synchronize().await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    entry: ListEntry,
    anime: Option<Anime>,
}

/// List entries with their anime, excluding entries waiting to be deleted.
#[tauri::command]
pub fn list_get(state: State<'_, AppState>) -> CommandResult<Vec<ListItem>> {
    let db = state.db.lock()?;
    let anime: std::collections::HashMap<i32, Anime> =
        anime::all(&db)?.into_iter().map(|a| (a.id, a)).collect();
    Ok(list::all(&db)?
        .into_iter()
        .filter(|e| !e.pending_delete)
        .map(|entry| ListItem {
            anime: anime.get(&entry.anime_id).cloned(),
            entry,
        })
        .collect())
}

/// Saves an edited entry and queues it. Returns whether anything changed.
#[tauri::command]
pub fn list_save_entry(state: State<'_, AppState>, entry: ListEntry) -> CommandResult<bool> {
    Ok(state.sync.save_entry(entry)?)
}

#[tauri::command]
pub fn list_remove_entry(state: State<'_, AppState>, anime_id: i32) -> CommandResult<bool> {
    Ok(state.sync.remove_entry(anime_id)?)
}

#[tauri::command]
pub fn queue_get(state: State<'_, AppState>) -> CommandResult<Vec<QueueItem>> {
    let db = state.db.lock()?;
    Ok(queue::all(&db)?)
}

/// Searches the current service and stores the results.
#[tauri::command]
pub async fn anime_search(
    state: State<'_, AppState>,
    params: SearchParams,
) -> CommandResult<Vec<Anime>> {
    let (client, _) = state
        .sync
        .session()
        .ok_or(tracker_sync::Error::NotLoggedIn)?;
    let results = client.search(&params).await?;
    let db = state.db.lock()?;
    anime::upsert_many(&db, &results)?;
    Ok(results)
}
