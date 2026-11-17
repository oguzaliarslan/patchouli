use serde::{Deserialize, Serialize};

/// A list service. Serialised with Taiga's slugs (`anilist`, `kitsu`, `myanimelist`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceId {
    MyAnimeList,
    Kitsu,
    #[default]
    AniList,
}

impl ServiceId {
    pub const ALL: [Self; 3] = [Self::MyAnimeList, Self::Kitsu, Self::AniList];

    pub fn slug(self) -> &'static str {
        match self {
            Self::MyAnimeList => "myanimelist",
            Self::Kitsu => "kitsu",
            Self::AniList => "anilist",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.slug() == slug)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::MyAnimeList => "MyAnimeList",
            Self::Kitsu => "Kitsu",
            Self::AniList => "AniList",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_uses_slugs() {
        for id in ServiceId::ALL {
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.slug()));
            assert_eq!(ServiceId::from_slug(id.slug()), Some(id));
        }
    }
}
