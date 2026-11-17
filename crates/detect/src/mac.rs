//! macOS: players and browsers that can be asked through AppleScript.
//!
//! Anisthesia has no macOS support. Window titles of other apps need the
//! Accessibility permission, so this asks the apps themselves instead, and
//! only apps that are already running (so nothing is launched):
//!
//! - VLC: the current item's path and name
//! - QuickTime Player: the front document's file
//! - Safari, Chrome, Brave, Edge, Vivaldi, Arc: the active tab's URL and title
//!
//! IINA, mpv and Firefox have no AppleScript support and aren't detected.
//! macOS asks once per app whether Patchouli may control it.

use crate::{Detection, MediaInfo, MediaInfoKind, PlayerType};

pub struct MacPlayer {
    pub name: &'static str,
    /// The executable name `ps` shows.
    pub process: &'static str,
    pub kind: PlayerType,
    /// Prints the media, one value per line: the file (or URL), then the title.
    pub script: &'static str,
}

/// A Chromium-based browser: the active tab's URL and title.
macro_rules! chromium {
    ($app:literal) => {
        MacPlayer {
            name: $app,
            process: $app,
            kind: PlayerType::WebBrowser,
            script: concat!(
                "tell application \"",
                $app,
                "\"\n    if (count of windows) > 0 then return (URL of active tab of front window) & linefeed & (title of active tab of front window)\nend tell"
            ),
        }
    };
}

pub const PLAYERS: [MacPlayer; 8] = [
    MacPlayer {
        name: "VLC media player",
        process: "VLC",
        kind: PlayerType::Default,
        script: r#"tell application "VLC"
    if playing then return (path of current item) & linefeed & (name of current item)
end tell"#,
    },
    MacPlayer {
        name: "QuickTime Player",
        process: "QuickTime Player",
        kind: PlayerType::Default,
        script: r#"tell application "QuickTime Player"
    if (count of documents) > 0 then return (POSIX path of (file of front document as alias)) & linefeed & (name of front document)
end tell"#,
    },
    // Safari calls them `current tab` and `name`.
    MacPlayer {
        name: "Safari",
        process: "Safari",
        kind: PlayerType::WebBrowser,
        script: r#"tell application "Safari"
    if (count of windows) > 0 then return (URL of current tab of front window) & linefeed & (name of current tab of front window)
end tell"#,
    },
    chromium!("Google Chrome"),
    chromium!("Brave Browser"),
    chromium!("Microsoft Edge"),
    chromium!("Vivaldi"),
    chromium!("Arc"),
];

/// Players running, from `ps -axo pid=,comm=` output (`PID /path/to/exe`).
pub fn running(ps_output: &str) -> Vec<(&'static MacPlayer, u32)> {
    let mut found = Vec::new();
    for line in ps_output.lines() {
        let line = line.trim();
        let Some((pid, command)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let exe = command.trim().rsplit('/').next().unwrap_or_default();
        if let Some(player) = PLAYERS.iter().find(|p| p.process == exe)
            && !found
                .iter()
                .any(|(p, _): &(&MacPlayer, u32)| std::ptr::eq(*p, player))
        {
            found.push((player, pid.parse().unwrap_or(0)));
        }
    }
    found
}

/// A player's script output (file or URL, then title) as a detection.
pub fn to_detection(player: &MacPlayer, process_id: u32, output: &str) -> Option<Detection> {
    let mut lines = output.lines().map(str::trim).filter(|l| !l.is_empty());
    let location = lines.next()?;
    let title = lines.next().unwrap_or_default();
    let mut media = Vec::new();
    let kind = if location.starts_with('/') {
        MediaInfoKind::File
    } else if location.starts_with("http://") || location.starts_with("https://") {
        MediaInfoKind::Url
    } else {
        MediaInfoKind::Unknown
    };
    if kind != MediaInfoKind::Unknown {
        media.push(MediaInfo {
            kind,
            value: location.to_owned(),
        });
    }
    if !title.is_empty() {
        media.push(MediaInfo {
            kind: if player.kind == PlayerType::WebBrowser {
                MediaInfoKind::Tab
            } else {
                MediaInfoKind::Title
            },
            value: title.to_owned(),
        });
    }
    if media.is_empty() {
        return None;
    }
    Some(Detection {
        player: player.name.to_owned(),
        player_type: player.kind,
        window: process_id as isize,
        window_title: title.to_owned(),
        process_id,
        process_name: player.process.to_owned(),
        media,
    })
}

#[cfg(target_os = "macos")]
pub fn detect() -> Vec<Detection> {
    use std::process::Command;
    let Ok(ps) = Command::new("ps").args(["-axo", "pid=,comm="]).output() else {
        return Vec::new();
    };
    let ps = String::from_utf8_lossy(&ps.stdout);
    running(&ps)
        .into_iter()
        .filter_map(|(player, pid)| {
            let output = Command::new("osascript")
                .args(["-e", player.script])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            to_detection(player, pid, &String::from_utf8_lossy(&output.stdout))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_running_players() {
        let ps = "  1 /sbin/launchd\n 501 /Applications/VLC.app/Contents/MacOS/VLC\n 777 /Applications/Safari.app/Contents/MacOS/Safari\n 778 /Applications/Safari.app/Contents/MacOS/Safari\n";
        let found: Vec<(&str, u32)> = running(ps).iter().map(|(p, pid)| (p.name, *pid)).collect();
        assert_eq!(found, [("VLC media player", 501), ("Safari", 777)]);
    }

    #[test]
    fn parses_script_output() {
        let vlc = to_detection(
            &PLAYERS[0],
            5,
            "/Users/a/[Grp] Show - 01.mkv\n[Grp] Show - 01.mkv\n",
        )
        .unwrap();
        assert_eq!(
            vlc.media[0],
            MediaInfo {
                kind: MediaInfoKind::File,
                value: "/Users/a/[Grp] Show - 01.mkv".into()
            }
        );
        assert_eq!(vlc.media[1].kind, MediaInfoKind::Title);
        let safari = to_detection(
            &PLAYERS[2],
            6,
            "https://example.com/watch\nShow Episode 2\n",
        )
        .unwrap();
        assert_eq!(safari.player_type, PlayerType::WebBrowser);
        assert_eq!(safari.media[0].kind, MediaInfoKind::Url);
        assert_eq!(safari.media[1].kind, MediaInfoKind::Tab);
        assert_eq!(to_detection(&PLAYERS[0], 5, "\n"), None);
        // Every script names its own app.
        for player in &PLAYERS {
            let app = if player.name == "VLC media player" {
                "VLC"
            } else {
                player.name
            };
            assert!(
                player
                    .script
                    .contains(&format!("tell application \"{app}\"")),
                "{}",
                player.name
            );
        }
    }
}
