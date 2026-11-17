use serde::{Deserialize, Serialize};

use crate::FuzzyDate;

pub const MAX_EPISODE_COUNT: i32 = 1900;
pub const UNKNOWN_EPISODE_COUNT: i32 = -1;
pub const UNKNOWN_EPISODE_LENGTH: i32 = -1;
pub const UNKNOWN_ID: i32 = 0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum AgeRating {
    #[default]
    Unknown = 0,
    G = 1,
    Pg = 2,
    Pg13 = 3,
    R17 = 4,
    R18 = 5,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Status {
    #[default]
    Unknown = 0,
    FinishedAiring = 1,
    Airing = 2,
    NotYetAired = 3,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Type {
    #[default]
    Unknown = 0,
    Tv = 1,
    Ova = 2,
    Movie = 3,
    Special = 4,
    Ona = 5,
    Music = 6,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TitleLanguage {
    #[default]
    Romaji,
    English,
    Native,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Titles {
    pub romaji: String,
    pub english: String,
    pub japanese: String,
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anime {
    pub id: i32,
    pub last_modified: i64,
    pub episode_count: i32,
    pub episode_length: i32,
    pub age_rating: AgeRating,
    pub status: Status,
    pub r#type: Type,
    pub date_started: FuzzyDate,
    pub date_finished: FuzzyDate,
    pub score: f32,
    pub popularity_rank: i32,
    pub image_url: String,
    pub slug: String,
    pub synopsis: String,
    pub trailer_id: String,
    pub titles: Titles,
    pub genres: Vec<String>,
    pub producers: Vec<String>,
    pub studios: Vec<String>,
    pub tags: Vec<String>,
    pub last_aired_episode: i32,
    pub next_episode_time: i64,
}

impl Default for Anime {
    fn default() -> Self {
        Self {
            id: UNKNOWN_ID,
            last_modified: 0,
            episode_count: UNKNOWN_EPISODE_COUNT,
            episode_length: UNKNOWN_EPISODE_LENGTH,
            age_rating: AgeRating::default(),
            status: Status::default(),
            r#type: Type::default(),
            date_started: FuzzyDate::default(),
            date_finished: FuzzyDate::default(),
            score: 0.0,
            popularity_rank: 0,
            image_url: String::new(),
            slug: String::new(),
            synopsis: String::new(),
            trailer_id: String::new(),
            titles: Titles::default(),
            genres: Vec::new(),
            producers: Vec::new(),
            studios: Vec::new(),
            tags: Vec::new(),
            last_aired_episode: 0,
            next_episode_time: 0,
        }
    }
}

/// Per-anime user overrides (`anime_settings` table).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimeSettings {
    pub id: i32,
    pub display_title: String,
    pub folder: String,
    pub synonyms: Vec<String>,
}
