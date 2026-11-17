//! Applying filters to feed items (Taiga v1 `track/feed_filter.cpp` and
//! `feed_filter_manager.cpp`).

use std::cmp::Ordering;

use tracker_core::feed::{Action, Condition, Element, Filter, Match, Operator};
use tracker_core::{Anime, ListEntry, ListStatus};

use crate::item::{FeedItem, State, parse_size, resolution_height};

/// What filters know about the user's anime.
pub trait Library {
    fn anime(&self, id: i32) -> Option<&Anime>;
    fn entry(&self, id: i32) -> Option<&ListEntry>;
    /// The episode is in the library folders.
    fn is_episode_available(&self, id: i32, episode: i32) -> bool;
}

/// `0` is shown as `default` (Taiga's `TranslateNumber`).
fn number(value: i32, default: &str) -> String {
    if value > 0 {
        value.to_string()
    } else {
        default.to_owned()
    }
}

/// Replaces `%watched%`, `%total%`, `%title%`, `%episode%`, `%group%`, … in a
/// condition value (the variables of Taiga's `ReplaceVariables`).
pub fn replace_variables(value: &str, item: &FeedItem, library: &dyn Library) -> String {
    if !value.contains('%') {
        return value.to_owned();
    }
    let anime = item.anime_id.and_then(|id| library.anime(id));
    let entry = item.anime_id.and_then(|id| library.entry(id));
    let episode = &item.parsed;
    let lookup = |name: &str| -> Option<String> {
        Some(match name {
            "title" => {
                anime.map_or_else(|| episode.title().to_owned(), |a| a.titles.romaji.clone())
            }
            "watched" => number(entry.map_or(0, |e| e.watched_episodes), ""),
            "total" => number(anime.map_or(0, |a| a.episode_count), ""),
            "score" => number(entry.map_or(0, |e| e.score), ""),
            "id" => item.anime_id.map(|id| id.to_string()).unwrap_or_default(),
            "image" => anime.map(|a| a.image_url.clone()).unwrap_or_default(),
            "status" => (entry.map_or(ListStatus::NotInList, |e| e.status) as i32).to_string(),
            "rewatching" => i32::from(entry.is_some_and(|e| e.rewatching)).to_string(),
            "notes" => entry.map(|e| e.notes.clone()).unwrap_or_default(),
            "name" => episode
                .element(tracker_anitomy::ElementKind::EpisodeTitle)
                .unwrap_or_default()
                .to_owned(),
            "episode" => item.episode.number.trim_start_matches('0').to_owned(),
            "version" => item.episode_version().to_string(),
            "group" => item.group().to_owned(),
            "resolution" => item.resolution().to_owned(),
            "video" => item.video_terms(),
            "audio" => episode
                .values(tracker_anitomy::ElementKind::AudioTerm)
                .collect::<Vec<_>>()
                .join(" "),
            "checksum" => episode
                .element(tracker_anitomy::ElementKind::FileChecksum)
                .unwrap_or_default()
                .to_owned(),
            "file" => item.title.clone(),
            _ => return None,
        })
    };
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after
            .find('%')
            .and_then(|end| Some((end, lookup(&after[..end])?)))
        {
            Some((end, text)) => {
                out.push_str(&text);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The item's value for a condition's element, as text.
fn element_value(element: Element, item: &FeedItem, library: &dyn Library) -> String {
    let anime = item.anime_id.and_then(|id| library.anime(id));
    let entry = item.anime_id.and_then(|id| library.entry(id));
    match element {
        Element::FileTitle => item.title.clone(),
        Element::FileCategory => item.category.name().to_owned(),
        Element::FileDescription => item.description.clone(),
        Element::FileLink => item.link.clone(),
        Element::FileSize => item.file_size.to_string(),
        Element::MetaId => anime.map_or(0, |a| a.id).to_string(),
        Element::EpisodeTitle => item.parsed.title().to_owned(),
        Element::MetaDateStart => anime
            .map(|a| a.date_started.to_string())
            .unwrap_or_default(),
        Element::MetaDateEnd => anime
            .map(|a| a.date_finished.to_string())
            .unwrap_or_default(),
        Element::MetaEpisodes => anime
            .map(|a| a.episode_count.to_string())
            .unwrap_or_default(),
        Element::MetaStatus => anime.map_or(0, |a| a.status as i32).to_string(),
        Element::MetaType => anime.map_or(0, |a| a.r#type as i32).to_string(),
        Element::UserNotes => entry.map(|e| e.notes.clone()).unwrap_or_default(),
        Element::UserStatus => {
            let status = anime.and(entry).map_or(ListStatus::NotInList, |e| e.status);
            (status as i32).to_string()
        }
        Element::EpisodeNumber => match item.parsed.episode_range() {
            Some((_, high)) => high.to_string(),
            // No number: a single-episode anime or a batch of all of them.
            None => anime
                .map(|a| a.episode_count.to_string())
                .unwrap_or_default(),
        },
        Element::EpisodeVersion => item.episode_version().to_string(),
        Element::LocalEpisodeAvailable => match anime {
            Some(a) => {
                i32::from(library.is_episode_available(a.id, item.episode_high())).to_string()
            }
            None => String::new(),
        },
        Element::EpisodeGroup => item.group().to_owned(),
        Element::EpisodeVideoResolution => item.resolution().to_owned(),
        Element::EpisodeVideoType => item.video_terms(),
    }
}

fn compare<T: Ord>(a: T, b: T, op: Operator) -> bool {
    let ordering = a.cmp(&b);
    match op {
        Operator::NotEquals => ordering != Ordering::Equal,
        Operator::GreaterThan => ordering == Ordering::Greater,
        Operator::GreaterThanOrEqual => ordering != Ordering::Less,
        Operator::LessThan => ordering == Ordering::Less,
        Operator::LessThanOrEqual => ordering != Ordering::Greater,
        _ => ordering == Ordering::Equal,
    }
}

/// Qt-style `toInt`: 0 unless the whole text is a number.
fn to_int(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

pub fn evaluate(condition: &Condition, item: &FeedItem, library: &dyn Library) -> bool {
    let element = element_value(condition.element, item, library);
    let value = replace_variables(&condition.value, item, library);
    let op = condition.operator;
    match op {
        Operator::BeginsWith => element.starts_with(&value),
        Operator::EndsWith => element.ends_with(&value),
        Operator::Contains => element.to_lowercase().contains(&value.to_lowercase()),
        Operator::NotContains => !element.to_lowercase().contains(&value.to_lowercase()),
        _ => match condition.element {
            Element::FileSize => {
                compare(element.parse::<u64>().unwrap_or(0), parse_size(&value), op)
            }
            Element::EpisodeVideoResolution => {
                compare(resolution_height(&element), resolution_height(&value), op)
            }
            // Empty values compare as text (Taiga issue #639).
            e if e.is_numeric() && !element.is_empty() && !value.is_empty() => {
                if matches!(op, Operator::Equals | Operator::NotEquals)
                    && value.eq_ignore_ascii_case("true")
                {
                    compare(to_int(&element), 1, op)
                } else {
                    compare(to_int(&element), to_int(&value), op)
                }
            }
            _ => {
                if matches!(op, Operator::Equals | Operator::NotEquals) {
                    compare(element.to_lowercase() == value.to_lowercase(), true, op)
                } else {
                    compare(element.to_lowercase(), value.to_lowercase(), op)
                }
            }
        },
    }
}

fn matches(filter: &Filter, item: &FeedItem, library: &dyn Library) -> bool {
    match filter.matching {
        Match::All => filter.conditions.iter().all(|c| evaluate(c, item, library)),
        Match::Any => filter.conditions.iter().any(|c| evaluate(c, item, library)),
    }
}

/// Applies a filter to `items[index]`. Returns whether it had any effect.
pub fn apply(
    filter: &Filter,
    items: &mut [FeedItem],
    index: usize,
    recursive: bool,
    library: &dyn Library,
) -> bool {
    let item = &items[index];
    if !filter.enabled || item.is_discarded() {
        return false;
    }
    if !filter.anime_ids.is_empty() && !filter.anime_ids.contains(&item.anime_id.unwrap_or(0)) {
        return false;
    }
    let matched = matches(filter, item, library);
    match filter.action {
        Action::Discard => {
            if !matched {
                return false;
            }
            items[index].discard(filter.option);
        }
        Action::Select => {
            if !matched {
                return false;
            }
            items[index].state = State::Selected;
        }
        Action::Prefer if recursive => {
            // Limited to some anime: a strong preference.
            if !filter.anime_ids.is_empty() {
                if matched {
                    items[index].state = State::Selected;
                } else {
                    items[index].discard(filter.option);
                }
            } else if !matched || !apply_preference(filter, items, index, library) {
                return false;
            }
        }
        // Checking the items similar to a preferred one.
        Action::Prefer => {
            if matched {
                return false;
            }
            items[index].discard(filter.option);
        }
    }
    true
}

/// Discards the items similar to `items[index]` (same anime, episode and
/// group, unless the filter's conditions are about those) that don't match.
fn apply_preference(
    filter: &Filter,
    items: &mut [FeedItem],
    index: usize,
    library: &dyn Library,
) -> bool {
    let uses = |element| filter.conditions.iter().any(|c| c.element == element);
    let (by_title, by_id, by_number, by_group) = (
        uses(Element::EpisodeTitle),
        uses(Element::MetaId),
        uses(Element::EpisodeNumber),
        uses(Element::EpisodeGroup),
    );
    let mut applied = false;
    for other in 0..items.len() {
        let (a, b) = (&items[index], &items[other]);
        if other == index || b.is_discarded() || b.same_as(a) {
            continue;
        }
        let known = |id: Option<i32>| id.is_some_and(|id| id > 0);
        if !known(a.anime_id) && !known(b.anime_id) {
            if !by_title && !a.parsed.title().eq_ignore_ascii_case(b.parsed.title()) {
                continue;
            }
        } else if !by_id && a.anime_id != b.anime_id {
            continue;
        }
        if !by_number && a.parsed.episode_range() != b.parsed.episode_range() {
            continue;
        }
        if !by_group && !a.group().eq_ignore_ascii_case(b.group()) {
            continue;
        }
        applied |= apply(filter, items, other, false, library);
    }
    applied
}

/// Runs the select and discard filters, or the preference filters.
pub fn run(filters: &[Filter], items: &mut [FeedItem], preferences: bool, library: &dyn Library) {
    for index in 0..items.len() {
        for filter in filters {
            if preferences == (filter.action == Action::Prefer) {
                apply(filter, items, index, true, library);
            }
        }
    }
}

/// A short description of a condition, e.g. `Episode number is less than or
/// equal to "%watched%"` (Taiga's `TranslateCondition`).
pub fn describe(condition: &Condition) -> String {
    let element = match condition.element {
        Element::FileTitle => "File name",
        Element::FileCategory => "File category",
        Element::FileDescription => "File description",
        Element::FileLink => "File link",
        Element::FileSize => "File size",
        Element::MetaId => "Anime ID",
        Element::EpisodeTitle => "Episode title",
        Element::MetaDateStart => "Anime date started",
        Element::MetaDateEnd => "Anime date ended",
        Element::MetaEpisodes => "Anime episode count",
        Element::MetaStatus => "Anime airing status",
        Element::MetaType => "Anime type",
        Element::UserNotes => "Anime notes",
        Element::UserStatus => "Anime watching status",
        Element::EpisodeNumber => "Episode number",
        Element::EpisodeVersion => "Episode version",
        Element::LocalEpisodeAvailable => "Episode availability",
        Element::EpisodeGroup => "Episode fansub group",
        Element::EpisodeVideoResolution => "Episode video resolution",
        Element::EpisodeVideoType => "Episode video type",
    };
    let operator = match condition.operator {
        Operator::Equals => "is",
        Operator::NotEquals => "is not",
        Operator::GreaterThan => "is greater than",
        Operator::GreaterThanOrEqual => "is greater than or equal to",
        Operator::LessThan => "is less than",
        Operator::LessThanOrEqual => "is less than or equal to",
        Operator::BeginsWith => "begins with",
        Operator::EndsWith => "ends with",
        Operator::Contains => "contains",
        Operator::NotContains => "does not contain",
    };
    format!("{element} {operator} \"{}\"", condition.value)
}
