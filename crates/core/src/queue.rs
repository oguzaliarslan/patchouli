use serde::{Deserialize, Serialize};

use crate::Fields;

/// A list entry change waiting to be sent to the sync service (`queue` table).
///
/// There is at most one item per anime. Further changes are merged into `dirty`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub anime_id: i32,
    pub dirty: Fields,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub retry_count: i32,
    pub last_error: Option<String>,
}
