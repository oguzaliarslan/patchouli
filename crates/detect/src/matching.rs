//! Matching windows to players and reading window titles (Anisthesia's
//! `win_platform.cpp` and `win_strategies.cpp`, minus the Windows calls).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use crate::MediaInfoKind;
use crate::players::Player;

/// Compiled `^(?:pattern)$`, cached; `None` if the pattern is invalid.
fn full_match_regex(pattern: &str) -> Option<Regex> {
    static CACHE: LazyLock<Mutex<HashMap<String, Option<Regex>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(pattern.to_owned())
        .or_insert_with(|| Regex::new(&format!("^(?:{pattern})$")).ok())
        .clone()
}

/// A pattern is a regular expression if it starts with `^`, else a string
/// compared ignoring ASCII case.
pub fn check_pattern(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if pattern.starts_with('^') && full_match_regex(pattern).is_some_and(|r| r.is_match(value)) {
        return true;
    }
    pattern.eq_ignore_ascii_case(value)
}

/// The window's class and the process's file name (without extension) both
/// match the player's lists.
pub fn is_player_window(player: &Player, class_name: &str, process_name: &str) -> bool {
    player.windows.iter().any(|p| check_pattern(p, class_name))
        && player
            .executables
            .iter()
            .any(|p| check_pattern(p, process_name))
}

/// Applies a window title format: the first non-empty group of a full match,
/// or an empty string if it matched without one (e.g. `No file - mpv`).
/// Titles that don't match are left as they are.
pub fn apply_window_title_format(format: &str, title: &str) -> String {
    if format.is_empty() {
        return title.to_owned();
    }
    let Some(regex) = full_match_regex(format) else {
        return title.to_owned();
    };
    match regex.captures(title) {
        Some(captures) => captures
            .iter()
            .skip(1)
            .flatten()
            .map(|m| m.as_str())
            .find(|s| !s.is_empty())
            .unwrap_or_default()
            .to_owned(),
        None => title.to_owned(),
    }
}

/// A title that looks like a path (`C:\...` or `\\server\...`) is a file.
pub fn infer_media_kind(value: &str) -> MediaInfoKind {
    static PATH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^(?:[A-Za-z]:[/\\]|\\\\)[^<>:"/\\|?*]+"#).expect("valid pattern")
    });
    if PATH.is_match(value) {
        MediaInfoKind::File
    } else {
        MediaInfoKind::Unknown
    }
}

/// Removes the `\\?\` prefix that `GetFinalPathNameByHandle` adds.
pub fn clean_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(check_pattern("mpv", "MPV"));
        assert!(!check_pattern("mpv", "mpv.net"));
        assert!(check_pattern("^mpc-hc.+", "mpc-hc64_nvo"));
        assert!(!check_pattern("^mpc-hc.+", "xmpc-hc64"));
        assert!(!check_pattern("", ""));
    }

    #[test]
    fn players() {
        let player = Player {
            windows: vec!["mpv".into()],
            executables: vec!["mpv".into()],
            ..Player::default()
        };
        assert!(is_player_window(&player, "mpv", "mpv"));
        assert!(!is_player_window(&player, "mpv", "vlc"));
    }

    #[test]
    fn window_title_formats() {
        let mpv = "^No file - mpv|(.+) - mpv|mpv - (.+)";
        assert_eq!(
            apply_window_title_format(mpv, "Show - 01.mkv - mpv"),
            "Show - 01.mkv"
        );
        assert_eq!(
            apply_window_title_format(mpv, "mpv - Show - 01.mkv"),
            "Show - 01.mkv"
        );
        assert_eq!(apply_window_title_format(mpv, "No file - mpv"), "");
        assert_eq!(
            apply_window_title_format(mpv, "something else"),
            "something else"
        );

        let chrome = r"^(.+) \(Incognito\)(?: - Google Chrome)?|(.+) - Google Chrome|(.+)";
        assert_eq!(
            apply_window_title_format(chrome, "Ep 1 - Site - Google Chrome"),
            "Ep 1 - Site"
        );
        assert_eq!(
            apply_window_title_format(chrome, "Ep 1 (Incognito)"),
            "Ep 1"
        );
    }

    #[test]
    fn media_kinds_and_paths() {
        assert_eq!(infer_media_kind(r"C:\Anime\01.mkv"), MediaInfoKind::File);
        assert_eq!(infer_media_kind(r"\\nas\anime\01.mkv"), MediaInfoKind::File);
        assert_eq!(infer_media_kind("Show - 01.mkv"), MediaInfoKind::Unknown);
        assert_eq!(clean_path(r"\\?\D:\Anime\01.mkv"), r"D:\Anime\01.mkv");
        assert_eq!(clean_path(r"\\?\UNC\nas\a.mkv"), r"\\nas\a.mkv");
        assert_eq!(clean_path(r"D:\a.mkv"), r"D:\a.mkv");
    }
}
