use serde::{Deserialize, Serialize};
use tracker_core::SeasonName;
use tracker_core::anime::{Status, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchSort {
    Title,
    Duration,
    Score,
    Type,
    StartDate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchParams {
    pub text: String,
    pub year: Option<i32>,
    pub season: Option<SeasonName>,
    pub r#type: Option<Type>,
    pub status: Option<Status>,
    pub sort: Option<SearchSort>,
    pub descending: bool,
}

impl SearchParams {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}
