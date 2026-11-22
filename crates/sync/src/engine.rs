//! Local list edits, the update queue and list fetching.
//!
//! Ported from Taiga `media/anime_list_utils.cpp` (saving and removing
//! entries), `sync/queue.cpp` (the queue) and `sync/service.cpp`
//! (`synchronize`, `pruneMissingEntries`).
//!
//! Local edits are saved to the database at once and queued. The queue is
//! sent oldest first, one item at a time, and stops at the first failure,
//! which is recorded on the item and retried later.
//!
//! With mirroring on, each change is also queued for the second service
//! (`mirror_queue`). The main service's list is the one shown and fetched;
//! the mirror is sent the whole entry as it is locally, under the matching
//! anime ID there (see [`patchouli_db::id_map`]).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;

use patchouli_core::{Fields, ListEntry, QueueItem, ServiceId};
use patchouli_db::queue::mirror as mirror_queue;
use patchouli_db::{Connection, anime, history, id_map, list, queue};
use serde::Serialize;
use tokio::sync::Notify;

use crate::accounts::Account;
use crate::client::{Client, SavedEntry};
use crate::{Error, Result, now};

/// Wait this long after the last edit before sending, so edits made in a row
/// go out together.
const QUEUE_DEBOUNCE: Duration = Duration::from_secs(10);
/// Retry failed items this often.
const QUEUE_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Serialised as `{ "type": "queueChanged" }`, `{ "type": "busy", "busy": true }`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncEvent {
    /// Items were added to or removed from the queue.
    QueueChanged,
    /// List entries or anime changed in the database.
    ListChanged,
    /// A sync started or finished.
    Busy { busy: bool },
    /// Background sync failed. `unauthorized` means the user must log in again.
    Error { message: String, unauthorized: bool },
}

type Listener = Box<dyn Fn(SyncEvent) + Send + Sync>;

#[derive(Default)]
struct Processing {
    anime_id: Option<i32>,
    /// The entry being sent was edited again before the response arrived.
    modified: bool,
}

type Session = (Arc<Client>, Account);

#[derive(Default)]
struct Mirror {
    /// Changes are queued for the mirror.
    enabled: bool,
    /// Logged in to the mirror service.
    session: Option<Session>,
}

pub struct Synchronizer {
    db: Arc<Mutex<Connection>>,
    session: RwLock<Option<Session>>,
    mirror: RwLock<Mirror>,
    processing: Mutex<Processing>,
    mirror_processing: Mutex<Processing>,
    /// Only one sync (queue or fetch) runs at a time.
    running: tokio::sync::Mutex<()>,
    wake: Notify,
    listener: Listener,
}

