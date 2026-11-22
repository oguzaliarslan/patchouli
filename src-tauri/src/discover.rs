//! Sequels and recommendations of anime in the list.
//!
//! Completed and watched anime are checked in the background, each at most
//! once every [`RECHECK`]. Results are stored (`related` table), and the
//! suggestions are worked out from them and the list when asked.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use patchouli_core::anime::Status;
use patchouli_core::{Anime, ListEntry, ListStatus, ServiceId};
use patchouli_db::related::{self, Kind};
use patchouli_db::{anime, list, meta};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;
use crate::error::CommandResult;

/// Event emitted when stored sequels or recommendations change.
pub const DISCOVER_CHANGED: &str = "discover-changed";
/// `meta` key: anime IDs the user doesn't want suggested, as JSON.
const META_DISMISSED: &str = "discover.dismissed";
const DAY: i64 = 86_400;
const RECHECK: i64 = 7 * DAY;
/// MyAnimeList needs a request per anime, so each run checks only a few.
const MAL_PER_RUN: usize = 30;
const MAL_PER_REFRESH: usize = 100;
const START_DELAY: Duration = Duration::from_secs(60);
const INTERVAL: Duration = Duration::from_secs(30 * 60);
/// A score of 7/10 or more counts as liked.
const LIKED_SCORE: i32 = 70;

#[derive(Default)]
pub struct Discover {
    running: tokio::sync::Mutex<()>,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn seeds(entries: &[ListEntry]) -> impl Iterator<Item = &ListEntry> {
    entries.iter().filter(|e| {
        !e.pending_delete && matches!(e.status, ListStatus::Completed | ListStatus::Watching)
    })
}

/// Anime to check, never-checked first, then the most recently updated.
fn due(entries: &[ListEntry], checked: &HashMap<i32, i64>, now: i64, force: bool) -> Vec<i32> {
    let mut due: Vec<&ListEntry> = seeds(entries)
        .filter(|e| force || checked.get(&e.anime_id).is_none_or(|t| now - t >= RECHECK))
        .collect();
    due.sort_by_key(|e| (checked.contains_key(&e.anime_id), -e.last_updated));
    due.into_iter().map(|e| e.anime_id).collect()
}

/// Fetches sequels and recommendations of anime that are due. Returns how
/// many anime were checked; 0 if logged out or a check is already running.
async fn refresh(app: &AppHandle, force: bool) -> patchouli_sync::Result<usize> {
    let state = app.state::<AppState>();
    let Ok(_running) = state.discover.running.try_lock() else {
        return Ok(0);
    };
    let Some((client, _)) = state.sync.session() else {
        return Ok(0);
    };
    let now = now();
    let mut ids = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let entries = list::all(&db)?;
        due(&entries, &related::checked(&db)?, now, force)
    };
    if client.service() == ServiceId::MyAnimeList {
        ids.truncate(if force { MAL_PER_REFRESH } else { MAL_PER_RUN });
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let found = client.related(&ids).await?;
    let partial = client.service() == ServiceId::MyAnimeList;

    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut seen = HashSet::new();
    for r in &found {
        anime::upsert_many(&db, &r.sequels)?;
        for rec in &r.recommendations {
            // MyAnimeList's are only an ID, title and picture: don't replace
            // what's known.
            if !partial || anime::get(&db, rec.id)?.is_none() {
                anime::upsert(&db, rec)?;
            }
        }
        let sequels: Vec<i32> = r.sequels.iter().map(|a| a.id).collect();
        let recs: Vec<i32> = r.recommendations.iter().map(|a| a.id).collect();
        related::replace(&db, r.anime_id, &sequels, &recs, now)?;
        seen.insert(r.anime_id);
    }
    // Not found on the service: don't ask again until the next recheck.
    for id in ids.iter().filter(|id| !seen.contains(id)) {
        related::replace(&db, *id, &[], &[], now)?;
    }
    drop(db);
    let _ = app.emit(DISCOVER_CHANGED, ());
    Ok(ids.len())
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        let mut tick = tokio::time::interval(INTERVAL);
        loop {
            tick.tick().await;
            if !app.state::<AppState>().settings.get().sync_enabled {
                continue;
            }
            match refresh(&app, false).await {
                Ok(0) => {}
                Ok(n) => log::info!("Checked sequels and recommendations of {n} anime"),
                Err(e) => log::warn!("Checking sequels and recommendations: {e}"),
            }
        }
    });
}

