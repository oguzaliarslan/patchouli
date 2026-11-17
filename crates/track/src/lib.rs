//! What's playing, and when to update the list.
//!
//! - [`media`]: picks the episode out of what detection found
//!   (Taiga `track/media.cpp`).
//! - [`decision`]: whether watching an episode should update the list
//!   (`track/update_decision.cpp`).
//! - [`session`]: the countdown / confirm / commit state machine
//!   (`track/update_session.cpp`), driven by one-second ticks so it can be
//!   tested without real time.
//! - [`entry_with_episode_watched`]: the list entry after watching an episode
//!   (`media/anime_list_utils.cpp`).

pub mod decision;
pub mod media;
pub mod session;

use tracker_core::{Anime, FuzzyDate, ListEntry, ListStatus};

pub use decision::{Action, Reason, UpdateDecision, decide_update, watched_episode_number};
pub use session::{Effect, Phase, UpdateSession, UpdateSettings, UpdateState, UpdateTrigger};

/// Today in local time.
pub fn today() -> FuzzyDate {
    use chrono::Datelike;
    let date = chrono::Local::now().date_naive();
    FuzzyDate::new(date.year() as u16, date.month() as u8, date.day() as u8)
}

/// In the list and not waiting to be removed.
pub fn is_in_list(entry: Option<&ListEntry>) -> bool {
    entry.is_some_and(|e| e.status != ListStatus::NotInList && !e.pending_delete)
}

/// The entry after watching `episode`: sets the start date on episode 1, and
/// completes the anime (status, date, rewatch count) on the last episode.
pub fn entry_with_episode_watched(
    anime: &Anime,
    entry: Option<&ListEntry>,
    episode: i32,
    today: FuzzyDate,
) -> ListEntry {
    let mut updated = match entry {
        Some(e) if is_in_list(Some(e)) => e.clone(),
        _ => ListEntry {
            anime_id: anime.id,
            ..ListEntry::default()
        },
    };
    let is_final = anime.episode_count > 0 && episode == anime.episode_count;
    updated.watched_episodes = episode;
    if episode == 1 && !updated.date_started.is_complete() {
        updated.date_started = today;
    }
    if is_final && !updated.date_completed.is_complete() {
        updated.date_completed = today;
    }
    if updated.rewatching && is_final {
        updated.rewatching = false;
        updated.rewatched_times += 1;
    }
    if is_final {
        updated.status = ListStatus::Completed;
    } else if !updated.rewatching {
        updated.status = ListStatus::Watching;
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anime(episodes: i32) -> Anime {
        Anime {
            id: 1,
            episode_count: episodes,
            ..Anime::default()
        }
    }

    const TODAY: FuzzyDate = FuzzyDate::new(2026, 11, 18);

    #[test]
    fn first_episode_starts_the_anime() {
        let entry = entry_with_episode_watched(&anime(12), None, 1, TODAY);
        assert_eq!(entry.status, ListStatus::Watching);
        assert_eq!(entry.date_started, TODAY);
        assert!(entry.date_completed.is_empty());
    }

    #[test]
    fn last_episode_completes_it() {
        let watching = ListEntry {
            anime_id: 1,
            watched_episodes: 11,
            status: ListStatus::Watching,
            rewatching: true,
            ..ListEntry::default()
        };
        let entry = entry_with_episode_watched(&anime(12), Some(&watching), 12, TODAY);
        assert_eq!(entry.status, ListStatus::Completed);
        assert_eq!(entry.date_completed, TODAY);
        assert!(!entry.rewatching);
        assert_eq!(entry.rewatched_times, 1);
    }

    #[test]
    fn rewatching_keeps_its_status() {
        let completed = ListEntry {
            anime_id: 1,
            watched_episodes: 3,
            status: ListStatus::Completed,
            rewatching: true,
            ..ListEntry::default()
        };
        let entry = entry_with_episode_watched(&anime(12), Some(&completed), 4, TODAY);
        assert_eq!(entry.status, ListStatus::Completed);
        assert_eq!(entry.watched_episodes, 4);
    }
}
