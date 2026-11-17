//! Anime video filename parser, ported from erengy/anitomy (MPL-2.0).
//!
//! Phase 4: port the parser and pass anitomy's `test/data.json` parity suite.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub kind: ElementKind,
    pub value: String,
    /// Byte offset in the input.
    pub position: usize,
}
