//! Whether watching an episode should update the list
//! (Taiga `track/update_decision.cpp`).

use serde::{Deserialize, Serialize};
use tracker_core::{Anime, ListEntry, ListStatus};
use tracker_recognition::Episode;

use crate::is_in_list;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Update automatically.
    #[default]
    Allow,
    /// Update only if the user agrees.
    Confirm,
    /// Don't update.
    Deny,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    #[default]
    None,
    InvalidEpisode,
    OutsideLibrary,
    AlreadyWatched,
    SkipsAhead,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateDecision {
    pub action: Action,
    pub reason: Reason,
}

/// The episode to record: the last of a range, or 1 for single-episode anime
/// without a number.
pub fn watched_episode_number(episode: &Episode, anime: &Anime) -> Option<i32> {
    match episode.episode_range() {
        Some((_, last)) if last > 0 => Some(last),
        _ if anime.episode_count == 1 => Some(1),
        _ => None,
    }
}

pub fn decide_update(
    episode: &Episode,
    anime: &Anime,
    entry: Option<&ListEntry>,
    outside_library: bool,
) -> UpdateDecision {
    let decision = |action, reason| UpdateDecision { action, reason };
    if outside_library {
        return decision(Action::Deny, Reason::OutsideLibrary);
    }
    let Some(number) = watched_episode_number(episode, anime)
        .filter(|&n| anime.episode_count <= 0 || n <= anime.episode_count)
    else {
        return decision(Action::Deny, Reason::InvalidEpisode);
    };
    let in_list = is_in_list(entry);
    let entry = entry.filter(|_| in_list);
    let completed = entry.is_some_and(|e| e.status == ListStatus::Completed && !e.rewatching);
    let watched = entry.map_or(0, |e| e.watched_episodes);
    if completed || number < watched || (number == watched && anime.episode_count != 1) {
        return decision(Action::Deny, Reason::AlreadyWatched);
    }
    let lowest = episode.episode_range().map_or(number, |(first, _)| first);
    if lowest > watched + 1 {
        return decision(Action::Confirm, Reason::SkipsAhead);
    }
    decision(Action::Allow, Reason::None)
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

    fn entry(watched: i32, status: ListStatus) -> ListEntry {
        ListEntry {
            anime_id: 1,
            watched_episodes: watched,
            status,
            ..ListEntry::default()
        }
    }

    fn episode(name: &str) -> Episode {
        Episode::parse(name)
    }

    fn decide(name: &str, episodes: i32, e: Option<&ListEntry>) -> (Action, Reason) {
        let d = decide_update(&episode(name), &anime(episodes), e, false);
        (d.action, d.reason)
    }

    #[test]
    fn next_episode_is_allowed() {
        let e = entry(3, ListStatus::Watching);
        assert_eq!(
            decide("Show - 04.mkv", 12, Some(&e)),
            (Action::Allow, Reason::None)
        );
        assert_eq!(
            decide("Show - 01.mkv", 12, None),
            (Action::Allow, Reason::None)
        );
    }

    #[test]
    fn watched_or_completed_is_denied() {
        let e = entry(3, ListStatus::Watching);
        assert_eq!(
            decide("Show - 03.mkv", 12, Some(&e)),
            (Action::Deny, Reason::AlreadyWatched)
        );
        assert_eq!(
            decide("Show - 02.mkv", 12, Some(&e)),
            (Action::Deny, Reason::AlreadyWatched)
        );
        let done = entry(12, ListStatus::Completed);
        assert_eq!(
            decide("Show - 05.mkv", 12, Some(&done)),
            (Action::Deny, Reason::AlreadyWatched)
        );
        // Rewatching a completed anime is fine.
        let rewatch = ListEntry {
            rewatching: true,
            ..entry(4, ListStatus::Completed)
        };
        assert_eq!(
            decide("Show - 05.mkv", 12, Some(&rewatch)),
            (Action::Allow, Reason::None)
        );
    }

    #[test]
    fn skipping_ahead_needs_confirmation() {
        let e = entry(3, ListStatus::Watching);
        assert_eq!(
            decide("Show - 06.mkv", 12, Some(&e)),
            (Action::Confirm, Reason::SkipsAhead)
        );
        // A range starting at the next episode is fine.
        assert_eq!(
            decide("Show - 04-06.mkv", 12, Some(&e)),
            (Action::Allow, Reason::None)
        );
    }

    #[test]
    fn invalid_numbers_and_movies() {
        assert_eq!(
            decide("Show - 13.mkv", 12, None),
            (Action::Deny, Reason::InvalidEpisode)
        );
        assert_eq!(
            decide("Show.mkv", 12, None),
            (Action::Deny, Reason::InvalidEpisode)
        );
        // Single-episode anime need no number, and can be rewatched once.
        assert_eq!(decide("Movie.mkv", 1, None), (Action::Allow, Reason::None));
        // Unknown episode counts accept any number, though skipping ahead
        // still needs confirmation.
        let e = entry(499, ListStatus::Watching);
        assert_eq!(
            decide("Show - 500.mkv", 0, Some(&e)),
            (Action::Allow, Reason::None)
        );
        assert_eq!(
            decide("Show - 500.mkv", 0, None),
            (Action::Confirm, Reason::SkipsAhead)
        );
    }

    #[test]
    fn outside_library_is_denied() {
        let d = decide_update(&episode("Show - 01.mkv"), &anime(12), None, true);
        assert_eq!((d.action, d.reason), (Action::Deny, Reason::OutsideLibrary));
    }
}
