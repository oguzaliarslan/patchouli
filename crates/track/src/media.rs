//! The episode in what detection found (Taiga `track/media.cpp`).

use std::path::Path;

use patchouli_detect::{Detection, MediaInfoKind};
use patchouli_recognition::{Episode, parse_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    pub episode: Episode,
    /// The video file, if the player reported open files.
    pub file: Option<String>,
}

/// Picks the episode: the first open video file, or else the title (the
/// window title is preferred over a browser's page title).
pub fn resolve(detection: &Detection, library_folders: &[String]) -> Option<ResolvedMedia> {
    let mut files = Vec::new();
    let mut title: Option<&str> = None;
    for info in &detection.media {
        match info.kind {
            MediaInfoKind::File => files.push(info.value.as_str()),
            MediaInfoKind::Title => {
                title.get_or_insert(info.value.as_str());
            }
            MediaInfoKind::Unknown => title = Some(info.value.as_str()),
            MediaInfoKind::Url | MediaInfoKind::Tab => {}
        }
    }
    if !files.is_empty() {
        // Players also open subtitles, audio tracks and resources.
        return files.into_iter().find_map(|file| {
            let episode = parse_file(Path::new(file), library_folders);
            episode.is_video_file().then(|| ResolvedMedia {
                episode,
                file: Some(file.to_owned()),
            })
        });
        // Files are open but none is a video: don't fall back to the title,
        // which describes the same (non-video) file.
    }
    let title = title.filter(|t| !t.is_empty())?;
    Some(ResolvedMedia {
        episode: Episode::parse(title),
        file: None,
    })
}

/// The detection to follow: the same window as before if it's still there,
/// else the top-most one.
pub fn pick(detections: &[Detection], previous_window: Option<isize>) -> Option<&Detection> {
    previous_window
        .and_then(|w| detections.iter().find(|d| d.window == w))
        .or_else(|| detections.first())
}

#[cfg(test)]
mod tests {
    use patchouli_anitomy::ElementKind;
    use patchouli_detect::{MediaInfo, PlayerType};

    use super::*;

    fn detection(window: isize, media: &[(MediaInfoKind, &str)]) -> Detection {
        Detection {
            player: "mpv".into(),
            player_type: PlayerType::Default,
            window,
            window_title: String::new(),
            process_id: 1,
            process_name: "mpv".into(),
            media: media
                .iter()
                .map(|(kind, value)| MediaInfo {
                    kind: *kind,
                    value: (*value).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn prefers_the_video_file() {
        let d = detection(
            1,
            &[
                (MediaInfoKind::File, r"C:\Program Files\mpv\fonts.conf"),
                (
                    MediaInfoKind::File,
                    r"D:\Anime\Show\[Group] Show - 04 [1080p].ass",
                ),
                (
                    MediaInfoKind::File,
                    r"D:\Anime\Show\[Group] Show - 04 [1080p].mkv",
                ),
                (MediaInfoKind::Unknown, "something else"),
            ],
        );
        let resolved = resolve(&d, &[]).unwrap();
        assert_eq!(resolved.episode.title(), "Show");
        assert_eq!(resolved.episode.element(ElementKind::Episode), Some("04"));
        assert!(resolved.file.unwrap().ends_with(".mkv"));
    }

    #[test]
    fn non_video_files_mean_nothing_is_playing() {
        let d = detection(
            1,
            &[
                (MediaInfoKind::File, r"D:\Music\song.flac"),
                (MediaInfoKind::Unknown, "song"),
            ],
        );
        assert_eq!(resolve(&d, &[]), None);
    }

    #[test]
    fn falls_back_to_the_title() {
        let d = detection(
            1,
            &[
                (MediaInfoKind::Title, "Page title"),
                (MediaInfoKind::Url, "example.com"),
                (MediaInfoKind::Unknown, "Show - 05"),
            ],
        );
        let resolved = resolve(&d, &[]).unwrap();
        assert_eq!(resolved.episode.title(), "Show");
        assert_eq!(resolved.file, None);
        assert_eq!(
            resolve(&detection(1, &[(MediaInfoKind::Unknown, "")]), &[]),
            None
        );
    }

    #[test]
    fn keeps_following_the_same_window() {
        let ds = [detection(1, &[]), detection(2, &[])];
        assert_eq!(pick(&ds, Some(2)).unwrap().window, 2);
        assert_eq!(pick(&ds, Some(9)).unwrap().window, 1);
        assert_eq!(pick(&ds, None).unwrap().window, 1);
        assert!(pick(&[], None).is_none());
    }
}
