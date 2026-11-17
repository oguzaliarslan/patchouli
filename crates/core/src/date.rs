use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A date where any component may be unknown (0).
///
/// Ported from Taiga `base::FuzzyDate`. Stored and serialised as `YYYY-MM-DD`,
/// with unknown components written as zeros (e.g. `2024-04-00`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FuzzyDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl FuzzyDate {
    pub const fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// No component is known.
    pub fn is_empty(&self) -> bool {
        self.year == 0 && self.month == 0 && self.day == 0
    }

    /// Every component is known (Taiga's `operator bool`).
    pub fn is_complete(&self) -> bool {
        self.year != 0 && self.month != 0 && self.day != 0
    }

    /// Lenient parse matching Taiga: anything shorter than 10 characters is an
    /// empty date, and components that aren't numbers become 0.
    pub fn parse(s: &str) -> Self {
        fn component<T: FromStr + Default>(s: &str, range: std::ops::Range<usize>) -> T {
            s.get(range)
                .and_then(|p| p.parse().ok())
                .unwrap_or_default()
        }
        if s.len() < 10 {
            return Self::default();
        }
        Self {
            year: component(s, 0..4),
            month: component(s, 5..7),
            day: component(s, 8..10),
        }
    }
}

impl fmt::Display for FuzzyDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for FuzzyDate {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Unknown components sort after known ones, so undated items go last.
impl Ord for FuzzyDate {
    fn cmp(&self, other: &Self) -> Ordering {
        fn fuzzy<T: Ord + Default>(a: T, b: T) -> Ordering {
            let zero = T::default();
            if a == b {
                Ordering::Equal
            } else if a == zero {
                Ordering::Greater
            } else if b == zero {
                Ordering::Less
            } else {
                a.cmp(&b)
            }
        }
        fuzzy(self.year, other.year)
            .then_with(|| fuzzy(self.month, other.month))
            .then_with(|| fuzzy(self.day, other.day))
    }
}

impl PartialOrd for FuzzyDate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for FuzzyDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FuzzyDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_taiga_format() {
        for s in ["2024-04-07", "2024-04-00", "2024-00-00", "0000-00-00"] {
            assert_eq!(FuzzyDate::parse(s).to_string(), s);
        }
    }

    #[test]
    fn parse_is_lenient() {
        assert!(FuzzyDate::parse("").is_empty());
        assert!(FuzzyDate::parse("2024-04").is_empty());
        assert_eq!(FuzzyDate::parse("2024-xx-07"), FuzzyDate::new(2024, 0, 7));
    }

    #[test]
    fn unknown_components_sort_last() {
        let known = FuzzyDate::new(2020, 1, 1);
        let later = FuzzyDate::new(2021, 1, 1);
        let no_year = FuzzyDate::new(0, 1, 1);
        let no_day = FuzzyDate::new(2020, 1, 0);
        assert!(known < later);
        assert!(later < no_year);
        assert!(known < no_day);
        assert!(FuzzyDate::default() > later);
    }

    #[test]
    fn completeness() {
        assert!(FuzzyDate::new(2020, 1, 1).is_complete());
        assert!(!FuzzyDate::new(2020, 1, 0).is_complete());
    }
}
