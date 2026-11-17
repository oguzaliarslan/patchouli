//! Imports user data from Taiga (Taiga `src/compat/*` for v1, plus a reader
//! for Taiga v2's own files).
//!
//! Both versions are read into [`ImportData`]: the anime, list, per-anime
//! settings, history and unsent changes of one service, and settings in
//! Taiga v2's `settings.json` keys. Taiga's files are only read.
//!
//! Login tokens are not imported: they were issued to Taiga's application, so
//! the user logs in again.

pub mod v1;
pub mod v2;
pub mod xml;

use std::path::{Path, PathBuf};

use patchouli_core::{Anime, AnimeSettings, HistoryItem, ListEntry, QueueItem, ServiceId};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportData {
    pub service: Option<ServiceId>,
    pub username: Option<String>,
    pub anime: Vec<Anime>,
    pub entries: Vec<ListEntry>,
    pub anime_settings: Vec<AnimeSettings>,
    pub history: Vec<HistoryItem>,
    pub queue: Vec<QueueItem>,
    /// Settings in Taiga v2's keys (e.g. `track.update.delay`).
    pub settings: Map<String, Value>,
    /// Problems that didn't stop the import (e.g. a missing file).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TaigaVersion {
    V1,
    V2,
}

/// A Taiga data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub version: TaigaVersion,
    /// The `data` folder.
    pub path: PathBuf,
}

/// Where Taiga keeps its data when installed (not portable): v2 under
/// `%APPDATA%\erengy\taiga\data`, v1 under `%APPDATA%\Taiga\data`.
pub fn default_locations() -> Vec<Found> {
    let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        appdata.join("erengy").join("taiga").join("data"),
        appdata.join("Taiga").join("data"),
    ]
    .iter()
    .filter_map(|p| locate(p))
    .collect()
}

/// Recognises a Taiga data folder, or a folder that contains one (e.g. the
/// folder of a portable Taiga).
pub fn locate(path: &Path) -> Option<Found> {
    let candidates = [path.to_path_buf(), path.join("data")];
    candidates.into_iter().find_map(|dir| {
        if dir.join(patchouli_db::FILE_NAME).is_file() {
            Some(Found {
                version: TaigaVersion::V2,
                path: dir,
            })
        } else if dir.join("settings.xml").is_file() || dir.join("db").join("anime.xml").is_file() {
            Some(Found {
                version: TaigaVersion::V1,
                path: dir,
            })
        } else {
            None
        }
    })
}

pub fn read(found: &Found) -> Result<ImportData, String> {
    match found.version {
        TaigaVersion::V1 => v1::read(&found.path),
        TaigaVersion::V2 => v2::read(&found.path),
    }
}

/// Reads a text file, without a UTF-8 byte order mark.
fn read_text(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}
