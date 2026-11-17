//! Finding episodes and folders in library folders (Taiga `track/scanner.cpp`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tracker_anitomy::ElementKind;
use walkdir::WalkDir;

use crate::episode::to_int;
use crate::path::comparable;
use crate::{Episode, Recognizer, parse_file};

fn contains_episode(episode: &Episode, number: i32) -> bool {
    let numbers: Vec<i32> = episode.values(ElementKind::Episode).map(to_int).collect();
    match (numbers.iter().min(), numbers.iter().max()) {
        (Some(&low), Some(&high)) => low <= number && number <= high,
        _ => false,
    }
}

/// The first video file under `root` that is episode `number` of `anime_id`.
pub fn find_episode(
    root: &Path,
    anime_id: i32,
    number: i32,
    recognizer: &Recognizer,
    library_folders: &[String],
) -> Option<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .find_map(|entry| {
            let mut episode = parse_file(entry.path(), library_folders);
            if !episode.is_video_file() || !contains_episode(&episode, number) {
                return None;
            }
            (recognizer.identify(&mut episode) == Some(anime_id)).then(|| entry.into_path())
        })
}

/// The first folder under `root` whose name is recognised as `anime_id`.
pub fn find_folder(root: &Path, anime_id: i32, recognizer: &Recognizer) -> Option<PathBuf> {
    WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut episode = Episode::parse(&name);
            (recognizer.identify(&mut episode) == Some(anime_id)).then(|| entry.into_path())
        })
}

/// `path` is inside one of the library folders.
pub fn is_inside_library_folders(path: &Path, library_folders: &[String]) -> bool {
    let path = comparable(path);
    library_folders.iter().any(|folder| {
        let root = format!("{}/", comparable(Path::new(folder)));
        path.starts_with(&root)
    })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub path: String,
    /// Episode numbers after redirection (empty for movies and batches).
    pub episodes: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAnime {
    pub anime_id: i32,
    pub files: Vec<LibraryFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScan {
    pub anime: Vec<LibraryAnime>,
    /// Video files that couldn't be matched to an anime.
    pub unrecognized: Vec<String>,
    pub files_scanned: usize,
}

/// Recognises every video file in the library folders.
pub fn scan_library(library_folders: &[String], recognizer: &Recognizer) -> LibraryScan {
    let mut by_anime: BTreeMap<i32, Vec<LibraryFile>> = BTreeMap::new();
    let mut scan = LibraryScan::default();
    for folder in library_folders {
        for entry in WalkDir::new(folder)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let mut episode = parse_file(entry.path(), library_folders);
            if !episode.is_video_file() {
                continue;
            }
            scan.files_scanned += 1;
            let path = entry.path().display().to_string();
            match recognizer.identify(&mut episode) {
                Some(id) => {
                    let episodes = match episode.episode_range() {
                        Some((first, last)) if first > 0 => (first..=last.max(first)).collect(),
                        _ => Vec::new(),
                    };
                    by_anime
                        .entry(id)
                        .or_default()
                        .push(LibraryFile { path, episodes });
                }
                None => scan.unrecognized.push(path),
            }
        }
    }
    scan.anime = by_anime
        .into_iter()
        .map(|(anime_id, mut files)| {
            files.sort_by(|a, b| {
                a.episodes
                    .first()
                    .cmp(&b.episodes.first())
                    .then(a.path.cmp(&b.path))
            });
            LibraryAnime { anime_id, files }
        })
        .collect();
    scan.unrecognized.sort();
    scan
}

#[cfg(test)]
mod tests {
    use tracker_core::Anime;
    use tracker_core::anime::Titles;

    use super::*;
    use crate::Relations;

    #[test]
    fn library_paths() {
        let folders = vec!["D:\\Anime".to_owned()];
        assert!(is_inside_library_folders(
            Path::new("d:/anime/Show/01.mkv"),
            &folders
        ));
        assert!(!is_inside_library_folders(
            Path::new("D:/Animes/01.mkv"),
            &folders
        ));
        assert!(!is_inside_library_folders(Path::new("D:/Anime"), &folders));
    }

    #[test]
    fn finds_episodes_and_folders_on_disk() {
        let root = std::env::temp_dir().join(format!("tracker-scan-{}", std::process::id()));
        let show = root.join("[Group] Sample Show [1080p]");
        std::fs::create_dir_all(&show).unwrap();
        for name in [
            "Sample Show - 01.mkv",
            "Sample Show - 02.mkv",
            "notes.txt",
            "Other - 01.mkv",
        ] {
            std::fs::write(show.join(name), b"").unwrap();
        }
        let anime = Anime {
            id: 7,
            episode_count: 12,
            titles: Titles {
                romaji: "Sample Show".into(),
                ..Titles::default()
            },
            ..Anime::default()
        };
        let recognizer = Recognizer::new(vec![anime], vec![], Relations::default());
        let folders = vec![root.display().to_string()];

        let found = find_episode(&root, 7, 2, &recognizer, &folders).unwrap();
        assert!(found.ends_with("Sample Show - 02.mkv"));
        assert_eq!(find_episode(&root, 7, 3, &recognizer, &folders), None);
        assert_eq!(find_folder(&root, 7, &recognizer), Some(show.clone()));

        let scan = scan_library(&folders, &recognizer);
        assert_eq!(scan.files_scanned, 3);
        assert_eq!(scan.anime.len(), 1);
        assert_eq!(scan.anime[0].files.len(), 2);
        assert_eq!(scan.anime[0].files[0].episodes, [1]);
        assert_eq!(scan.unrecognized.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
