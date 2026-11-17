//! Domain types shared by every crate.
//!
//! Ported from Taiga `src/media/anime.hpp`, `anime_list.hpp` and `anime_settings.hpp`.
//! Enum discriminants are persisted in SQLite, so never renumber them.

pub mod anime;
pub mod date;
pub mod list;

pub use anime::{Anime, AnimeSettings};
pub use date::FuzzyDate;
pub use list::ListEntry;
