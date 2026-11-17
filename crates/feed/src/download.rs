//! Downloading torrents and handing them to a BitTorrent client (Taiga v1
//! `Aggregator::Download` and `HandleFeedDownloadOpen`).

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::item::FeedItem;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    #[default]
    EpisodeNumber,
    ReleaseDate,
}

/// Order for downloading several selected items: by anime, then by episode
/// number or release date.
pub fn compare(a: &FeedItem, b: &FeedItem, by: SortBy, descending: bool) -> Ordering {
    let anime = a.anime_id.cmp(&b.anime_id);
    if anime != Ordering::Equal {
        return anime;
    }
    let ordering = match by {
        SortBy::EpisodeNumber => a.episode_high().cmp(&b.episode_high()),
        SortBy::ReleaseDate => a.pub_time.cmp(&b.pub_time),
    };
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

/// Use the magnet link instead of downloading the `.torrent` file.
pub fn use_magnet(item: &FeedItem, prefer_magnet: bool) -> bool {
    (prefer_magnet && !item.magnet_link.is_empty()) || item.link.starts_with("magnet:")
}

/// The magnet link to open.
pub fn magnet_link(item: &FeedItem) -> &str {
    if item.magnet_link.is_empty() {
        &item.link
    } else {
        &item.magnet_link
    }
}

/// Checks that a response is a torrent file (Taiga's `ValidateFeedDownload`).
pub fn validate(
    status: u16,
    url: &str,
    content_type: Option<&str>,
    has_content_disposition: bool,
    body: &[u8],
) -> Result<(), String> {
    if status == 404 {
        return Err(format!("File not found at {url}"));
    }
    if status >= 400 {
        return Err(format!("Invalid HTTP response ({status})"));
    }
    if body.starts_with(b"<!DOCTYPE html>") || body.starts_with(b"<!doctype html>") {
        return Err(format!("Invalid torrent file: {url}"));
    }
    const ALLOWED: [&str; 5] = [
        "application/x-bittorrent",
        // Wrong for .torrent files, but used by misconfigured servers.
        "application/force-download",
        "application/octet-stream",
        "application/torrent",
        "application/x-torrent",
    ];
    if let Some(content_type) = content_type {
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if !ALLOWED.contains(&mime.as_str()) && !has_content_disposition {
            return Err(format!("Invalid content type: {content_type}"));
        }
    }
    Ok(())
}

/// A file name from a title: characters Windows doesn't allow are removed.
pub fn file_name(title: &str) -> String {
    let name: String = title
        .chars()
        .filter(|c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()
        })
        .collect();
    let name = name.trim().trim_end_matches('.').to_owned();
    if name.is_empty() {
        "download".into()
    } else {
        name
    }
}

/// Command-line arguments for a BitTorrent client, telling it to save into
/// `download_dir` when the client is one Taiga knows. Returns the arguments
/// and whether the client runs without a window.
pub fn client_arguments(
    client_path: &str,
    file: &str,
    download_dir: Option<&str>,
) -> (Vec<String>, bool) {
    let name = std::path::Path::new(client_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let Some(dir) = download_dir.filter(|d| !d.is_empty()) else {
        return (vec![file.to_owned()], false);
    };
    let dir = dir.trim_end_matches(['\\', '/']).to_owned();
    let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    if name.contains("aria2c") {
        (args(&[&format!("--dir={dir}"), file]), false)
    } else if name.contains("deluge-console") {
        (args(&["add", "-p", &dir, file]), true)
    } else if name.contains("picotorrent") {
        (
            args(&[&format!("--save-path={dir}"), "--silent", file]),
            false,
        )
    } else if name.contains("qbittorrent") {
        (
            args(&[&format!("--save-path={dir}"), "--skip-dialog=true", file]),
            false,
        )
    } else if name.contains("transmission-remote") {
        (args(&["-a", file, "-w", &dir]), true)
    } else if name.contains("utorrent") {
        (args(&["/directory", &dir, file]), false)
    } else {
        (vec![file.to_owned()], false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_responses() {
        assert!(
            validate(
                200,
                "u",
                Some("application/x-bittorrent"),
                false,
                b"d8:announce"
            )
            .is_ok()
        );
        assert!(validate(200, "u", None, false, b"d8:announce").is_ok());
        assert!(validate(200, "u", Some("text/html"), true, b"d8:").is_ok());
        assert!(validate(200, "u", Some("text/html"), false, b"d8:").is_err());
        assert!(validate(200, "u", None, false, b"<!DOCTYPE html><html>").is_err());
        assert_eq!(
            validate(404, "u", None, false, b"").unwrap_err(),
            "File not found at u"
        );
    }

    #[test]
    fn client_arguments_for_known_clients() {
        let (args, hidden) = client_arguments(
            r"C:\Apps\qBittorrent\qbittorrent.exe",
            "a.torrent",
            Some(r"D:\Anime\Show\"),
        );
        assert_eq!(
            args,
            [
                r"--save-path=D:\Anime\Show",
                "--skip-dialog=true",
                "a.torrent"
            ]
        );
        assert!(!hidden);
        let (args, hidden) =
            client_arguments("/usr/bin/transmission-remote", "a.torrent", Some("/anime"));
        assert_eq!(args, ["-a", "a.torrent", "-w", "/anime"]);
        assert!(hidden);
        assert_eq!(
            client_arguments("other.exe", "a.torrent", Some("/x")).0,
            ["a.torrent"]
        );
        assert_eq!(
            client_arguments("qbittorrent.exe", "a.torrent", None).0,
            ["a.torrent"]
        );
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("[A] Show: Part 2? - 01."), "[A] Show Part 2 - 01");
        assert_eq!(file_name("///"), "download");
    }
}
