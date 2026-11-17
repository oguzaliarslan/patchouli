//! Titles from parent folders (Taiga `track/recognition_path.cpp`), for files
//! named only by episode (e.g. `Title/Season 2/01.mkv`).

use std::path::Path;

use tracker_anitomy::{ElementKind, Options};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathInfo {
    pub title: String,
    pub season: String,
}

/// Lower-case path with `/` separators and no trailing separator, for
/// comparing folders the way Windows does.
pub fn comparable(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
    text.trim_end_matches('/').to_owned()
}

fn is_library_folder(dir: &Path, library_folders: &[String]) -> bool {
    let dir = comparable(dir);
    library_folders
        .iter()
        .any(|folder| comparable(Path::new(folder)) == dir)
}

fn has_title(name: &str) -> bool {
    tracker_anitomy::parse(name)
        .iter()
        .any(|e| e.kind == ElementKind::Title)
}

fn is_invalid_name(name: &str) -> bool {
    const INVALID: [&str; 5] = ["anime", "download", "downloads", "extra", "extras"];
    name.is_empty() || INVALID.iter().any(|n| n.eq_ignore_ascii_case(name)) || !has_title(name)
}

fn season_number(name: &str) -> String {
    tracker_anitomy::parse(name)
        .into_iter()
        .find(|e| e.kind == ElementKind::Season)
        .map(|e| e.value)
        .unwrap_or_default()
}

/// The title from the nearest suitable parent folder (up to two levels), and
/// a season number from a season-only folder on the way (`Season 2`).
pub fn parse_parent_directories(file: &Path, library_folders: &[String]) -> PathInfo {
    const MAX_DEPTH: usize = 2;
    let mut season = String::new();
    let mut dir = file.parent();
    for _ in 0..MAX_DEPTH {
        let Some(current) = dir else {
            break;
        };
        // A library folder, a drive root or anything above can't be an anime folder.
        let Some(name) = current
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            break;
        };
        if is_library_folder(current, library_folders) || name.contains(':') {
            break;
        }
        if !is_invalid_name(&name) {
            return PathInfo {
                title: extract_title(&name),
                season,
            };
        }
        if season.is_empty() {
            season = season_number(&name);
        }
        dir = current.parent();
    }
    PathInfo::default()
}

/// The title of a folder named like `[Group] Title [Info]`, or the whole name.
fn extract_title(name: &str) -> String {
    let options = Options {
        parse_episode: false,
        parse_episode_title: false,
        parse_file_checksum: false,
        parse_file_extension: false,
        ..Options::default()
    };
    tracker_anitomy::parse_with(name, &options)
        .into_iter()
        .find(|e| e.kind == ElementKind::Title)
        .map_or_else(|| name.to_owned(), |e| e.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(path: &str, libraries: &[&str]) -> PathInfo {
        let libraries: Vec<String> = libraries.iter().map(|s| s.to_string()).collect();
        parse_parent_directories(Path::new(path), &libraries)
    }

    #[test]
    fn uses_parent_folder() {
        assert_eq!(
            info("D:/Anime/Toradora/01.mkv", &["D:/Anime"]).title,
            "Toradora"
        );
    }

    #[test]
    fn skips_season_folders() {
        let parsed = info("D:/Anime/Shingeki no Kyojin/Season 2/01.mkv", &["D:/Anime"]);
        assert_eq!(parsed.title, "Shingeki no Kyojin");
        assert_eq!(parsed.season, "2");
    }

    #[test]
    fn stops_at_library_folders_and_generic_names() {
        assert_eq!(
            info("D:/Anime/01.mkv", &["d:\\anime\\"]),
            PathInfo::default()
        );
        assert_eq!(info("C:/Downloads/01.mkv", &[]), PathInfo::default());
    }

    #[test]
    fn extracts_title_from_release_folder() {
        assert_eq!(
            info("D:/[Group] Toradora! [BD 1080p]/01.mkv", &[]).title,
            "Toradora!"
        );
    }
}
