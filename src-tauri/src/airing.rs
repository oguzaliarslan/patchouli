//! New-episode alerts for anime being watched.
//!
//! `next_episode_time` is the airing time the service last reported. Once it
//! passes, the episode has aired: notify (once per episode) and fetch the
//! anime again so the time moves on to the following episode.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use patchouli_core::{Anime, ListEntry, ListStatus};
use patchouli_db::{anime, list, meta};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

/// `meta` key holding the last episode notified per anime, as JSON.
const META_NOTIFIED: &str = "airing.notified";
/// Episodes that aired longer ago than this (say, while the app was closed)
/// are not announced.
const NOTIFY_WINDOW: i64 = 12 * 3600;
/// How often an anime whose airing time has passed is fetched again.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Airing {
    /// When each anime was last fetched because its airing time had passed.
    refreshed: Mutex<HashMap<i32, Instant>>,
}

/// An episode that has aired since the anime was last fetched.
#[derive(Debug, PartialEq)]
struct Aired {
    anime_id: i32,
    episode: i32,
    /// Aired recently and not announced yet.
    announce: bool,
}

/// Episodes of anime being watched whose airing time has passed.
fn aired(
    entries: &[ListEntry],
    anime: &HashMap<i32, Anime>,
    notified: &HashMap<i32, i32>,
    now: i64,
) -> Vec<Aired> {
    entries
        .iter()
        .filter(|e| e.status == ListStatus::Watching && !e.pending_delete)
        .filter_map(|e| anime.get(&e.anime_id))
        .filter(|a| a.next_episode_time > 0 && a.next_episode_time <= now)
        .map(|a| {
            let episode = a.last_aired_episode + 1;
            Aired {
                anime_id: a.id,
                episode,
                announce: now - a.next_episode_time <= NOTIFY_WINDOW
                    && notified.get(&a.id).is_none_or(|&n| n < episode),
            }
        })
        .collect()
}

fn notification_text(episodes: &[(String, i32)]) -> (String, String) {
    match episodes {
        [(title, episode)] => (format!("Episode {episode} of {title} aired"), String::new()),
        _ => (
            format!("{} new episodes aired", episodes.len()),
            episodes
                .iter()
                .map(|(title, episode)| format!("{title} #{episode}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

fn load_notified(db: &patchouli_db::Connection) -> HashMap<i32, i32> {
    meta::get(db, META_NOTIFIED)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

async fn check(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<AppState>();
    let settings = state.settings.get();
    let now = now();
    let (due, titles) = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let entries = list::all(&db)?;
        let anime: HashMap<i32, Anime> = anime::all(&db)?.into_iter().map(|a| (a.id, a)).collect();
        let mut notified = load_notified(&db);
        let due = aired(&entries, &anime, &notified, now);
        let announced: Vec<&Aired> = due.iter().filter(|a| a.announce).collect();
        if !announced.is_empty() {
            for a in &announced {
                notified.insert(a.anime_id, a.episode);
            }
            // Forget anime no longer in the list.
            notified.retain(|id, _| entries.iter().any(|e| e.anime_id == *id));
            meta::set(&db, META_NOTIFIED, &serde_json::to_string(&notified)?)?;
        }
        let mut display = crate::library::display_titles(&db)?;
        let titles: Vec<(String, i32)> = announced
            .iter()
            .filter_map(|a| {
                let title = display.remove(&a.anime_id).or_else(|| {
                    anime
                        .get(&a.anime_id)
                        .map(|x| crate::sharing::preferred_title(x, &state))
                })?;
                Some((title, a.episode))
            })
            .collect();
        (due, titles)
    };

    if settings.notify_airing && !titles.is_empty() {
        let (heading, body) = notification_text(&titles);
        let mut builder = app.notification().builder().title(heading);
        if !body.is_empty() {
            builder = builder.body(body);
        }
        let _ = builder.show();
    }

    // Fetch the anime again to learn the next airing time.
    let Some((client, _)) = state.sync.session() else {
        return Ok(());
    };
    let stale: Vec<i32> = {
        let mut refreshed = state
            .airing
            .refreshed
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut stale = Vec::new();
        for a in &due {
            if refreshed
                .get(&a.anime_id)
                .is_none_or(|t| t.elapsed() >= REFRESH_INTERVAL)
            {
                refreshed.insert(a.anime_id, Instant::now());
                stale.push(a.anime_id);
            }
        }
        stale
    };
    let mut changed = false;
    for id in stale {
        match client.fetch_anime(id).await {
            Ok(Some(fetched)) => {
                anime::upsert(
                    &state.db.lock().unwrap_or_else(|e| e.into_inner()),
                    &fetched,
                )?;
                changed = true;
            }
            Ok(None) => {}
            Err(e) => log::warn!("Couldn't refresh airing time of anime {id}: {e}"),
        }
    }
    if changed {
        let _ = app.emit(
            crate::sync::SYNC_EVENT,
            patchouli_sync::SyncEvent::ListChanged,
        );
    }
    Ok(())
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(CHECK_INTERVAL);
        loop {
            tick.tick().await;
            if let Err(e) = check(&app).await {
                log::warn!("Airing check: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(anime_id: i32, status: ListStatus) -> ListEntry {
        ListEntry {
            anime_id,
            status,
            ..ListEntry::default()
        }
    }

    fn anime(id: i32, next: i64, last_aired: i32) -> Anime {
        Anime {
            id,
            next_episode_time: next,
            last_aired_episode: last_aired,
            ..Anime::default()
        }
    }

    #[test]
    fn finds_aired_episodes() {
        let now = 1_000_000;
        let entries = [
            entry(1, ListStatus::Watching),
            entry(2, ListStatus::Watching),
            entry(3, ListStatus::Watching),
            entry(4, ListStatus::PlanToWatch),
            entry(5, ListStatus::Watching),
            entry(6, ListStatus::Watching),
        ];
        let anime: HashMap<i32, Anime> = [
            anime(1, now - 60, 6),                // just aired
            anime(2, now + 60, 6),                // not yet
            anime(3, now - NOTIFY_WINDOW - 1, 2), // aired too long ago
            anime(4, now - 60, 1),                // not watching
            anime(5, now - 60, 9),                // already announced
            anime(6, 0, 0),                       // unknown
        ]
        .into_iter()
        .map(|a| (a.id, a))
        .collect();
        let notified = HashMap::from([(5, 10)]);
        assert_eq!(
            aired(&entries, &anime, &notified, now),
            [
                Aired {
                    anime_id: 1,
                    episode: 7,
                    announce: true
                },
                Aired {
                    anime_id: 3,
                    episode: 3,
                    announce: false
                },
                Aired {
                    anime_id: 5,
                    episode: 10,
                    announce: false
                },
            ]
        );
    }

    #[test]
    fn notification_texts() {
        assert_eq!(
            notification_text(&[("Frieren".into(), 7)]),
            ("Episode 7 of Frieren aired".into(), String::new())
        );
        assert_eq!(
            notification_text(&[("Frieren".into(), 7), ("Dandadan".into(), 3)]),
            (
                "2 new episodes aired".into(),
                "Frieren #7\nDandadan #3".into()
            )
        );
    }
}
