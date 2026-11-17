//! Feed items and their categories (Taiga v1 `track/feed.cpp`).

use serde::Serialize;
use tracker_anitomy::ElementKind;
use tracker_core::feed::DiscardOption;
use tracker_recognition::Episode;

use crate::rss;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Blank,
    DiscardedNormal,
    DiscardedInactive,
    DiscardedHidden,
    Selected,
}

impl State {
    /// Selected first, then blank, then discarded (Taiga's sort order).
    fn priority(self) -> u8 {
        match self {
            Self::Selected => 0,
            Self::Blank => 1,
            Self::DiscardedNormal => 2,
            Self::DiscardedInactive => 3,
            Self::DiscardedHidden => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum Category {
    #[default]
    Anime,
    Batch,
    Other,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Self::Anime => "Anime",
            Self::Batch => "Batch",
            Self::Other => "Other",
        }
    }
}

/// What the title says about the episode, for display.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeInfo {
    pub title: String,
    /// `5`, `1-12`, or empty.
    pub number: String,
    pub version: i32,
    pub group: String,
    pub resolution: String,
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub description: String,
    pub category: Category,
    /// The feed's own category text.
    pub source_category: String,
    pub guid: String,
    #[serde(skip)]
    pub guid_is_permalink: bool,
    pub pub_date: String,
    /// Unix time from `pub_date`, 0 if unknown.
    pub pub_time: i64,
    pub info_link: String,
    pub magnet_link: String,
    pub file_size: u64,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub downloads: Option<u32>,
    pub state: State,
    pub anime_id: Option<i32>,
    /// Newer than the last watched episode.
    pub new_episode: bool,
    pub episode: EpisodeInfo,
    #[serde(skip)]
    pub enclosure_url: String,
    #[serde(skip)]
    pub enclosure_length: String,
    #[serde(skip)]
    pub namespace_elements: std::collections::BTreeMap<String, String>,
    #[serde(skip)]
    pub parsed: Episode,
}

impl From<rss::Item> for FeedItem {
    fn from(item: rss::Item) -> Self {
        let pub_time =
            chrono::DateTime::parse_from_rfc2822(item.pub_date.trim()).map_or(0, |t| t.timestamp());
        Self {
            title: item.title,
            link: item.link,
            description: item.description,
            source_category: item.category,
            guid: item.guid,
            guid_is_permalink: item.guid_is_permalink,
            pub_date: item.pub_date,
            pub_time,
            enclosure_url: item.enclosure_url,
            enclosure_length: item.enclosure_length,
            namespace_elements: item.namespace_elements,
            ..Self::default()
        }
    }
}

impl FeedItem {
    pub fn discard(&mut self, option: DiscardOption) {
        self.state = match option {
            DiscardOption::Default => State::DiscardedNormal,
            DiscardOption::Deactivate => State::DiscardedInactive,
            DiscardOption::Hide => State::DiscardedHidden,
        };
    }

    pub fn is_discarded(&self) -> bool {
        matches!(
            self.state,
            State::DiscardedNormal | State::DiscardedInactive | State::DiscardedHidden
        )
    }

    /// Same item: by permalink GUID, then link, then title.
    pub fn same_as(&self, other: &Self) -> bool {
        if self.guid_is_permalink
            && other.guid_is_permalink
            && !self.guid.is_empty()
            && self.guid == other.guid
        {
            return true;
        }
        if !self.link.is_empty() && self.link == other.link {
            return true;
        }
        !self.title.is_empty() && self.title == other.title
    }

    pub(crate) fn sort_key(&self) -> u8 {
        self.state.priority()
    }

    /// Highest episode number, or 0.
    pub fn episode_high(&self) -> i32 {
        self.parsed.episode_range().map_or(0, |(_, high)| high)
    }

