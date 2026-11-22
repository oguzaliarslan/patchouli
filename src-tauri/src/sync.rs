//! Accounts, login and sync commands.

use std::sync::Arc;

use patchouli_core::{Anime, ListEntry, QueueItem, ServiceId};
use patchouli_db::queue::mirror as mirror_queue;
use patchouli_db::{anime, list, meta, queue};
use patchouli_sync::anilist::AniList;
use patchouli_sync::myanimelist::MyAnimeList;
use patchouli_sync::oauth::{OAuthConfig, PendingLogin};
use patchouli_sync::{Client, CredentialStore, Credentials, SearchParams, Synchronizer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;
use crate::error::{CommandError, CommandResult};

/// Event carrying a [`patchouli_sync::SyncEvent`].
pub const SYNC_EVENT: &str = "sync";

/// `meta` key naming the service the local list came from.
pub const META_SERVICE: &str = "service";

/// Services that can be logged in to.
const SERVICES: [ServiceId; 2] = [ServiceId::AniList, ServiceId::MyAnimeList];

pub fn make_client(
    service: ServiceId,
    credentials: Credentials,
    config: &OAuthConfig,
    store: Arc<dyn CredentialStore>,
) -> patchouli_sync::Result<Client> {
    match service {
        ServiceId::AniList => Ok(Client::AniList(AniList::new(credentials.access_token))),
        ServiceId::MyAnimeList => Ok(Client::MyAnimeList(MyAnimeList::new(
            config.client_id(service)?,
            credentials,
            Some(store),
        ))),
        ServiceId::Kitsu => Err(patchouli_sync::Error::NotConfigured(
            "Kitsu is not supported yet.".into(),
        )),
    }
}

/// The service changes are copied to: the one that isn't the main list.
pub fn mirror_service(primary: ServiceId) -> ServiceId {
    match primary {
        ServiceId::MyAnimeList => ServiceId::AniList,
        _ => ServiceId::MyAnimeList,
    }
}

/// A client for `service` from its saved account and tokens.
fn saved_session(
    state: &AppState,
    service: ServiceId,
) -> Option<(Client, patchouli_sync::Account)> {
    let account = state.accounts.get(service)?;
    let credentials = state.credentials.load(service).ok()??;
    let store: Arc<dyn CredentialStore> = state.credentials.clone();
    let client = make_client(service, credentials, &state.oauth, store).ok()?;
    Some((client, account))
}

/// Restores the session for the selected service, and for the service changes
/// are copied to, if their accounts and tokens are saved.
pub fn restore_session(state: &AppState) {
    let settings = state.settings.get();
    if let Some(session) = saved_session(state, settings.sync_service) {
        state.sync.set_session(Some(session));
    }
    restore_mirror(state);
}

/// Applies the mirroring setting and logs in to the mirror service.
pub fn restore_mirror(state: &AppState) {
    let settings = state.settings.get();
    state.sync.set_mirroring(settings.sync_mirror);
    let session = settings
        .sync_mirror
        .then(|| saved_session(state, mirror_service(settings.sync_service)))
        .flatten();
    state.sync.set_mirror_session(session);
}

/// Runs a full sync in the background, reporting failure as a sync event.
pub fn spawn_synchronize(app: &AppHandle, sync: Arc<Synchronizer>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = sync.synchronize().await {
            let _ = app.emit(
                SYNC_EVENT,
                patchouli_sync::SyncEvent::Error {
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
    /// How scores are shown: AniList's `POINT_100`, `POINT_10_DECIMAL`,
    /// `POINT_10`, `POINT_5` or `POINT_3`.
    score_format: Option<String>,
    username: Option<String>,
    avatar_url: Option<String>,
    /// The user's page on the service.
    profile_url: Option<String>,
    /// The selected service, whose list is shown.
    current: bool,
    /// Logged in with a working session.
    connected: bool,
    /// Changes to the main list are copied to this service.
    mirror: bool,
    /// …and its session works.
    mirror_connected: bool,
}

fn status(state: &AppState, service: ServiceId) -> AccountStatus {
    let settings = state.settings.get();
    let current = settings.sync_service == service;
    let mirror = settings.sync_mirror && mirror_service(settings.sync_service) == service;
    let account = state.accounts.get(service);
    AccountStatus {
        service,
        name: service.name(),
        configured: state.oauth.is_configured(service),
        score_format: match service {
            // MyAnimeList scores are always 1–10.
            ServiceId::MyAnimeList => Some("POINT_10".into()),
            _ => account.as_ref().and_then(|a| a.score_format.clone()),
        },
        avatar_url: account.as_ref().and_then(|a| a.avatar_url.clone()),
        profile_url: account.as_ref().map(|a| match service {
            ServiceId::MyAnimeList => format!("https://myanimelist.net/profile/{}", a.username),
            _ => format!("https://anilist.co/user/{}/", a.username),
        }),
        username: account.map(|a| a.username),
        current,
        connected: current
            && state
                .sync
                .session()
                .is_some_and(|(client, _)| client.service() == service),
        mirror,
        mirror_connected: mirror
            && state
                .sync
                .mirror_session()
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
///
/// With `mirror`, logs in to the service that isn't the main list and starts
/// copying changes to it; the local list is kept.
#[tauri::command]
pub async fn account_login(
    app: AppHandle,
    state: State<'_, AppState>,
    service: ServiceId,
    replace_local: bool,
    mirror: Option<bool>,
) -> CommandResult<AccountStatus> {
    let _login = state
        .login
        .try_lock()
        .map_err(|_| CommandError::new("A login is already in progress."))?;

    if mirror.unwrap_or(false) {
        if state.settings.get().sync_service == service {
            return Err(CommandError::new(format!(
                "{} is your main list. Changes are copied to the other service.",
                service.name()
            )));
        }
        let login = PendingLogin::start(&state.oauth, service).await?;
        app.opener()
            .open_url(login.authorize_url(), None::<&str>)
            .map_err(|e| CommandError::new(format!("Couldn't open the browser: {e}")))?;
        let credentials = login.finish().await?;
        let store: Arc<dyn CredentialStore> = state.credentials.clone();
        let client = make_client(service, credentials.clone(), &state.oauth, store)?;
        let account = client.user().await?;
        state.credentials.save(service, &credentials)?;
        state.accounts.set(service, account)?;
        state.settings.update(|s| s.sync_mirror = true)?;
        restore_mirror(&state);
        return Ok(status(&state, service));
    }

    let owner = {
        let db = state.db.lock()?;
        let owner = meta::get(&db, META_SERVICE)?.and_then(|s| ServiceId::from_slug(&s));
        let replaces = owner.is_some_and(|o| o != service) && patchouli_db::has_list_data(&db)?;
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
            patchouli_db::clear_service_data(&db)?;
        }
        meta::set(&db, META_SERVICE, service.slug())?;
    }
    {
        // IDs in the mirror queue belong to the old main service.
        let db = state.db.lock()?;
        mirror_queue::clear(&db)?;
    }
    state.settings.update(|s| s.sync_service = service)?;
    state.sync.set_session(Some((client, account)));
    restore_mirror(&state);
    spawn_synchronize(&app, state.sync.clone());
    Ok(status(&state, service))
}

/// Forgets the tokens and account. The local list is kept. Logging out of the
/// service changes are copied to stops copying.
#[tauri::command]
pub fn account_logout(
    state: State<'_, AppState>,
    service: ServiceId,
) -> CommandResult<AccountStatus> {
    state.credentials.delete(service)?;
    state.accounts.remove(service)?;
    let settings = state.settings.get();
    if settings.sync_mirror && mirror_service(settings.sync_service) == service {
        state.settings.update(|s| s.sync_mirror = false)?;
        let db = state.db.lock()?;
        mirror_queue::clear(&db)?;
    }
    if state
        .sync
        .mirror_session()
        .is_some_and(|(client, _)| client.service() == service)
    {
        state.sync.set_mirror_session(None);
    }
    if state
        .sync
        .session()
        .is_some_and(|(client, _)| client.service() == service)
    {
        state.sync.set_session(None);
    }
    Ok(status(&state, service))
}

/// Fetches the account again (name, picture, score format).
#[tauri::command]
pub async fn profile_refresh(state: State<'_, AppState>) -> CommandResult<AccountStatus> {
    let (client, _) = state
        .sync
        .session()
        .ok_or(patchouli_sync::Error::NotLoggedIn)?;
    let service = client.service();
    state.accounts.set(service, client.user().await?)?;
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
    /// Custom title from per-anime settings.
    display_title: Option<String>,
    /// Has changes not yet sent to the service.
    queued: bool,
}

/// List entries with their anime, excluding entries waiting to be deleted.
#[tauri::command]
pub fn list_get(state: State<'_, AppState>) -> CommandResult<Vec<ListItem>> {
    let db = state.db.lock()?;
    let mut anime: std::collections::HashMap<i32, Anime> =
        anime::all(&db)?.into_iter().map(|a| (a.id, a)).collect();
    let mut titles = crate::library::display_titles(&db)?;
    let queued: std::collections::HashSet<i32> =
        queue::all(&db)?.iter().map(|q| q.anime_id).collect();
    Ok(list::all(&db)?
        .into_iter()
        .filter(|e| !e.pending_delete)
        .map(|entry| ListItem {
            anime: anime.remove(&entry.anime_id),
            display_title: titles.remove(&entry.anime_id),
            queued: queued.contains(&entry.anime_id),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorFailure {
    anime_id: i32,
    title: String,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorStatus {
    enabled: bool,
    service: ServiceId,
    name: &'static str,
    connected: bool,
    /// Changes waiting to be copied, including failed ones.
    pending: usize,
    failed: Vec<MirrorFailure>,
}

#[tauri::command]
pub fn mirror_status(state: State<'_, AppState>) -> CommandResult<MirrorStatus> {
    let settings = state.settings.get();
    let service = mirror_service(settings.sync_service);
    let db = state.db.lock()?;
    let items = mirror_queue::all(&db)?;
    let mut titles = crate::library::display_titles(&db)?;
    let failed = items
        .iter()
        .filter_map(|item| {
            let error = item.last_error.clone()?;
            let title = titles.remove(&item.anime_id).unwrap_or_else(|| {
                anime::get(&db, item.anime_id).ok().flatten().map_or_else(
                    || format!("#{}", item.anime_id),
                    |a| crate::sharing::preferred_title(&a, &state),
                )
            });
            Some(MirrorFailure {
                anime_id: item.anime_id,
                title,
                error,
            })
        })
        .collect();
    Ok(MirrorStatus {
        enabled: settings.sync_mirror,
        service,
        name: service.name(),
        connected: state
            .sync
            .mirror_session()
            .is_some_and(|(client, _)| client.service() == service),
        pending: items.len(),
        failed,
    })
}

/// Queues the whole list to be copied. Returns how many entries.
#[tauri::command]
pub fn mirror_copy_all(state: State<'_, AppState>) -> CommandResult<usize> {
    Ok(state.sync.mirror_all()?)
}

/// Forgets changes that couldn't be copied.
#[tauri::command]
pub fn mirror_dismiss_failed(state: State<'_, AppState>) -> CommandResult<usize> {
    Ok(state.sync.mirror_dismiss_failed()?)
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
        .ok_or(patchouli_sync::Error::NotLoggedIn)?;
    let results = client.search(&params).await?;
    let db = state.db.lock()?;
    anime::upsert_many(&db, &results)?;
    Ok(results)
}
