//! Torrent RSS feeds (Taiga v1 `track/feed*.cpp`).
//!
//! A feed is fetched by the app, then [`Feed::parse`] reads it, applying
//! each site's quirks ([`source`]), and [`examine`] recognises every item,
//! categorises it, and runs the filters ([`filter`]) and the archive of items
//! already downloaded. The result is sorted: selected items first.

pub mod download;
pub mod filter;
pub mod item;
pub mod rss;
pub mod source;

use std::path::{Path, PathBuf};

use serde::Serialize;
use tracker_core::feed::Filter;
use tracker_recognition::{Episode, Recognizer};

pub use filter::Library;
pub use item::{Category, EpisodeInfo, FeedItem, State};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    /// The URL the feed was fetched from.
    pub url: String,
    pub title: String,
    pub items: Vec<FeedItem>,
}

impl Feed {
    pub fn parse(url: &str, text: &str) -> Result<Self, String> {
        let document = rss::parse_document(text)?;
        let link = if document.channel.link.is_empty() {
            url
        } else {
            &document.channel.link
        };
        let mut source = source::detect(link);
        if source == source::Source::Unknown {
            source = source::detect(url);
        }
        let items = document
            .items
            .into_iter()
            .map(|rss_item| {
                let mut item = FeedItem::from(rss_item);
                source::apply(source, &mut item);
                item.description = source::tidy_description(&item.description);
                let title = source::preprocess_title(source, &item.title);
                item.parsed = Episode::parse(&title);
                item
            })
            .collect();
        Ok(Self {
            url: url.to_owned(),
            title: document.channel.title,
            items,
        })
    }
}

/// Identifies, categorises and filters the items, then sorts them.
/// `filters` is `None` when filtering is turned off.
pub fn examine(
    feed: &mut Feed,
    recognizer: &Recognizer,
    library: &dyn Library,
    filters: Option<&[Filter]>,
    archive: &Archive,
) {
    for item in &mut feed.items {
        item.anime_id = recognizer.identify(&mut item.parsed);
        item.category = item::categorize(item);
        item.fill_episode_info();
        item.new_episode = item.anime_id.is_some_and(|id| {
            library.anime(id).is_some()
                && item.episode_high() > library.entry(id).map_or(0, |e| e.watched_episodes)
        });
    }
    if let Some(filters) = filters {
        // Preferences last, so they don't discard items other filters select.
        filter::run(filters, &mut feed.items, false, library);
        filter::run(filters, &mut feed.items, true, library);
    }
    for item in &mut feed.items {
        if !item.is_discarded() && archive.contains(&item.title) {
            item.state = State::DiscardedNormal;
        }
    }
    feed.items.sort_by_key(FeedItem::sort_key);
}

/// Titles of downloaded items, so they aren't selected again
/// (Taiga's `feed_history.xml`). Oldest first.
#[derive(Debug, Clone, Default)]
pub struct Archive {
    path: PathBuf,
    titles: Vec<String>,
}

