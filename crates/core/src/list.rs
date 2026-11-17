use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::FuzzyDate;

pub const SCORE_MAX: i32 = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum ListStatus {
    #[default]
    NotInList = 0,
    Watching = 1,
    Completed = 2,
    OnHold = 3,
    Dropped = 4,
    PlanToWatch = 5,
}

bitflags! {
    /// Fields of a list entry that changed locally and must be sent to the service.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Fields: u32 {
        const EPISODE         = 1 << 0;
        const SCORE           = 1 << 1;
        const STATUS          = 1 << 2;
        const REWATCHING      = 1 << 3;
        const REWATCHED_TIMES = 1 << 4;
        const DATE_STARTED    = 1 << 5;
        const DATE_COMPLETED  = 1 << 6;
        const NOTES           = 1 << 7;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
    pub id: i64,
    pub anime_id: i32,
    pub watched_episodes: i32,
    /// 0–100
    pub score: i32,
    pub status: ListStatus,
    pub is_private: bool,
    pub rewatched_times: i32,
    pub rewatching: bool,
    pub rewatching_ep: i32,
    pub date_started: FuzzyDate,
    pub date_completed: FuzzyDate,
    pub last_updated: i64,
    pub notes: String,
    pub pending_delete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_bits_match_taiga() {
        assert_eq!(Fields::NOTES.bits(), 128);
        assert_eq!((Fields::EPISODE | Fields::STATUS).bits(), 0b101);
    }
}
