//! Domain types shared by every crate.
//!
//! Ported from Taiga `src/media/*` and `src/sync/service.hpp`. Enum discriminants
//! are persisted in SQLite, so never renumber them.

/// Implements `from_i32` for a fieldless `#[repr(i32)]` enum, returning `None`
/// for unknown values.
macro_rules! int_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        impl $name {
            pub fn from_i32(value: i32) -> Option<Self> {
                $(if value == Self::$variant as i32 {
                    return Some(Self::$variant);
                })+
                None
            }
        }
    };
}

pub mod anime;
pub mod date;
pub mod feed;
pub mod history;
pub mod list;
pub mod queue;
pub mod season;
pub mod service;

pub use anime::{Anime, AnimeSettings, TitleLanguage};
pub use date::FuzzyDate;
pub use history::HistoryItem;
pub use list::{Fields, ListEntry, ListStatus};
pub use queue::QueueItem;
pub use season::{Season, SeasonName};
pub use service::ServiceId;
