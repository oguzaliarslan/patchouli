//! List services and synchronisation.
//!
//! - [`anilist`] and [`myanimelist`]: API clients, wrapped by [`Client`].
//! - [`oauth`]: browser login through a loopback redirect server.
//! - [`credentials`] and [`accounts`]: where tokens and user names are kept.
//! - [`Synchronizer`]: local list edits, the offline update queue, and
//!   fetching the remote list (port of Taiga `sync/queue.cpp`, `sync/service.cpp`
//!   and `media/anime_list_utils.cpp`).
//!
//! Kitsu is not supported yet.

pub mod accounts;
pub mod anilist;
mod client;
pub mod credentials;
mod engine;
mod error;
pub mod http;
pub mod myanimelist;
pub mod oauth;
mod search;

pub use accounts::{Account, AccountsFile};
pub use client::{Client, RemoteList, SavedEntry, anime_page_url};
pub use credentials::{CredentialStore, Credentials, MemoryCredentialStore};
pub use engine::{SyncEvent, Synchronizer};
pub use error::{Error, Result};
pub use search::{SearchParams, SearchSort};

/// AniList GraphQL documents, copied from Taiga.
pub mod gql {
    pub const DELETE_MEDIA_LIST_ENTRY: &str =
        include_str!("../gql/anilist/DeleteMediaListEntry.gql");
    pub const MEDIA: &str = include_str!("../gql/anilist/Media.gql");
    pub const MEDIA_FRAGMENT: &str = include_str!("../gql/anilist/MediaFragment.gql");
    pub const MEDIA_IDS: &str = include_str!("../gql/anilist/MediaIds.gql");
    pub const MEDIA_LIST_COLLECTION: &str = include_str!("../gql/anilist/MediaListCollection.gql");
    pub const MEDIA_LIST_FRAGMENT: &str = include_str!("../gql/anilist/MediaListFragment.gql");
    pub const MEDIA_LIST_ID: &str = include_str!("../gql/anilist/MediaListId.gql");
    pub const MEDIA_SEARCH: &str = include_str!("../gql/anilist/MediaSearch.gql");
    pub const SAVE_MEDIA_LIST_ENTRY: &str = include_str!("../gql/anilist/SaveMediaListEntry.gql");
    pub const VIEWER: &str = include_str!("../gql/anilist/Viewer.gql");
}

/// Current time in seconds since the Unix epoch.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
