//! Sync services and the offline update queue.
//!
//! Phase 2: implement a client per `tracker_core::ServiceId` and port Taiga
//! `src/sync/queue.cpp`.

/// AniList GraphQL documents, copied from Taiga.
pub mod gql {
    pub const DELETE_MEDIA_LIST_ENTRY: &str =
        include_str!("../gql/anilist/DeleteMediaListEntry.gql");
    pub const MEDIA: &str = include_str!("../gql/anilist/Media.gql");
    pub const MEDIA_FRAGMENT: &str = include_str!("../gql/anilist/MediaFragment.gql");
    pub const MEDIA_LIST_COLLECTION: &str = include_str!("../gql/anilist/MediaListCollection.gql");
    pub const MEDIA_LIST_FRAGMENT: &str = include_str!("../gql/anilist/MediaListFragment.gql");
    pub const MEDIA_SEARCH: &str = include_str!("../gql/anilist/MediaSearch.gql");
    pub const SAVE_MEDIA_LIST_ENTRY: &str = include_str!("../gql/anilist/SaveMediaListEntry.gql");
    pub const VIEWER: &str = include_str!("../gql/anilist/Viewer.gql");
}
