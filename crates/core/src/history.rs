use serde::{Deserialize, Serialize};

/// A watched episode (`history` table).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    /// Row id, assigned by the database.
    pub id: i64,
    pub anime_id: i32,
    pub episode: i32,
    /// Seconds since the Unix epoch.
    pub time: i64,
}
