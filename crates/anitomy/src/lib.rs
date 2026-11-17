//! Anime video filename parser, ported from [erengy/anitomy] (MPL-2.0).
//!
//! ```
//! use tracker_anitomy::{parse, ElementKind};
//!
//! let elements = parse("[TaigaSubs]_Toradora!_(2008)_-_01v2_-_Tiger_and_Dragon_[1280x720_H.264_FLAC][1234ABCD].mkv");
//! let title = elements.iter().find(|e| e.kind == ElementKind::Title).unwrap();
//! assert_eq!(title.value, "Toradora!");
//! ```
//!
//! The port follows the C++ structure closely (tokenizer, keyword table, one
//! function per element) so behaviour can be compared line by line. It is
//! checked against output recorded from the C++ library (`tests/golden.rs`).
//!
//! [erengy/anitomy]: https://github.com/erengy/anitomy

mod chars;
mod keyword;
mod parser;
mod token;
mod value;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    AudioTerm,
    Device,
    Episode,
    EpisodeTitle,
    FileChecksum,
    FileExtension,
    Language,
    Other,
    Part,
    ReleaseGroup,
    ReleaseInformation,
    ReleaseVersion,
    Season,
    Source,
    Subtitles,
    Title,
    Type,
    VideoResolution,
    VideoTerm,
    Volume,
    Year,
}

impl ElementKind {
    /// Snake-case name, as in Anitomy's JSON output (e.g. `release_group`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AudioTerm => "audio_term",
            Self::Device => "device",
            Self::Episode => "episode",
            Self::EpisodeTitle => "episode_title",
            Self::FileChecksum => "file_checksum",
            Self::FileExtension => "file_extension",
            Self::Language => "language",
            Self::Other => "other",
            Self::Part => "part",
            Self::ReleaseGroup => "release_group",
            Self::ReleaseInformation => "release_information",
            Self::ReleaseVersion => "release_version",
            Self::Season => "season",
            Self::Source => "source",
            Self::Subtitles => "subtitles",
            Self::Title => "title",
            Self::Type => "type",
            Self::VideoResolution => "video_resolution",
            Self::VideoTerm => "video_term",
            Self::Volume => "volume",
            Self::Year => "year",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub kind: ElementKind,
    pub value: String,
    /// Byte offset in the input.
    pub position: usize,
}

impl Element {
    pub(crate) fn new(kind: ElementKind, value: impl Into<String>, position: usize) -> Self {
        Self {
            kind,
            value: value.into(),
            position,
        }
    }
}

/// Which elements to look for. Everything is on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub parse_episode: bool,
    pub parse_episode_title: bool,
    pub parse_file_checksum: bool,
    pub parse_file_extension: bool,
    pub parse_part: bool,
    pub parse_release_group: bool,
    pub parse_season: bool,
    pub parse_title: bool,
    pub parse_video_resolution: bool,
    pub parse_year: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            parse_episode: true,
            parse_episode_title: true,
            parse_file_checksum: true,
            parse_file_extension: true,
            parse_part: true,
            parse_release_group: true,
            parse_season: true,
            parse_title: true,
            parse_video_resolution: true,
            parse_year: true,
        }
    }
}

/// Parses a filename (or other release title) into elements, sorted by position.
pub fn parse(input: &str) -> Vec<Element> {
    parse_with(input, &Options::default())
}

pub fn parse_with(input: &str, options: &Options) -> Vec<Element> {
    let mut tokens = token::tokenize(input);
    parser::parse(&mut tokens, options)
}
