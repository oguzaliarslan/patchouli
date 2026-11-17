//! Torrent feed filters (Taiga v1 `track/feed_filter.h`), stored in settings.
//!
//! Serialized with Taiga's shortcodes (`"discard"`, `"episode_group"`, `"ge"`,
//! …), so v1's `settings.xml` filters map one to one.

use serde::{Deserialize, Serialize};

use crate::anime::Status;
use crate::list::ListStatus;

/// What a condition looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    MetaId,
    MetaStatus,
    MetaType,
    MetaEpisodes,
    MetaDateStart,
    MetaDateEnd,
    /// The list entry's notes (Taiga's shortcode is `user_tags`).
    #[serde(rename = "user_tags")]
    UserNotes,
    UserStatus,
    LocalEpisodeAvailable,
    EpisodeTitle,
    EpisodeNumber,
    EpisodeVersion,
    EpisodeGroup,
    EpisodeVideoResolution,
    EpisodeVideoType,
    FileTitle,
    FileCategory,
    FileDescription,
    FileLink,
    FileSize,
}

impl Element {
    pub const ALL: [Self; 20] = [
        Self::MetaId,
        Self::MetaStatus,
        Self::MetaType,
        Self::MetaEpisodes,
        Self::MetaDateStart,
        Self::MetaDateEnd,
        Self::UserNotes,
        Self::UserStatus,
        Self::LocalEpisodeAvailable,
        Self::EpisodeTitle,
        Self::EpisodeNumber,
        Self::EpisodeVersion,
        Self::EpisodeGroup,
        Self::EpisodeVideoResolution,
        Self::EpisodeVideoType,
        Self::FileTitle,
        Self::FileCategory,
        Self::FileDescription,
        Self::FileLink,
        Self::FileSize,
    ];

    /// Taiga's shortcode, e.g. `episode_group`.
    pub fn shortcode(self) -> &'static str {
        match self {
            Self::MetaId => "meta_id",
            Self::MetaStatus => "meta_status",
            Self::MetaType => "meta_type",
            Self::MetaEpisodes => "meta_episodes",
            Self::MetaDateStart => "meta_date_start",
            Self::MetaDateEnd => "meta_date_end",
            Self::UserNotes => "user_tags",
            Self::UserStatus => "user_status",
            Self::LocalEpisodeAvailable => "local_episode_available",
            Self::EpisodeTitle => "episode_title",
            Self::EpisodeNumber => "episode_number",
            Self::EpisodeVersion => "episode_version",
            Self::EpisodeGroup => "episode_group",
            Self::EpisodeVideoResolution => "episode_video_resolution",
            Self::EpisodeVideoType => "episode_video_type",
            Self::FileTitle => "file_title",
            Self::FileCategory => "file_category",
            Self::FileDescription => "file_description",
            Self::FileLink => "file_link",
            Self::FileSize => "file_size",
        }
    }

    pub fn from_shortcode(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.shortcode().eq_ignore_ascii_case(code))
    }

    /// Compared as integers by the ordering operators.
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::FileSize
                | Self::MetaId
                | Self::MetaEpisodes
                | Self::MetaStatus
                | Self::MetaType
                | Self::UserStatus
                | Self::EpisodeNumber
                | Self::EpisodeVersion
                | Self::LocalEpisodeAvailable
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operator {
    #[serde(rename = "equals")]
    Equals,
    #[serde(rename = "notequals")]
    NotEquals,
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "ge")]
    GreaterThanOrEqual,
    #[serde(rename = "lt")]
    LessThan,
    #[serde(rename = "le")]
    LessThanOrEqual,
    #[serde(rename = "beginswith")]
    BeginsWith,
    #[serde(rename = "endswith")]
    EndsWith,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "notcontains")]
    NotContains,
}

impl Operator {
    pub const ALL: [Self; 10] = [
        Self::Equals,
        Self::NotEquals,
        Self::GreaterThan,
        Self::GreaterThanOrEqual,
        Self::LessThan,
        Self::LessThanOrEqual,
        Self::BeginsWith,
        Self::EndsWith,
        Self::Contains,
        Self::NotContains,
    ];

    pub fn shortcode(self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::NotEquals => "notequals",
            Self::GreaterThan => "gt",
            Self::GreaterThanOrEqual => "ge",
            Self::LessThan => "lt",
            Self::LessThanOrEqual => "le",
            Self::BeginsWith => "beginswith",
            Self::EndsWith => "endswith",
            Self::Contains => "contains",
            Self::NotContains => "notcontains",
        }
    }

    pub fn from_shortcode(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|o| o.shortcode().eq_ignore_ascii_case(code))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Discard,
    Select,
    /// Prefer matched items to similar ones (same anime, episode and group).
    Prefer,
}

/// How discarded items are shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscardOption {
    #[default]
    Default,
    Deactivate,
    Hide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub element: Element,
    pub operator: Operator,
    /// May contain `%watched%`, `%total%`, `%title%`, … (see
    /// `tracker_feed::filter::replace_variables`).
    pub value: String,
}

