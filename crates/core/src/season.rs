//! Anime seasons, ported from Taiga `media/anime_season.cpp`.
//!
//! Winter covers December to February and belongs to the year of its January,
//! so December 2018 is Winter 2019.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::FuzzyDate;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(i32)]
pub enum SeasonName {
    #[default]
    Unknown = 0,
    Winter = 1,
    Spring = 2,
    Summer = 3,
    Fall = 4,
}

impl SeasonName {
    pub fn from_month(month: u8) -> Self {
        match month {
            12 | 1 | 2 => Self::Winter,
            3..=5 => Self::Spring,
            6..=8 => Self::Summer,
            9..=11 => Self::Fall,
            _ => Self::Unknown,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Winter => Self::Spring,
            Self::Spring => Self::Summer,
            Self::Summer => Self::Fall,
            Self::Fall => Self::Winter,
            Self::Unknown => Self::Unknown,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Winter => Self::Fall,
            Self::Spring => Self::Winter,
            Self::Summer => Self::Spring,
            Self::Fall => Self::Summer,
            Self::Unknown => Self::Unknown,
        }
    }

    /// First and last month of the season.
    fn months(self) -> Option<(u8, u8)> {
        match self {
            Self::Winter => Some((12, 2)),
            Self::Spring => Some((3, 5)),
            Self::Summer => Some((6, 8)),
            Self::Fall => Some((9, 11)),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Season {
    pub name: SeasonName,
    /// 0 if unknown.
    pub year: i32,
}

impl Season {
    pub const fn new(name: SeasonName, year: i32) -> Self {
        Self { name, year }
    }

    pub fn from_date(date: FuzzyDate) -> Self {
        let name = SeasonName::from_month(date.month);
        let mut year = i32::from(date.year);
        if year != 0 && date.month == 12 {
            year += 1;
        }
        Self { name, year }
    }

    /// Both name and year are known.
    pub fn is_valid(&self) -> bool {
        self.name != SeasonName::Unknown && self.year != 0
    }

    pub fn next(self) -> Self {
        let name = self.name.next();
        let year = if name == SeasonName::Winter {
            self.year + 1
        } else {
            self.year
        };
        Self { name, year }
    }

    pub fn prev(self) -> Self {
        let name = self.name.prev();
        let year = if name == SeasonName::Fall {
            self.year - 1
        } else {
            self.year
        };
        Self { name, year }
    }

    /// First and last day of the season, or `None` if it isn't valid.
    pub fn date_range(&self) -> Option<(FuzzyDate, FuzzyDate)> {
        if !self.is_valid() {
            return None;
        }
        let (first_month, last_month) = self.name.months()?;
        let first_year = if self.name == SeasonName::Winter {
            self.year - 1
        } else {
            self.year
        };
        let first = FuzzyDate::new(u16::try_from(first_year).ok()?, first_month, 1);
        let year = u16::try_from(self.year).ok()?;
        let last = FuzzyDate::new(year, last_month, days_in_month(self.year, last_month));
        Some((first, last))
    }
}

/// Unknown years and names sort last, as they are assumed to be in the future.
impl Ord for Season {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.year != other.year {
            return match (self.year, other.year) {
                (0, _) => Ordering::Greater,
                (_, 0) => Ordering::Less,
                (a, b) => a.cmp(&b),
            };
        }
        if self.name != other.name {
            return match (self.name, other.name) {
                (SeasonName::Unknown, _) => Ordering::Greater,
                (_, SeasonName::Unknown) => Ordering::Less,
                (a, b) => (a as i32).cmp(&(b as i32)),
            };
        }
        Ordering::Equal
    }
}

impl PartialOrd for Season {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn december_belongs_to_next_winter() {
        let season = Season::from_date(FuzzyDate::new(2018, 12, 15));
        assert_eq!(season, Season::new(SeasonName::Winter, 2019));
    }

    #[test]
    fn next_and_prev_wrap_years() {
        let fall = Season::new(SeasonName::Fall, 2024);
        assert_eq!(fall.next(), Season::new(SeasonName::Winter, 2025));
        assert_eq!(fall.next().prev(), fall);
    }

    #[test]
    fn date_ranges() {
        let winter = Season::new(SeasonName::Winter, 2024);
        assert_eq!(
            winter.date_range(),
            Some((FuzzyDate::new(2023, 12, 1), FuzzyDate::new(2024, 2, 29)))
        );
        let summer = Season::new(SeasonName::Summer, 2025);
        assert_eq!(
            summer.date_range(),
            Some((FuzzyDate::new(2025, 6, 1), FuzzyDate::new(2025, 8, 31)))
        );
        assert_eq!(Season::default().date_range(), None);
    }

    #[test]
    fn ordering() {
        let a = Season::new(SeasonName::Fall, 2023);
        let b = Season::new(SeasonName::Winter, 2024);
        let unknown_name = Season::new(SeasonName::Unknown, 2024);
        let unknown_year = Season::new(SeasonName::Spring, 0);
        assert!(a < b);
        assert!(b < unknown_name);
        assert!(unknown_name < unknown_year);
    }
}