fn dismissed(db: &patchouli_db::Connection) -> HashSet<i32> {
    meta::get(db, META_DISMISSED)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    anime: Anime,
    /// The anime in the list it's suggested because of.
    because_id: i32,
    because_title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationGroup {
    because_id: i32,
    because_title: String,
    items: Vec<Anime>,
}

/// Sequels of completed anime that aren't in the list: airing first, then
/// upcoming, then those already out.
fn sequel_suggestions(
    entries: &[ListEntry],
    rows: &[related::Row],
    anime: &HashMap<i32, Anime>,
    dismissed: &HashSet<i32>,
) -> Vec<(i32, i32)> {
    let in_list: HashSet<i32> = entries.iter().map(|e| e.anime_id).collect();
    let completed: HashMap<i32, &ListEntry> = entries
        .iter()
        .filter(|e| e.status == ListStatus::Completed && !e.pending_delete)
        .map(|e| (e.anime_id, e))
        .collect();
    let mut seen = HashSet::new();
    let mut result: Vec<(i32, i32, i64)> = rows
        .iter()
        .filter(|r| r.kind == Kind::Sequel && completed.contains_key(&r.anime_id))
        .filter(|r| !in_list.contains(&r.related_id) && !dismissed.contains(&r.related_id))
        .filter(|r| anime.contains_key(&r.related_id))
        .filter(|r| seen.insert(r.related_id))
        .map(|r| {
            (
                r.related_id,
                r.anime_id,
                completed[&r.anime_id].last_updated,
            )
        })
        .collect();
    let order = |id: i32| match anime[&id].status {
        Status::Airing => 0,
        Status::NotYetAired => 1,
        _ => 2,
    };
    result.sort_by_key(|&(id, _, updated)| (order(id), -updated));
    result
        .into_iter()
        .map(|(id, because, _)| (id, because))
        .collect()
}

/// Recommendations grouped by the liked anime they come from. Each anime is
/// suggested once, and only groups with at least 3 are kept.
fn recommendation_groups(
    entries: &[ListEntry],
    rows: &[related::Row],
    anime: &HashMap<i32, Anime>,
    dismissed: &HashSet<i32>,
) -> Vec<(i32, Vec<i32>)> {
    let in_list: HashSet<i32> = entries.iter().map(|e| e.anime_id).collect();
    let mut liked: Vec<&ListEntry> = seeds(entries).filter(|e| e.score >= LIKED_SCORE).collect();
    if liked.is_empty() {
        // Unscored lists: what was completed most recently.
        liked = seeds(entries)
            .filter(|e| e.status == ListStatus::Completed)
            .collect();
    }
    liked.sort_by_key(|e| (-e.score, -e.last_updated));
    let mut used = HashSet::new();
    let mut groups = Vec::new();
    for seed in liked {
        let items: Vec<i32> = rows
            .iter()
            .filter(|r| r.anime_id == seed.anime_id && r.kind == Kind::Recommendation)
            .map(|r| r.related_id)
            .filter(|id| !in_list.contains(id) && !dismissed.contains(id))
            .filter(|id| anime.contains_key(id) && !used.contains(id))
            .take(8)
            .collect();
        if items.len() >= 3 {
            used.extend(items.iter().copied());
            groups.push((seed.anime_id, items));
            if groups.len() == 6 {
                break;
            }
        }
    }
    groups
}

struct Data {
    entries: Vec<ListEntry>,
    rows: Vec<related::Row>,
    anime: HashMap<i32, Anime>,
    titles: HashMap<i32, String>,
    dismissed: HashSet<i32>,
}

fn load(state: &AppState) -> CommandResult<Data> {
    let db = state.db.lock()?;
    Ok(Data {
        entries: list::all(&db)?,
        rows: related::all(&db)?,
        anime: anime::all(&db)?.into_iter().map(|a| (a.id, a)).collect(),
        titles: crate::library::display_titles(&db)?,
        dismissed: dismissed(&db),
    })
}

fn title(data: &Data, state: &AppState, id: i32) -> String {
    data.titles.get(&id).cloned().unwrap_or_else(|| {
        data.anime.get(&id).map_or_else(
            || format!("#{id}"),
            |a| crate::sharing::preferred_title(a, state),
        )
    })
}

#[tauri::command]
pub fn sequels_get(state: State<'_, AppState>) -> CommandResult<Vec<Suggestion>> {
    let data = load(&state)?;
    Ok(
        sequel_suggestions(&data.entries, &data.rows, &data.anime, &data.dismissed)
            .into_iter()
            .map(|(id, because)| Suggestion {
                anime: data.anime[&id].clone(),
                because_id: because,
                because_title: title(&data, &state, because),
            })
            .collect(),
    )
}

#[tauri::command]
pub fn recommendations_get(state: State<'_, AppState>) -> CommandResult<Vec<RecommendationGroup>> {
    let data = load(&state)?;
    Ok(
        recommendation_groups(&data.entries, &data.rows, &data.anime, &data.dismissed)
            .into_iter()
            .map(|(because, items)| RecommendationGroup {
                because_id: because,
                because_title: title(&data, &state, because),
                items: items.iter().map(|id| data.anime[id].clone()).collect(),
            })
            .collect(),
    )
}

/// Stops suggesting `anime_id`.
#[tauri::command]
pub fn suggestion_dismiss(
    app: AppHandle,
    state: State<'_, AppState>,
    anime_id: i32,
) -> CommandResult<()> {
    let db = state.db.lock()?;
    let mut ids = dismissed(&db);
    ids.insert(anime_id);
    meta::set(&db, META_DISMISSED, &serde_json::to_string(&ids)?)?;
    drop(db);
    let _ = app.emit(DISCOVER_CHANGED, ());
    Ok(())
}

/// Checks every completed and watched anime again now. Returns how many.
#[tauri::command]
pub async fn discover_refresh(app: AppHandle) -> CommandResult<usize> {
    Ok(refresh(&app, true).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(anime_id: i32, status: ListStatus, score: i32, updated: i64) -> ListEntry {
        ListEntry {
            anime_id,
            status,
            score,
            last_updated: updated,
            ..ListEntry::default()
        }
    }

    fn row(anime_id: i32, related_id: i32, kind: Kind, rank: i32) -> related::Row {
        related::Row {
            anime_id,
            related_id,
            kind,
            rank,
        }
    }

    fn catalog(items: &[(i32, Status)]) -> HashMap<i32, Anime> {
        items
            .iter()
            .map(|&(id, status)| {
                (
                    id,
                    Anime {
                        id,
                        status,
                        ..Anime::default()
                    },
                )
            })
            .collect()
    }

    #[test]
    fn suggests_missing_sequels_airing_first() {
        let entries = [
            entry(1, ListStatus::Completed, 0, 10),
            entry(2, ListStatus::Completed, 0, 20),
            entry(3, ListStatus::Watching, 0, 30),
            entry(11, ListStatus::PlanToWatch, 0, 5),
        ];
        let rows = [
            row(1, 10, Kind::Sequel, 0), // finished, from older entry
            row(1, 11, Kind::Sequel, 1), // already in the list
            row(2, 12, Kind::Sequel, 0), // airing
            row(2, 13, Kind::Sequel, 1), // dismissed
            row(3, 14, Kind::Sequel, 0), // still watching the first season
            row(2, 15, Kind::Recommendation, 0),
            row(1, 12, Kind::Sequel, 2), // repeated
        ];
        let anime = catalog(&[
            (10, Status::FinishedAiring),
            (11, Status::FinishedAiring),
            (12, Status::Airing),
            (13, Status::Airing),
            (14, Status::Airing),
            (15, Status::Airing),
        ]);
        let dismissed = HashSet::from([13]);
        assert_eq!(
            sequel_suggestions(&entries, &rows, &anime, &dismissed),
            [(12, 2), (10, 1)]
        );
    }

    #[test]
    fn groups_recommendations_by_liked_anime() {
        let entries = [
            entry(1, ListStatus::Completed, 90, 10),
            entry(2, ListStatus::Completed, 80, 20),
            entry(3, ListStatus::Completed, 40, 30),
            entry(20, ListStatus::Dropped, 0, 0),
        ];
        let mut rows = Vec::new();
        for (rank, id) in [20, 21, 22, 23].into_iter().enumerate() {
            rows.push(row(1, id, Kind::Recommendation, rank as i32));
        }
        // Overlaps with the first group, leaving too few.
        for (rank, id) in [21, 22, 24].into_iter().enumerate() {
            rows.push(row(2, id, Kind::Recommendation, rank as i32));
        }
        for (rank, id) in [25, 26, 27].into_iter().enumerate() {
            rows.push(row(3, id, Kind::Recommendation, rank as i32));
        }
        let anime = catalog(
            &(20..=27)
                .map(|id| (id, Status::FinishedAiring))
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            recommendation_groups(&entries, &rows, &anime, &HashSet::new()),
            [(1, vec![21, 22, 23])]
        );
    }

    #[test]
    fn checks_unchecked_first_then_recent() {
        let entries = [
            entry(1, ListStatus::Completed, 0, 10),
            entry(2, ListStatus::Completed, 0, 30),
            entry(3, ListStatus::Watching, 0, 20),
            entry(4, ListStatus::PlanToWatch, 0, 40),
            entry(5, ListStatus::Completed, 0, 50),
        ];
        let now = 100 * DAY;
        let checked = HashMap::from([(1, now - RECHECK), (5, now - DAY)]);
        assert_eq!(due(&entries, &checked, now, false), [2, 3, 1]);
        assert_eq!(due(&entries, &checked, now, true), [2, 3, 5, 1]);
    }
}
