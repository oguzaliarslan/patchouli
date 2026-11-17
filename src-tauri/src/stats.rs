//! List statistics (Taiga v1 `taiga/stats.cpp`), the seasonal anime browser
//! and the profile.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use tauri::State;
use tracker_core::anime::Type;
use tracker_core::{Anime, HistoryItem, ListEntry, ListStatus, SeasonName};
use tracker_db::{anime, history, list};

use crate::AppState;
use crate::error::CommandResult;

/// Minutes per episode, guessed from the type when unknown (Taiga's
/// `EstimateDuration`).
fn estimate_duration(anime: Option<&Anime>) -> i64 {
    match anime {
        Some(a) if a.episode_length > 0 => a.episode_length.into(),
        Some(a) => match a.r#type {
            Type::Movie => 90,
            Type::Special => 12,
            Type::Music => 5,
            _ => 24,
        },
        None => 24,
    }
}

/// Episodes in total, guessed when unknown (Taiga's `EstimateEpisodeCount`):
/// most series run 12, 26 or 52 episodes.
fn estimate_episodes(anime: Option<&Anime>, entry: &ListEntry) -> i64 {
    if let Some(a) = anime
        && a.episode_count > 0
    {
        return a.episode_count.into();
    }
    let last = anime
        .map_or(0, |a| a.last_aired_episode)
        .max(entry.watched_episodes);
    match last {
        n if n < 12 => 12,
        n if n < 24 => 26,
        n if n < 50 => 52,
        n => i64::from(n),
    }
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Entries in the list.
    pub anime_count: usize,
    /// Episodes watched, counting rewatches.
    pub episode_count: i64,
    pub seconds_watched: i64,
    /// Time left on anime being watched, on hold or planned.
    pub seconds_planned: i64,
    /// Mean of non-zero scores (0–100).
    pub mean_score: f64,
    pub score_deviation: f64,
    /// Entries scored 1–9, 10–19, …, 100 (index 0 is unused, as in Taiga).
    pub score_counts: [usize; 11],
    pub status_counts: BTreeMap<String, usize>,
    pub type_counts: BTreeMap<String, usize>,
    /// The most common genres, most first.
    pub top_genres: Vec<(String, usize)>,
    pub history_count: usize,
    /// Episodes watched per month (`YYYY-MM`), last 12 months with history.
    pub history_by_month: Vec<(String, usize)>,
}