impl Condition {
    pub fn new(element: Element, operator: Operator, value: impl Into<String>) -> Self {
        Self {
            element,
            operator,
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    pub name: String,
    pub enabled: bool,
    pub action: Action,
    #[serde(rename = "match")]
    pub matching: Match,
    pub option: DiscardOption,
    /// Applies to these anime only; all anime if empty.
    pub anime_ids: Vec<i32>,
    pub conditions: Vec<Condition>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            name: "New filter".into(),
            enabled: true,
            action: Action::Discard,
            matching: Match::All,
            option: DiscardOption::Default,
            anime_ids: Vec::new(),
            conditions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub description: &'static str,
    /// Added to new installations.
    pub is_default: bool,
    pub filter: Filter,
}

/// Taiga's presets (`FeedFilterManager::InitializePresets`).
pub fn presets() -> Vec<Preset> {
    use Element as E;
    use Operator as O;
    let preset =
        |action, matching, is_default, option, name: &str, description, conditions| Preset {
            description,
            is_default,
            filter: Filter {
                name: name.into(),
                enabled: true,
                action,
                matching,
                option,
                anime_ids: Vec::new(),
                conditions,
            },
        };
    let bad = ["AVI", "DIVX", "LQ", "RMVB", "SD", "WMV", "XVID"]
        .into_iter()
        .map(|v| Condition::new(E::EpisodeVideoType, O::Contains, v))
        .collect();
    let int = |v: i32| v.to_string();
    vec![
        preset(
            Action::Discard,
            Match::All,
            false,
            DiscardOption::Default,
            "(Custom)",
            "Lets you create a custom filter from scratch",
            vec![],
        ),
        preset(
            Action::Prefer,
            Match::All,
            false,
            DiscardOption::Default,
            "[Fansub] Anime",
            "Lets you choose a fansub group for one or more anime",
            vec![Condition::new(
                E::EpisodeGroup,
                O::Equals,
                "TaigaSubs (change this)",
            )],
        ),
        preset(
            Action::Discard,
            Match::Any,
            false,
            DiscardOption::Default,
            "Discard bad video keywords",
            "Discards everything that is AVI, DIVX, LQ, RMVB, SD, WMV or XVID",
            bad,
        ),
        preset(
            Action::Prefer,
            Match::Any,
            false,
            DiscardOption::Default,
            "Prefer new versions",
            "Prefers v2 files and above when there are earlier releases of the same episode as well",
            vec![Condition::new(E::EpisodeVersion, O::GreaterThan, "1")],
        ),
        preset(
            Action::Select,
            Match::Any,
            true,
            DiscardOption::Default,
            "Select currently watching",
            "Selects files that belong to anime that you're currently watching",
            vec![Condition::new(
                E::UserStatus,
                O::Equals,
                int(ListStatus::Watching as i32),
            )],
        ),
        preset(
            Action::Select,
            Match::All,
            true,
            DiscardOption::Default,
            "Select airing anime in plan to watch",
            "Selects files that belong to an airing anime that you're planning to watch",
            vec![
                Condition::new(E::MetaStatus, O::Equals, int(Status::Airing as i32)),
                Condition::new(
                    E::UserStatus,
                    O::Equals,
                    int(ListStatus::PlanToWatch as i32),
                ),
            ],
        ),
        preset(
            Action::Discard,
            Match::All,
            true,
            DiscardOption::Default,
            "Discard dropped",
            "Discards files that belong to anime that you've dropped watching",
            vec![Condition::new(
                E::UserStatus,
                O::Equals,
                int(ListStatus::Dropped as i32),
            )],
        ),
        preset(
            Action::Discard,
            Match::Any,
            true,
            DiscardOption::Deactivate,
            "Discard and deactivate not-in-list anime",
            "Discards files that do not belong to any anime in your list",
            vec![Condition::new(
                E::UserStatus,
                O::Equals,
                int(ListStatus::NotInList as i32),
            )],
        ),
        preset(
            Action::Discard,
            Match::Any,
            true,
            DiscardOption::Default,
            "Discard watched and available episodes",
            "Discards episodes you've already watched or downloaded",
            vec![
                Condition::new(E::EpisodeNumber, O::LessThanOrEqual, "%watched%"),
                Condition::new(E::LocalEpisodeAvailable, O::Equals, "True"),
            ],
        ),
        preset(
            Action::Prefer,
            Match::Any,
            true,
            DiscardOption::Default,
            "Prefer high-resolution files",
            "Prefers 1080p files when there are other files of the same episode as well",
            vec![Condition::new(
                E::EpisodeVideoResolution,
                O::Equals,
                "1080p",
            )],
        ),
    ]
}

/// The filters a new installation starts with.
pub fn default_filters() -> Vec<Filter> {
    presets()
        .into_iter()
        .filter(|p| p.is_default)
        .map(|p| p.filter)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_taiga_shortcodes() {
        let filter = &default_filters()[4];
        let json = serde_json::to_value(filter).unwrap();
        assert_eq!(json["action"], "discard");
        assert_eq!(json["match"], "any");
        assert_eq!(json["conditions"][0]["element"], "episode_number");
        assert_eq!(json["conditions"][0]["operator"], "le");
        assert_eq!(serde_json::from_value::<Filter>(json).unwrap(), *filter);
        assert_eq!(
            Element::from_shortcode("USER_TAGS"),
            Some(Element::UserNotes)
        );
        for e in Element::ALL {
            assert_eq!(serde_json::to_value(e).unwrap(), e.shortcode());
        }
        for o in Operator::ALL {
            assert_eq!(serde_json::to_value(o).unwrap(), o.shortcode());
        }
    }

    #[test]
    fn six_default_filters() {
        assert_eq!(default_filters().len(), 6);
        assert_eq!(presets().len(), 10);
    }
}
