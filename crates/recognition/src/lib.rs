//! Episode recognition: which anime and episode a file name refers to.
//!
//! Ported from Taiga `track/recognition*.cpp`, `scanner.cpp` and `play.cpp`:
//!
//! 1. Parse the file name with Anitomy ([`Episode`]); without a title, take
//!    it from the parent folders ([`path`]).
//! 2. Normalise the title ([`normalize`]) and look it up ([`TitleIndex`]).
//! 3. Check the episode type and number against each candidate; if the number
//!    is out of range, try an `anime-relations` redirect ([`Relations`]).

mod episode;
mod index;
pub mod normalize;
pub mod path;
pub mod play;
mod relations;
pub mod scanner;

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use tracker_anitomy::ElementKind;
use tracker_core::{Anime, AnimeSettings};

pub use episode::Episode;
pub use index::{Match, TitleIndex};
pub use normalize::normalize;
pub use relations::{Redirection, Relations};

/// Episode types that are never list episodes.
const EXTRA_TYPES: [&str; 8] = [
    "op", "opening", "ncop", "ed", "ending", "nced", "preview", "pv",
];

/// Not an opening, ending or preview.
pub fn is_valid_episode_type(episode: &Episode) -> bool {
    !episode
        .values(ElementKind::Type)
        .any(|v| EXTRA_TYPES.iter().any(|t| t.eq_ignore_ascii_case(v)))
}

/// The episode number fits the anime's episode count.
pub fn is_valid_episode_number(episode: &Episode, anime: &Anime) -> bool {
    if anime.episode_count < 1 {
        return true; // unknown count, anything goes
    }
    let Some((_, last)) = episode.episode_range() else {
        // Single-episode anime need no number; neither do batches (folders).
        return anime.episode_count == 1 || !episode.contains(ElementKind::FileExtension);
    };
    // Episode 0 falls through to redirection.
    last != 0 && last <= anime.episode_count
}

/// Parses a file, taking the title from parent folders if the name has none.
pub fn parse_file(path: &Path, library_folders: &[String]) -> Episode {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut episode = Episode::parse(&name);
    if !episode.contains(ElementKind::Title) {
        let info = path::parse_parent_directories(path, library_folders);
        if !info.title.is_empty() {
            episode.add(ElementKind::Title, info.title);
        }
        if !info.season.is_empty() && !episode.contains(ElementKind::Season) {
            episode.add(ElementKind::Season, info.season);
        }
    }
    episode
}

/// Why a candidate was or wasn't chosen, for the recognition test tool.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: i32,
    pub weight: f32,
    pub known: bool,
    pub valid_number: bool,
    pub redirection: Option<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identification {
    pub anime_id: Option<i32>,
    pub normalized_title: String,
    pub valid_type: bool,
    pub candidates: Vec<Candidate>,
    /// The episode after any redirection.
    pub episode: Episode,
}

/// Everything needed to identify episodes: the anime, their titles and the
/// redirection rules for the current service.
#[derive(Debug, Clone, Default)]
pub struct Recognizer {
    anime: HashMap<i32, Anime>,
    index: TitleIndex,
    relations: Relations,
}

impl Recognizer {
    pub fn new(anime: Vec<Anime>, settings: Vec<AnimeSettings>, relations: Relations) -> Self {
        let settings: HashMap<i32, AnimeSettings> =
            settings.into_iter().map(|s| (s.id, s)).collect();
        let index = TitleIndex::new(&anime, &settings);
        Self {
            anime: anime.into_iter().map(|a| (a.id, a)).collect(),
            index,
            relations,
        }
    }

    pub fn anime(&self, id: i32) -> Option<&Anime> {
        self.anime.get(&id)
    }

    pub fn relations(&self) -> &Relations {
        &self.relations
    }

    /// The anime the episode belongs to. May change the episode number when
    /// redirected (e.g. episode 14 of season 1 is episode 1 of season 2).
    pub fn identify(&self, episode: &mut Episode) -> Option<i32> {
        let result = self.explain(episode.clone());
        *episode = result.episode;
        result.anime_id
    }