pub fn compute(
    anime: &HashMap<i32, Anime>,
    entries: &[ListEntry],
    history: &[HistoryItem],
) -> Stats {
    let entries: Vec<&ListEntry> = entries.iter().filter(|e| !e.pending_delete).collect();
    let mut stats = Stats {
        anime_count: entries.len(),
        history_count: history.len(),
        ..Stats::default()
    };
    let mut genres: HashMap<&str, usize> = HashMap::new();
    let mut scores = Vec::new();
    for entry in &entries {
        let item = anime.get(&entry.anime_id);
        let count = item.map_or(0, |a| a.episode_count.max(0));
        let watched =
            i64::from(entry.watched_episodes) + i64::from(entry.rewatched_times) * i64::from(count);
        stats.episode_count += watched;
        let minutes = estimate_duration(item);
        stats.seconds_watched += watched * minutes * 60;
        if !matches!(
            entry.status,
            ListStatus::Completed | ListStatus::Dropped | ListStatus::NotInList
        ) {
            let left = (estimate_episodes(item, entry) - i64::from(entry.watched_episodes)).max(0);
            stats.seconds_planned += left * minutes * 60;
        }
        if entry.score > 0 {
            scores.push(f64::from(entry.score));
            stats.score_counts[(entry.score / 10).clamp(0, 10) as usize] += 1;
        }
        *stats
            .status_counts
            .entry(format!("{:?}", entry.status))
            .or_default() += 1;
        let kind = item.map_or(Type::Unknown, |a| a.r#type);
        *stats.type_counts.entry(format!("{kind:?}")).or_default() += 1;
        for genre in item.into_iter().flat_map(|a| &a.genres) {
            *genres.entry(genre).or_default() += 1;
        }
    }
    if !scores.is_empty() {
        let n = scores.len() as f64;
        stats.mean_score = scores.iter().sum::<f64>() / n;
        stats.score_deviation = (scores
            .iter()
            .map(|s| (s - stats.mean_score).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
    }
    let mut genres: Vec<(String, usize)> =
        genres.into_iter().map(|(g, n)| (g.to_owned(), n)).collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    genres.truncate(10);
    stats.top_genres = genres;

    stats.history_by_month = by_month(history);
    stats
}

/// Episodes per month, for the last 12 months that have history, with empty
/// months in between included.
fn by_month(history: &[HistoryItem]) -> Vec<(String, usize)> {
    use chrono::Datelike;
    let mut months: BTreeMap<(i32, u32), usize> = BTreeMap::new();
    for item in history {
        if let Some(time) = chrono::DateTime::from_timestamp(item.time, 0) {
            let local = time.with_timezone(&chrono::Local);
            *months.entry((local.year(), local.month())).or_default() += 1;
        }
    }
    let (Some(&first), Some(&last)) = (months.keys().next(), months.keys().next_back()) else {
        return Vec::new();
    };
    let mut all = Vec::new();
    let mut month = first;
    while month <= last {
        all.push((
            format!("{}-{:02}", month.0, month.1),
            months.get(&month).copied().unwrap_or(0),
        ));
        month = if month.1 == 12 {
            (month.0 + 1, 1)
        } else {
            (month.0, month.1 + 1)
        };
    }
    let skip = all.len().saturating_sub(12);
    all.split_off(skip)
}

#[tauri::command]
pub fn stats_get(state: State<'_, AppState>) -> CommandResult<Stats> {
    let db = state.db.lock()?;
    let anime: HashMap<i32, Anime> = anime::all(&db)?.into_iter().map(|a| (a.id, a)).collect();
    Ok(compute(&anime, &list::all(&db)?, &history::all(&db)?))
}

/// Every anime of a season, from the current service. They're stored, so
/// they can be opened and added like search results.
#[tauri::command]
pub async fn season_get(
    state: State<'_, AppState>,
    season: SeasonName,
    year: i32,
) -> CommandResult<Vec<Anime>> {
    let (client, _) = state
        .sync
        .session()
        .ok_or(tracker_sync::Error::NotLoggedIn)?;
    let results = client.season(season, year).await?;
    let db = state.db.lock()?;
    anime::upsert_many(&db, &results)?;
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anime(id: i32, kind: Type, episodes: i32, length: i32, genres: &[&str]) -> Anime {
        Anime {
            id,
            r#type: kind,
            episode_count: episodes,
            episode_length: length,
            genres: genres.iter().map(|g| (*g).to_owned()).collect(),
            ..Anime::default()
        }
    }

    fn entry(anime_id: i32, status: ListStatus, watched: i32, score: i32) -> ListEntry {
        ListEntry {
            anime_id,
            status,
            watched_episodes: watched,
            score,
            ..ListEntry::default()
        }
    }

    #[test]
    fn computes_taiga_statistics() {
        let anime: HashMap<i32, Anime> = [
            anime(1, Type::Tv, 12, 24, &["Action", "Drama"]),
            anime(2, Type::Movie, 1, 0, &["Drama"]),
            anime(3, Type::Tv, 0, 0, &[]),
        ]
        .into_iter()
        .map(|a| (a.id, a))
        .collect();
        let mut rewatched = entry(1, ListStatus::Completed, 12, 80);
        rewatched.rewatched_times = 1;
        let entries = vec![
            rewatched,
            entry(2, ListStatus::Completed, 1, 60),
            // Unknown count: 5 watched → 12 estimated → 7 left.
            entry(3, ListStatus::Watching, 5, 0),
            ListEntry {
                pending_delete: true,
                ..entry(4, ListStatus::Watching, 1, 100)
            },
        ];
        let stats = compute(&anime, &entries, &[]);
        assert_eq!(stats.anime_count, 3);
        assert_eq!(stats.episode_count, 24 + 1 + 5);
        assert_eq!(stats.seconds_watched, (24 * 24 + 90 + 5 * 24) * 60);
        assert_eq!(stats.seconds_planned, 7 * 24 * 60);
        assert!((stats.mean_score - 70.0).abs() < 1e-9);
        assert!((stats.score_deviation - 10.0).abs() < 1e-9);
        assert_eq!(stats.score_counts[8], 1);
        assert_eq!(stats.score_counts[6], 1);
        assert_eq!(stats.status_counts["Completed"], 2);
        assert_eq!(stats.type_counts["Tv"], 2);
        assert_eq!(stats.top_genres[0], ("Drama".to_owned(), 2));
    }

    #[test]
    fn fills_empty_months() {
        let at = |y, m| {
            use chrono::TimeZone;
            let time = chrono::Local
                .with_ymd_and_hms(y, m, 15, 12, 0, 0)
                .unwrap()
                .timestamp();
            HistoryItem {
                id: 0,
                anime_id: 1,
                episode: 1,
                time,
            }
        };
        let months = by_month(&[at(2024, 11), at(2025, 2), at(2025, 2)]);
        assert_eq!(
            months,
            [
                ("2024-11".into(), 1),
                ("2024-12".into(), 0),
                ("2025-01".into(), 0),
                ("2025-02".into(), 2)
            ]
        );
    }

    #[test]
    fn estimates_episode_counts() {
        let e = entry(1, ListStatus::Watching, 30, 0);
        assert_eq!(estimate_episodes(None, &e), 52);
        let e = entry(1, ListStatus::Watching, 13, 0);
        assert_eq!(estimate_episodes(None, &e), 26);
        let e = entry(1, ListStatus::Watching, 60, 0);
        assert_eq!(estimate_episodes(None, &e), 60);
    }
}
