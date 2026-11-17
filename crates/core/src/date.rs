use serde::{Deserialize, Serialize};

/// A date where any component may be unknown (0).
///
/// TODO(phase 1): match the string format of Taiga's `base::FuzzyDate::toString`
/// (`src/base/chrono.cpp`) so existing databases import correctly.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct FuzzyDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl FuzzyDate {
    pub fn is_empty(&self) -> bool {
        self.year == 0 && self.month == 0 && self.day == 0
    }
}