impl Archive {
    /// Loads `path` (a JSON array of titles), or starts empty.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let titles = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, titles }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn contains(&self, title: &str) -> bool {
        self.titles.iter().any(|t| t == title)
    }

    pub fn len(&self) -> usize {
        self.titles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }

    pub fn add(&mut self, title: &str) {
        if !self.contains(title) {
            self.titles.push(title.to_owned());
        }
    }

    pub fn clear(&mut self) {
        self.titles.clear();
    }

    /// Saves the newest `max_count` titles (none if 0).
    pub fn save(&mut self, max_count: usize) -> std::io::Result<()> {
        let excess = self.titles.len().saturating_sub(max_count);
        self.titles.drain(..excess);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_vec(&self.titles)?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use tracker_core::anime::{Status, Titles};
    use tracker_core::feed::{
        Action, Condition, DiscardOption, Element, Match, Operator, default_filters,
    };
    use tracker_core::{Anime, ListEntry, ListStatus};
    use tracker_recognition::Relations;

    use super::*;

    struct TestLibrary {
        anime: HashMap<i32, Anime>,
        entries: HashMap<i32, ListEntry>,
        available: HashSet<(i32, i32)>,
    }

    impl Library for TestLibrary {
        fn anime(&self, id: i32) -> Option<&Anime> {
            self.anime.get(&id)
        }
        fn entry(&self, id: i32) -> Option<&ListEntry> {
            self.entries.get(&id)
        }
        fn is_episode_available(&self, id: i32, episode: i32) -> bool {
            self.available.contains(&(id, episode))
        }
    }

    fn anime(id: i32, title: &str, status: Status) -> Anime {
        Anime {
            id,
            episode_count: 12,
            status,
            titles: Titles {
                romaji: title.into(),
                ..Titles::default()
            },
            ..Anime::default()
        }
    }

    fn entry(anime_id: i32, status: ListStatus, watched: i32) -> ListEntry {
        ListEntry {
            anime_id,
            status,
            watched_episodes: watched,
            ..ListEntry::default()
        }
    }

    fn setup() -> (Recognizer, TestLibrary) {
        let all = vec![
            anime(1, "Watching Show", Status::Airing),
            anime(2, "Dropped Show", Status::Airing),
            anime(3, "Planned Show", Status::Airing),
            anime(4, "Unlisted Show", Status::Airing),
        ];
        let recognizer = Recognizer::new(all.clone(), vec![], Relations::default());
        let library = TestLibrary {
            anime: all.into_iter().map(|a| (a.id, a)).collect(),
            entries: [
                entry(1, ListStatus::Watching, 4),
                entry(2, ListStatus::Dropped, 0),
                entry(3, ListStatus::PlanToWatch, 0),
            ]
            .into_iter()
            .map(|e| (e.anime_id, e))
            .collect(),
            available: [(1, 5)].into_iter().collect(),
        };
        (recognizer, library)
    }

    fn feed(titles: &[&str]) -> Feed {
        let items: String = titles
            .iter()
            .map(|t| format!("<item><title>{t}</title><link>https://x/{t}</link></item>"))
            .collect();
        Feed::parse(
            "https://example.com/rss",
            &format!("<rss><channel><title>Test</title>{items}</channel></rss>"),
        )
        .unwrap()
    }

    fn state(feed: &Feed, title: &str) -> State {
        feed.items.iter().find(|i| i.title == title).unwrap().state
    }

    #[test]
    fn default_filters_select_and_discard() {
        let (recognizer, library) = setup();
        let mut archive = Archive::default();
        archive.add("[A] Watching Show - 07 [1080p].mkv");
        let mut feed = feed(&[
            "[A] Watching Show - 03 [1080p].mkv",
            "[A] Watching Show - 05 [1080p].mkv",
            "[A] Watching Show - 06 [1080p].mkv",
            "[A] Watching Show - 06 [720p].mkv",
            "[A] Watching Show - 07 [1080p].mkv",
            "[A] Dropped Show - 01 [1080p].mkv",
            "[A] Planned Show - 01 [1080p].mkv",
            "[A] Unlisted Show - 01 [1080p].mkv",
            "[A] Some Unknown Show - 01 [1080p].mkv",
        ]);
        examine(
            &mut feed,
            &recognizer,
            &library,
            Some(&default_filters()),
            &archive,
        );

        // Already watched (3), already downloaded (5): discarded.
        assert_eq!(
            state(&feed, "[A] Watching Show - 03 [1080p].mkv"),
            State::DiscardedNormal
        );
        assert_eq!(
            state(&feed, "[A] Watching Show - 05 [1080p].mkv"),
            State::DiscardedNormal
        );
        // 1080p preferred to 720p of the same episode and group.
        assert_eq!(
            state(&feed, "[A] Watching Show - 06 [1080p].mkv"),
            State::Selected
        );
        assert_eq!(
            state(&feed, "[A] Watching Show - 06 [720p].mkv"),
            State::DiscardedNormal
        );
        // In the archive.
        assert_eq!(
            state(&feed, "[A] Watching Show - 07 [1080p].mkv"),
            State::DiscardedNormal
        );
        assert_eq!(
            state(&feed, "[A] Dropped Show - 01 [1080p].mkv"),
            State::DiscardedNormal
        );
        assert_eq!(
            state(&feed, "[A] Planned Show - 01 [1080p].mkv"),
            State::Selected
        );
        assert_eq!(
            state(&feed, "[A] Unlisted Show - 01 [1080p].mkv"),
            State::DiscardedInactive
        );
        assert_eq!(
            state(&feed, "[A] Some Unknown Show - 01 [1080p].mkv"),
            State::DiscardedInactive
        );

        // Sorted: selected first.
        assert_eq!(feed.items[0].state, State::Selected);
        let six = feed
            .items
            .iter()
            .find(|i| i.title.contains("06 [1080p]"))
            .unwrap();
        assert!(six.new_episode);
        assert_eq!(six.episode.number, "6");
        assert_eq!(six.episode.group, "A");
        assert_eq!(six.anime_id, Some(1));
    }

    #[test]
    fn strong_fansub_preference() {
        let (recognizer, library) = setup();
        let filters = vec![Filter {
            name: "[Fansub] Watching Show".into(),
            action: Action::Prefer,
            anime_ids: vec![1],
            conditions: vec![Condition::new(
                Element::EpisodeGroup,
                Operator::Equals,
                "good",
            )],
            ..Filter::default()
        }];
        let mut feed = feed(&[
            "[Good] Watching Show - 06.mkv",
            "[Bad] Watching Show - 06.mkv",
        ]);
        examine(
            &mut feed,
            &recognizer,
            &library,
            Some(&filters),
            &Archive::default(),
        );
        assert_eq!(
            state(&feed, "[Good] Watching Show - 06.mkv"),
            State::Selected
        );
        assert_eq!(
            state(&feed, "[Bad] Watching Show - 06.mkv"),
            State::DiscardedNormal
        );
    }

    #[test]
    fn conditions_and_variables() {
        let (recognizer, library) = setup();
        let mut feed = feed(&["[A] Watching Show - 06 [1080p] v2.mkv"]);
        examine(&mut feed, &recognizer, &library, None, &Archive::default());
        let item = &feed.items[0];
        let check = |element, operator, value: &str| {
            filter::evaluate(&Condition::new(element, operator, value), item, &library)
        };
        assert!(check(
            Element::EpisodeNumber,
            Operator::GreaterThan,
            "%watched%"
        ));
        assert!(check(
            Element::EpisodeVideoResolution,
            Operator::GreaterThanOrEqual,
            "720p"
        ));
        assert!(check(Element::FileTitle, Operator::Contains, "WATCHING"));
        assert!(check(Element::FileTitle, Operator::NotContains, "720p"));
        assert!(check(Element::UserStatus, Operator::Equals, "1"));
        assert!(check(Element::MetaStatus, Operator::Equals, "2"));
        assert!(check(Element::EpisodeGroup, Operator::Equals, "a"));
        assert!(check(Element::FileCategory, Operator::Equals, "anime"));
        assert!(!check(
            Element::LocalEpisodeAvailable,
            Operator::Equals,
            "True"
        ));
        assert_eq!(
            filter::replace_variables("%title% %episode%/%total% %bogus%", item, &library),
            "Watching Show 6/12 %bogus%"
        );
        // Match any; hide option.
        let filters = vec![Filter {
            matching: Match::Any,
            option: DiscardOption::Hide,
            conditions: vec![
                Condition::new(Element::FileTitle, Operator::BeginsWith, "nope"),
                Condition::new(Element::EpisodeVersion, Operator::Equals, "2"),
            ],
            ..Filter::default()
        }];
        let mut feed2 = feed.clone();
        feed2.items[0].state = State::Blank;
        filter::run(&filters, &mut feed2.items, false, &library);
        assert_eq!(feed2.items[0].state, State::DiscardedHidden);
    }

    #[test]
    fn archive_keeps_newest() {
        let path =
            std::env::temp_dir().join(format!("tracker-archive-{}.json", std::process::id()));
        let mut archive = Archive::load(&path);
        for title in ["a", "b", "c", "a"] {
            archive.add(title);
        }
        archive.save(2).unwrap();
        let archive = Archive::load(&path);
        assert!(!archive.contains("a") && archive.contains("b") && archive.contains("c"));
        let _ = std::fs::remove_file(path);
    }
}