    /// Identifies an episode and reports how each candidate was judged.
    ///
    /// Anitomy separates the season from the title (`Title S2 - 05` has the
    /// title `Title`), so with a season above 1, `Title Season 2` is looked up
    /// first and the bare title only if that finds nothing. Taiga v2 looks up
    /// the bare title only, which matches such files to the first season.
    pub fn explain(&self, mut episode: Episode) -> Identification {
        let title = episode.title().to_owned();
        let season = episode
            .element(ElementKind::Season)
            .map(episode::to_int)
            .filter(|&s| s > 1);
        let mut lookups = Vec::new();
        if let Some(season) = season {
            lookups.push(normalize(&format!("{title} Season {season}")));
        }
        lookups.push(normalize(&title));
        let (normalized_title, matches) = lookups
            .iter()
            .map(|t| (t.clone(), self.index.find(t)))
            .find(|(_, m)| !m.is_empty())
            .unwrap_or_else(|| (lookups.last().cloned().unwrap_or_default(), Vec::new()));

        let valid_type = is_valid_episode_type(&episode);
        let mut candidates = Vec::new();
        let mut anime_id = None;
        for Match { id, weight } in matches {
            let anime = self.anime.get(&id);
            let valid_number = anime.is_some_and(|a| is_valid_episode_number(&episode, a));
            let mut candidate = Candidate {
                id,
                weight,
                known: anime.is_some(),
                valid_number,
                redirection: None,
            };
            if anime_id.is_none() && anime.is_some() && valid_type {
                if valid_number {
                    anime_id = Some(id);
                } else if let Some(range) = episode.episode_range()
                    && let Some(redirect) = self.relations.find_redirection(id, range)
                {
                    episode.set_episode_range(redirect.episodes);
                    candidate.redirection = Some(redirect);
                    anime_id = Some(redirect.id);
                }
            }
            candidates.push(candidate);
        }
        Identification {
            anime_id,
            normalized_title,
            valid_type,
            candidates,
            episode,
        }
    }
}

#[cfg(test)]
mod tests {
    use tracker_core::FuzzyDate;
    use tracker_core::anime::Titles;

    use super::*;

    fn anime(id: i32, romaji: &str, english: &str, episodes: i32) -> Anime {
        Anime {
            id,
            episode_count: episodes,
            date_started: FuzzyDate::new(2016, 10, 0),
            titles: Titles {
                romaji: romaji.into(),
                english: english.into(),
                ..Titles::default()
            },
            ..Anime::default()
        }
    }

    fn recognizer() -> Recognizer {
        let relations = Relations::parse(
            "::rules\n- 31646|11380|21366:23-44 -> 35180|13401|98478:1-22\n",
            tracker_core::ServiceId::AniList,
        );
        Recognizer::new(
            vec![
                anime(21366, "3-gatsu no Lion", "March Comes in Like a Lion", 22),
                anime(98478, "3-gatsu no Lion 2nd Season", "", 22),
                anime(1, "Shingeki no Kyojin", "Attack on Titan", 25),
                anime(
                    2,
                    "Shingeki no Kyojin Season 2",
                    "Attack on Titan Season 2",
                    12,
                ),
            ],
            vec![AnimeSettings {
                id: 1,
                synonyms: vec!["SnK".into()],
                ..AnimeSettings::default()
            }],
            relations,
        )
    }

    #[test]
    fn identifies_by_any_title() {
        let r = recognizer();
        let mut e = Episode::parse("[Group] Attack on Titan - 05 [1080p].mkv");
        assert_eq!(r.identify(&mut e), Some(1));
        let mut e = Episode::parse("[Group] Shingeki no Kyojin 2nd Season - 05.mkv");
        assert_eq!(r.identify(&mut e), Some(2));
        let mut e = Episode::parse("SnK - 03.mkv");
        assert_eq!(r.identify(&mut e), Some(1));
    }

    #[test]
    fn redirects_out_of_range_episodes() {
        let r = recognizer();
        let mut e = Episode::parse("[Group] 3-gatsu no Lion - 25 [720p].mkv");
        assert_eq!(r.identify(&mut e), Some(98478));
        assert_eq!(e.episode_range(), Some((3, 3)));
    }

    #[test]
    fn rejects_extras_and_invalid_numbers() {
        let r = recognizer();
        let mut e = Episode::parse("[Group] Shingeki no Kyojin - NCOP1.mkv");
        assert_eq!(r.identify(&mut e), None);
        let mut e = Episode::parse("[Group] Shingeki no Kyojin Season 2 - 13.mkv");
        assert_eq!(r.identify(&mut e), None);
        let mut e = Episode::parse("Unknown Show - 01.mkv");
        assert_eq!(r.identify(&mut e), None);
    }

    #[test]
    fn explains_candidates() {
        let r = recognizer();
        let result = r.explain(Episode::parse("Shingeki no Kyojin S2 - 13.mkv"));
        assert_eq!(result.normalized_title, "shingekinokyojin2");
        assert_eq!(result.anime_id, None);
        assert_eq!(result.candidates.len(), 1);
        assert!(!result.candidates[0].valid_number);
    }

    #[test]
    fn title_from_parent_folder() {
        let r = recognizer();
        let mut e = parse_file(
            Path::new("D:/Anime/Attack on Titan/07.mkv"),
            &["D:/Anime".into()],
        );
        assert_eq!(e.title(), "Attack on Titan");
        assert_eq!(r.identify(&mut e), Some(1));
    }
}