impl Synchronizer {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        listener: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            db,
            session: RwLock::new(None),
            mirror: RwLock::new(Mirror::default()),
            processing: Mutex::new(Processing::default()),
            mirror_processing: Mutex::new(Processing::default()),
            running: tokio::sync::Mutex::new(()),
            wake: Notify::new(),
            listener: Box::new(listener),
        }
    }

    /// Sets the logged-in client, or logs out with `None`.
    pub fn set_session(&self, session: Option<(Client, Account)>) {
        *self.session.write().unwrap_or_else(|e| e.into_inner()) =
            session.map(|(client, account)| (Arc::new(client), account));
    }

    pub fn session(&self) -> Option<(Arc<Client>, Account)> {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_logged_in(&self) -> bool {
        self.session().is_some()
    }

    /// Turns copying changes to the second service on or off.
    pub fn set_mirroring(&self, enabled: bool) {
        self.mirror
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .enabled = enabled;
        if enabled {
            self.wake.notify_one();
        }
    }

    pub fn is_mirroring(&self) -> bool {
        self.mirror
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .enabled
    }

    /// Sets the client for the second service, or `None` when logged out.
    pub fn set_mirror_session(&self, session: Option<(Client, Account)>) {
        self.mirror
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .session = session.map(|(client, account)| (Arc::new(client), account));
        self.wake.notify_one();
    }

    pub fn mirror_session(&self) -> Option<(Arc<Client>, Account)> {
        self.mirror
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .session
            .clone()
    }

    /// Queues every list entry for the mirror, to copy the whole list once.
    /// Returns how many were queued.
    pub fn mirror_all(&self) -> Result<usize> {
        let db = self.db();
        let entries = list::all(&db)?;
        let time = now();
        for entry in &entries {
            mirror_queue::upsert(
                &db,
                &QueueItem {
                    anime_id: entry.anime_id,
                    dirty: Fields::all(),
                    time,
                    retry_count: 0,
                    last_error: None,
                },
            )?;
        }
        drop(db);
        self.wake.notify_one();
        self.emit(SyncEvent::QueueChanged);
        Ok(entries.len())
    }

    /// Forgets mirror changes that failed, such as anime with no match.
    pub fn mirror_dismiss_failed(&self) -> Result<usize> {
        let db = self.db();
        let mut removed = 0;
        for item in mirror_queue::all(&db)? {
            if item.last_error.is_some() {
                mirror_queue::remove(&db, item.anime_id)?;
                removed += 1;
            }
        }
        drop(db);
        self.emit(SyncEvent::QueueChanged);
        Ok(removed)
    }

    // ---------------------------------------------------------------------
    // Local edits

    /// Saves an edited entry and queues the changed fields. Returns `false` if
    /// nothing changed. Records history when the episode number changes.
    pub fn save_entry(&self, mut entry: ListEntry) -> Result<bool> {
        let db = self.db();
        let previous = list::get(&db, entry.anime_id)?;
        let baseline = previous.unwrap_or_else(|| ListEntry {
            anime_id: entry.anime_id,
            ..ListEntry::default()
        });
        let dirty = changed_fields(&baseline, &entry);
        if dirty.is_empty() {
            return Ok(false);
        }
        // The service's ID isn't the caller's to change.
        entry.id = baseline.id;
        entry.pending_delete = false;
        entry.last_updated = now();
        list::upsert(&db, &entry)?;
        if dirty.contains(Fields::EPISODE) && entry.watched_episodes > 0 {
            history::add(
                &db,
                entry.anime_id,
                entry.watched_episodes,
                entry.last_updated,
            )?;
        }
        self.push(&db, entry.anime_id, dirty)?;
        drop(db);
        self.emit(SyncEvent::ListChanged);
        Ok(true)
    }

    /// Marks an entry for deletion and queues it. Returns `false` if there's
    /// no such entry.
    pub fn remove_entry(&self, anime_id: i32) -> Result<bool> {
        let db = self.db();
        let Some(mut entry) = list::get(&db, anime_id)? else {
            return Ok(false);
        };
        entry.pending_delete = true;
        entry.last_updated = now();
        list::upsert(&db, &entry)?;
        // A delete replaces any pending update.
        queue::remove(&db, anime_id)?;
        self.push(&db, anime_id, Fields::empty())?;
        drop(db);
        self.emit(SyncEvent::ListChanged);
        Ok(true)
    }

    /// Adds `dirty` to the queue item for `anime_id`.
    fn push(&self, db: &Connection, anime_id: i32, dirty: Fields) -> Result<()> {
        {
            let mut processing = self.processing();
            if processing.anime_id == Some(anime_id) {
                processing.modified = true;
            }
        }
        let item = match queue::get(db, anime_id)? {
            Some(item) => QueueItem {
                dirty: item.dirty | dirty,
                time: now(),
                ..item
            },
            None => QueueItem {
                anime_id,
                dirty,
                time: now(),
                retry_count: 0,
                last_error: None,
            },
        };
        queue::upsert(db, &item)?;
        if self.is_mirroring() {
            {
                let mut processing = self.mirror_processing();
                if processing.anime_id == Some(anime_id) {
                    processing.modified = true;
                }
            }
            mirror_queue::upsert(
                db,
                &QueueItem {
                    anime_id,
                    dirty: Fields::all(),
                    time: now(),
                    retry_count: 0,
                    last_error: None,
                },
            )?;
        }
        self.wake.notify_one();
        self.emit(SyncEvent::QueueChanged);
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Remote

    /// Sends the queue, then fetches the list.
    pub async fn synchronize(&self) -> Result<()> {
        let _running = self.running.lock().await;
        self.emit(SyncEvent::Busy { busy: true });
        let result = async {
            let sent = self.process_queue_locked().await;
            self.report_mirror(self.process_mirror_locked().await);
            sent?;
            self.fetch_list_locked().await
        }
        .await;
        self.emit(SyncEvent::Busy { busy: false });
        result
    }

    /// Sends queued changes until the queue is empty or one fails.
    pub async fn process_queue(&self) -> Result<()> {
        let _running = self.running.lock().await;
        self.process_queue_locked().await
    }

    /// Replaces the local list with the service's.
    pub async fn fetch_list(&self) -> Result<()> {
        let _running = self.running.lock().await;
        self.fetch_list_locked().await
    }

    async fn process_queue_locked(&self) -> Result<()> {
        let (client, _) = self.session().ok_or(Error::NotLoggedIn)?;
        loop {
            let Some(item) = queue::all(&self.db())?.into_iter().next() else {
                return Ok(());
            };
            let Some(entry) = list::get(&self.db(), item.anime_id)? else {
                // Nothing left to send.
                queue::remove(&self.db(), item.anime_id)?;
                self.emit(SyncEvent::QueueChanged);
                continue;
            };

            *self.processing() = Processing {
                anime_id: Some(item.anime_id),
                modified: false,
            };
            let deleting = entry.pending_delete;
            let result = if deleting {
                client.delete_entry(&entry).await.map(|()| None)
            } else {
                client.save_entry(&entry, item.dirty).await.map(Some)
            };
            let modified = std::mem::take(&mut *self.processing()).modified;

            match result {
                Ok(saved) => {
                    let db = self.db();
                    reconcile(&db, item.anime_id, deleting, saved, modified)?;
                    match queue::get(&db, item.anime_id)? {
                        // Edited while in flight: send again later.
                        Some(mut current) if modified => {
                            current.retry_count = 0;
                            current.last_error = None;
                            queue::upsert(&db, &current)?;
                        }
                        _ => {
                            queue::remove(&db, item.anime_id)?;
                        }
                    }
                    drop(db);
                    self.emit(SyncEvent::ListChanged);
                    self.emit(SyncEvent::QueueChanged);
                }
                Err(error) => {
                    let db = self.db();
                    if let Some(mut current) = queue::get(&db, item.anime_id)? {
                        current.retry_count += 1;
                        current.last_error = Some(error.to_string());
                        queue::upsert(&db, &current)?;
                    }
                    drop(db);
                    self.emit(SyncEvent::QueueChanged);
                    return Err(error);
                }
            }
        }
    }

    /// Sends the mirror queue. Unlike the main queue, an anime with no match
    /// on the mirror is skipped (and stays queued with an error) rather than
    /// stopping the rest.
    pub async fn process_mirror(&self) -> Result<()> {
        let _running = self.running.lock().await;
        self.process_mirror_locked().await
    }

    async fn process_mirror_locked(&self) -> Result<()> {
        let Some((mirror, account)) = self.mirror_session() else {
            return Ok(());
        };
        let Some((primary, _)) = self.session() else {
            return Ok(());
        };
        if primary.service() == mirror.service() {
            return Ok(());
        }
        let items = mirror_queue::all(&self.db())?;
        if items.is_empty() {
            return Ok(());
        }
        let ids: Vec<i32> = items.iter().map(|i| i.anime_id).collect();
        let targets = self.match_ids(&primary, &mirror, &ids).await?;
        let target_name = mirror.service().name();

        for item in items {
            let target = targets.get(&item.anime_id).copied().unwrap_or(0);
            if target == 0 {
                self.mirror_failed(item.anime_id, format!("Not found on {target_name}."))?;
                continue;
            }
            let local = list::get(&self.db(), item.anime_id)?;
            *self.mirror_processing() = Processing {
                anime_id: Some(item.anime_id),
                modified: false,
            };
            let result = match local {
                Some(entry) if !entry.pending_delete => {
                    let entry = ListEntry {
                        id: 0,
                        anime_id: target,
                        ..entry
                    };
                    mirror.save_entry(&entry, Fields::all()).await.map(|_| ())
                }
                _ => mirror.delete_anime(&account.username, target).await,
            };
            let modified = std::mem::take(&mut *self.mirror_processing()).modified;
            match result {
                Ok(()) => {
                    // Edited again while in flight: send again.
                    if !modified {
                        mirror_queue::remove(&self.db(), item.anime_id)?;
                    }
                    self.emit(SyncEvent::QueueChanged);
                }
                Err(error) => {
                    self.mirror_failed(item.anime_id, error.to_string())?;
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// IDs on the mirror for `ids` on the main service; 0 if there's no match.
    /// Looks up IDs not seen before on AniList, which knows both.
    async fn match_ids(
        &self,
        primary: &Client,
        mirror: &Client,
        ids: &[i32],
    ) -> Result<HashMap<i32, i32>> {
        let (from, to) = (primary.service(), mirror.service());
        let mut known = id_map::get_many(&self.db(), from, to, ids)?;
        let missing: Vec<i32> = ids
            .iter()
            .copied()
            .filter(|id| !known.contains_key(id))
            .collect();
        if missing.is_empty() {
            return Ok(known);
        }
        let anilist = match (primary, mirror) {
            (Client::AniList(c), _) | (_, Client::AniList(c)) => c,
            _ => {
                return Err(Error::NotConfigured(
                    "Copying between these services isn't supported.".into(),
                ));
            }
        };
        let from_mal = from == ServiceId::MyAnimeList;
        let found: HashMap<i32, i32> = anilist
            .match_ids(&missing, from_mal)
            .await?
            .into_iter()
            .map(|(al, mal)| if from_mal { (mal, al) } else { (al, mal) })
            .collect();
        let rows: Vec<(i32, i32)> = missing
            .iter()
            .map(|id| (*id, found.get(id).copied().unwrap_or(0)))
            .collect();
        id_map::set_many(&self.db(), from, to, &rows)?;
        known.extend(rows);
        Ok(known)
    }

    fn mirror_failed(&self, anime_id: i32, message: String) -> Result<()> {
        let db = self.db();
        if let Some(mut current) = mirror_queue::get(&db, anime_id)? {
            current.retry_count += 1;
            current.last_error = Some(message);
            mirror_queue::upsert(&db, &current)?;
        }
        drop(db);
        self.emit(SyncEvent::QueueChanged);
        Ok(())
    }

    /// Reports a mirror failure as an event, naming the service.
    fn report_mirror(&self, result: Result<()>) {
        if let Err(error) = result {
            let service = self
                .mirror_session()
                .map_or("Second service", |(c, _)| c.service().name());
            self.emit(SyncEvent::Error {
                unauthorized: error.is_unauthorized(),
                message: format!("{service} (copy): {error}"),
            });
        }
    }

    async fn fetch_list_locked(&self) -> Result<()> {
        let (client, account) = self.session().ok_or(Error::NotLoggedIn)?;
        let remote = client.fetch_list(&account.username).await?;

        let db = self.db();
        anime::upsert_many(&db, &remote.anime)?;
        // Entries with unsent changes keep their local version.
        let queued: HashSet<i32> = queue::all(&db)?.iter().map(|q| q.anime_id).collect();
        let entries: Vec<ListEntry> = remote
            .entries
            .into_iter()
            .filter(|e| !queued.contains(&e.anime_id))
            .collect();
        list::upsert_many(&db, &entries)?;

        // Remove entries that are no longer on the service.
        let fetched: HashSet<i32> = entries.iter().map(|e| e.anime_id).collect();
        for local in list::all(&db)? {
            if !fetched.contains(&local.anime_id) && !queued.contains(&local.anime_id) {
                list::delete(&db, local.anime_id)?;
            }
        }
        drop(db);
        self.emit(SyncEvent::ListChanged);
        Ok(())
    }

    /// Sends the queue in the background: [`QUEUE_DEBOUNCE`] after the last
    /// edit, and every [`QUEUE_RETRY_INTERVAL`]. `enabled` is checked each
    /// time, so sync can be turned off in settings. Runs until dropped.
    pub async fn run_background(self: Arc<Self>, enabled: impl Fn() -> bool + Send + 'static) {
        let mut retry = tokio::time::interval(QUEUE_RETRY_INTERVAL);
        retry.tick().await;
        loop {
            tokio::select! {
                () = self.wake.notified() => {
                    while tokio::time::timeout(QUEUE_DEBOUNCE, self.wake.notified()).await.is_ok() {}
                }
                _ = retry.tick() => {}
            }
            if !enabled() || !self.is_logged_in() {
                continue;
            }
            let pending = queue::all(&self.db())
                .map(|q| !q.is_empty())
                .unwrap_or(false);
            if pending {
                if let Err(error) = self.process_queue().await {
                    self.emit(SyncEvent::Error {
                        unauthorized: error.is_unauthorized(),
                        message: error.to_string(),
                    });
                }
            }
            let mirror_pending = mirror_queue::all(&self.db())
                .map(|q| !q.is_empty())
                .unwrap_or(false);
            if mirror_pending {
                let result = self.process_mirror().await;
                self.report_mirror(result);
            }
        }
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn processing(&self) -> MutexGuard<'_, Processing> {
        self.processing.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mirror_processing(&self) -> MutexGuard<'_, Processing> {
        self.mirror_processing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, event: SyncEvent) {
        (self.listener)(event);
    }
}

fn changed_fields(old: &ListEntry, new: &ListEntry) -> Fields {
    let mut dirty = Fields::empty();
    dirty.set(
        Fields::EPISODE,
        old.watched_episodes != new.watched_episodes,
    );
    dirty.set(Fields::SCORE, old.score != new.score);
    dirty.set(Fields::STATUS, old.status != new.status);
    dirty.set(Fields::REWATCHING, old.rewatching != new.rewatching);
    dirty.set(
        Fields::REWATCHED_TIMES,
        old.rewatched_times != new.rewatched_times,
    );
    dirty.set(Fields::DATE_STARTED, old.date_started != new.date_started);
    dirty.set(
        Fields::DATE_COMPLETED,
        old.date_completed != new.date_completed,
    );
    dirty.set(Fields::NOTES, old.notes != new.notes);
    dirty
}

/// Applies a successful response to the local entry.
fn reconcile(
    db: &Connection,
    anime_id: i32,
    deleting: bool,
    saved: Option<SavedEntry>,
    modified: bool,
) -> Result<()> {
    let Some(local) = list::get(db, anime_id)? else {
        return Ok(());
    };
    if deleting {
        if local.pending_delete {
            list::delete(db, anime_id)?;
        } else {
            // Saved again while the delete was in flight, so the service no
            // longer has it: it must be added again.
            list::upsert(db, &ListEntry { id: 0, ..local })?;
        }
        return Ok(());
    }
    let Some(saved) = saved else {
        return Ok(());
    };
    if let Some(anime) = &saved.anime {
        anime::upsert(db, anime)?;
    }
    if let Some(remote) = saved.entry {
        let entry = if modified {
            // The response predates the local edits, so keep those.
            ListEntry {
                id: remote.id,
                ..local
            }
        } else {
            remote
        };
        list::upsert(db, &entry)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use patchouli_core::{FuzzyDate, ListStatus};

    use super::*;

    fn synchronizer() -> (Synchronizer, Arc<Mutex<Vec<SyncEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let db = Arc::new(Mutex::new(patchouli_db::open_in_memory().unwrap()));
        let sync = Synchronizer::new(db, move |e| sink.lock().unwrap().push(e));
        (sync, events)
    }

    fn entry(anime_id: i32) -> ListEntry {
        ListEntry {
            anime_id,
            watched_episodes: 1,
            status: ListStatus::Watching,
            ..ListEntry::default()
        }
    }

    #[test]
    fn save_queues_changed_fields_and_records_history() {
        let (sync, events) = synchronizer();
        assert!(sync.save_entry(entry(1)).unwrap());

        let db = sync.db();
        let item = queue::get(&db, 1).unwrap().unwrap();
        assert_eq!(item.dirty, Fields::EPISODE | Fields::STATUS);
        assert_eq!(history::all(&db).unwrap().len(), 1);
        drop(db);

        // Saving the same entry again changes nothing.
        assert!(!sync.save_entry(entry(1)).unwrap());

        // A second edit merges into the queued item.
        let mut edited = entry(1);
        edited.score = 70;
        edited.date_started = FuzzyDate::new(2024, 1, 1);
        assert!(sync.save_entry(edited).unwrap());
        let item = queue::get(&sync.db(), 1).unwrap().unwrap();
        assert_eq!(
            item.dirty,
            Fields::EPISODE | Fields::STATUS | Fields::SCORE | Fields::DATE_STARTED
        );
        assert!(events.lock().unwrap().contains(&SyncEvent::QueueChanged));
    }

    #[test]
    fn save_keeps_the_service_id() {
        let (sync, _) = synchronizer();
        list::upsert(&sync.db(), &ListEntry { id: 77, ..entry(1) }).unwrap();
        let mut edited = entry(1);
        edited.watched_episodes = 2;
        sync.save_entry(edited).unwrap();
        assert_eq!(list::get(&sync.db(), 1).unwrap().unwrap().id, 77);
    }

    #[test]
    fn remove_marks_pending_delete_and_replaces_update() {
        let (sync, _) = synchronizer();
        sync.save_entry(entry(1)).unwrap();
        assert!(sync.remove_entry(1).unwrap());
        let db = sync.db();
        assert!(list::get(&db, 1).unwrap().unwrap().pending_delete);
        assert_eq!(queue::get(&db, 1).unwrap().unwrap().dirty, Fields::empty());
        drop(db);
        assert!(!sync.remove_entry(2).unwrap());
    }

    #[test]
    fn reconcile_delete_of_resaved_entry_clears_id() {
        let (sync, _) = synchronizer();
        let db = sync.db();
        list::upsert(&db, &ListEntry { id: 5, ..entry(1) }).unwrap();
        reconcile(&db, 1, true, None, false).unwrap();
        let local = list::get(&db, 1).unwrap().unwrap();
        assert_eq!(local.id, 0);
    }

    #[test]
    fn reconcile_keeps_local_edits_made_in_flight() {
        let (sync, _) = synchronizer();
        let db = sync.db();
        let local = ListEntry {
            watched_episodes: 3,
            ..entry(1)
        };
        list::upsert(&db, &local).unwrap();
        let remote = ListEntry {
            id: 42,
            watched_episodes: 2,
            ..entry(1)
        };
        let saved = SavedEntry {
            entry: Some(remote.clone()),
            anime: None,
        };
        reconcile(&db, 1, false, Some(saved.clone()), true).unwrap();
        let after = list::get(&db, 1).unwrap().unwrap();
        assert_eq!((after.id, after.watched_episodes), (42, 3));

        reconcile(&db, 1, false, Some(saved), false).unwrap();
        assert_eq!(list::get(&db, 1).unwrap().unwrap(), remote);
    }

    #[test]
    fn mirroring_queues_changes_for_the_second_service() {
        let (sync, _) = synchronizer();
        sync.save_entry(entry(1)).unwrap();
        assert!(mirror_queue::all(&sync.db()).unwrap().is_empty());

        sync.set_mirroring(true);
        sync.save_entry(entry(2)).unwrap();
        sync.remove_entry(1).unwrap();
        let queued: Vec<i32> = mirror_queue::all(&sync.db())
            .unwrap()
            .iter()
            .map(|i| i.anime_id)
            .collect();
        assert_eq!(queued.len(), 2);
        assert!(queued.contains(&1) && queued.contains(&2));

        // Copying the whole list queues every entry.
        list::upsert(&sync.db(), &entry(3)).unwrap();
        assert_eq!(sync.mirror_all().unwrap(), 3);
    }

    #[tokio::test]
    async fn remote_operations_need_a_session() {
        let (sync, _) = synchronizer();
        assert!(matches!(
            sync.process_queue().await,
            Err(Error::NotLoggedIn)
        ));
        assert!(matches!(sync.fetch_list().await, Err(Error::NotLoggedIn)));
    }
}
