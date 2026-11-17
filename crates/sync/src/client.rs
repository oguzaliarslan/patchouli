use tracker_core::{Anime, Fields, ListEntry, ServiceId};

use crate::accounts::Account;
use crate::anilist::{self, AniList};
use crate::myanimelist::{self, MyAnimeList};
use crate::search::SearchParams;
use crate::{Error, Result};

/// A user's list as fetched from a service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteList {
    pub anime: Vec<Anime>,
    pub entries: Vec<ListEntry>,
}

/// What a service returned after saving an entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavedEntry {
    pub entry: Option<ListEntry>,
    /// AniList also returns the anime.
    pub anime: Option<Anime>,
}

/// A logged-in client for one service.
pub enum Client {
    AniList(AniList),
    MyAnimeList(MyAnimeList),
}

impl Client {
    pub fn service(&self) -> ServiceId {
        match self {
            Self::AniList(_) => ServiceId::AniList,
            Self::MyAnimeList(_) => ServiceId::MyAnimeList,
        }
    }

    /// The logged-in user.
    pub async fn user(&self) -> Result<Account> {
        match self {
            Self::AniList(c) => c.user().await,
            Self::MyAnimeList(c) => c.user().await,
        }
    }

    pub async fn fetch_list(&self, username: &str) -> Result<RemoteList> {
        match self {
            Self::AniList(c) => c.fetch_list(username).await,
            Self::MyAnimeList(c) => c.fetch_list(username).await,
        }
    }

    pub async fn save_entry(&self, entry: &ListEntry, dirty: Fields) -> Result<SavedEntry> {
        match self {
            Self::AniList(c) => c.save_entry(entry, dirty).await,
            Self::MyAnimeList(c) => c.save_entry(entry, dirty).await,
        }
    }

    pub async fn delete_entry(&self, entry: &ListEntry) -> Result<()> {
        match self {
            Self::AniList(c) => c.delete_entry(entry).await,
            Self::MyAnimeList(c) => c.delete_entry(entry).await,
        }
    }

    pub async fn fetch_anime(&self, id: i32) -> Result<Option<Anime>> {
        match self {
            Self::AniList(c) => c.fetch_anime(id).await,
            Self::MyAnimeList(c) => c.fetch_anime(id).await,
        }
    }

    pub async fn search(&self, params: &SearchParams) -> Result<Vec<Anime>> {
        match self {
            Self::AniList(c) => c.search(params).await,
            Self::MyAnimeList(c) => c.search(params).await,
        }
    }
}

pub fn anime_page_url(service: ServiceId, id: i32) -> Result<String> {
    match service {
        ServiceId::AniList => Ok(anilist::anime_page_url(id)),
        ServiceId::MyAnimeList => Ok(myanimelist::anime_page_url(id)),
        ServiceId::Kitsu => Err(Error::NotConfigured("Kitsu is not supported yet.".into())),
    }
}