    /// `release_version`, 1 if absent.
    pub fn episode_version(&self) -> i32 {
        self.parsed
            .element(ElementKind::ReleaseVersion)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1)
    }

    pub fn group(&self) -> &str {
        self.parsed
            .element(ElementKind::ReleaseGroup)
            .unwrap_or_default()
    }

    pub fn resolution(&self) -> &str {
        self.parsed
            .element(ElementKind::VideoResolution)
            .unwrap_or_default()
    }

    /// Video terms joined with spaces (`H264 AAC`).
    pub fn video_terms(&self) -> String {
        self.parsed
            .values(ElementKind::VideoTerm)
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(crate) fn fill_episode_info(&mut self) {
        let number = match self.parsed.episode_range() {
            Some((low, high)) if high > low => format!("{low}-{high}"),
            Some((low, _)) => low.to_string(),
            None => String::new(),
        };
        self.episode = EpisodeInfo {
            title: self.parsed.title().to_owned(),
            number,
            version: self.episode_version(),
            group: self.group().to_owned(),
            resolution: self.resolution().to_owned(),
            video: self.video_terms(),
        };
    }
}

/// Taiga's `GetTorrentCategory`.
pub fn categorize(item: &FeedItem) -> Category {
    if item.category != Category::Anime {
        return item.category;
    }
    if item.source_category.contains("Batch") {
        return Category::Batch;
    }
    let episode = &item.parsed;
    // A volume, or "Batch" in the release information.
    if episode.contains(ElementKind::Volume)
        || episode
            .values(ElementKind::ReleaseInformation)
            .any(|v| v.eq_ignore_ascii_case("batch"))
    {
        return Category::Batch;
    }
    if !tracker_recognition::is_valid_episode_type(episode) {
        return Category::Other;
    }
    if episode.values(ElementKind::Episode).count() > 1
        && episode
            .episode_range()
            .is_some_and(|(low, high)| high > low)
    {
        return Category::Batch;
    }
    Category::Anime
}

/// `720p`, `1280x720` or `720` → 720; anything else → 0.
pub fn resolution_height(value: &str) -> i32 {
    let value = value.trim();
    if value.len() > 6
        && let Some((width, height)) = value.split_once(['x', 'X'])
    {
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        return if digits(width) && digits(height) {
            height.parse().unwrap_or(0)
        } else {
            0
        };
    }
    if value.len() > 3
        && let Some(number) = value.strip_suffix(['p', 'P'])
    {
        return number.parse().unwrap_or(0);
    }
    let height = value.parse().unwrap_or(0);
    if matches!(height, 1080 | 720 | 480) {
        height
    } else {
        0
    }
}

/// `1.4 GiB` → bytes (Taiga's `ParseSizeString`). A bare number is bytes.
pub fn parse_size(value: &str) -> u64 {
    let value = value.trim();
    let split = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let multiplier: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "kb" => 1e3,
        "kib" => 1024.0,
        "mb" => 1e6,
        "mib" => 1024.0 * 1024.0,
        "gb" => 1e9,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        "tb" => 1e12,
        "tib" => 1024.0_f64.powi(4),
        _ => 1.0,
    };
    (number.parse::<f64>().unwrap_or(0.0) * multiplier) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> FeedItem {
        FeedItem {
            title: title.into(),
            parsed: Episode::parse(title),
            ..FeedItem::default()
        }
    }

    #[test]
    fn categories() {
        assert_eq!(
            categorize(&item("[Grp] Show - 05 [1080p].mkv")),
            Category::Anime
        );
        assert_eq!(
            categorize(&item("[Grp] Show - 01-12 [1080p]")),
            Category::Batch
        );
        assert_eq!(
            categorize(&item("[Grp] Show (Batch) [1080p]")),
            Category::Batch
        );
        assert_eq!(
            categorize(&item("[Grp] Show - NCOP [1080p].mkv")),
            Category::Other
        );
    }

    #[test]
    fn sizes_and_resolutions() {
        assert_eq!(parse_size("1.5 GiB"), 1_610_612_736);
        assert_eq!(parse_size("350 MB"), 350_000_000);
        assert_eq!(parse_size("1024"), 1024);
        assert_eq!(parse_size("junk"), 0);
        assert_eq!(resolution_height("1080p"), 1080);
        assert_eq!(resolution_height("1280x720"), 720);
        assert_eq!(resolution_height("480"), 480);
        assert_eq!(resolution_height("4K"), 0);
    }
}
