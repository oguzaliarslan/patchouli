//! Which episode to play (Taiga `track/play.cpp`).

use patchouli_core::{Anime, ListEntry, ListStatus};

/// The episode after the last watched one; episode 1 again once completed.
pub fn next_episode(anime: &Anime, entry: Option<&ListEntry>) -> i32 {
    let watched = entry.map_or(0, |e| e.watched_episodes);
    let next = watched + 1;
    if anime.episode_count > 0 && next > anime.episode_count {
        1
    } else {
        next
    }
}

/// A random episode up to the next unwatched one, so nothing is spoiled,
/// unless the anime is completed. `None` if there's nothing to pick.
pub fn random_episode(anime: &Anime, entry: Option<&ListEntry>) -> Option<i32> {
    let watched = entry.map_or(0, |e| e.watched_episodes);
    let completed = entry.is_some_and(|e| e.status == ListStatus::Completed);
    let mut max = if completed {
        anime.episode_count
    } else {
        watched + 1
    };
    if anime.episode_count > 0 {
        max = max.min(anime.episode_count);
    }
    if max < 1 {
        return None;
    }
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).ok()?;
    Some(1 + (u32::from_le_bytes(bytes) % max as u32) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anime(episodes: i32) -> Anime {
        Anime {
            episode_count: episodes,
            ..Anime::default()
        }
    }

    fn entry(watched: i32, status: ListStatus) -> ListEntry {
        ListEntry {
            watched_episodes: watched,
            status,
            ..ListEntry::default()
        }
    }

    #[test]
    fn next() {
        assert_eq!(next_episode(&anime(12), None), 1);
        assert_eq!(
            next_episode(&anime(12), Some(&entry(3, ListStatus::Watching))),
            4
        );
        assert_eq!(
            next_episode(&anime(12), Some(&entry(12, ListStatus::Completed))),
            1
        );
        assert_eq!(
            next_episode(&anime(0), Some(&entry(30, ListStatus::Watching))),
            31
        );
    }

    #[test]
    fn random_stays_in_range() {
        for _ in 0..50 {
            let n = random_episode(&anime(12), Some(&entry(3, ListStatus::Watching))).unwrap();
            assert!((1..=4).contains(&n));
            let n = random_episode(&anime(12), Some(&entry(12, ListStatus::Completed))).unwrap();
            assert!((1..=12).contains(&n));
        }
        // Completed with an unknown count: nothing to pick from.
        assert_eq!(
            random_episode(&anime(0), Some(&entry(5, ListStatus::Completed))),
            None
        );
    }
}
