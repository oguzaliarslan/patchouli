//! Detects running media players and what they are playing.
//!
//! Ported from [erengy/anisthesia] (MIT). Windows only for now: windows are
//! enumerated, matched against the player list (`players.anisthesia`), and
//! each player's strategies read what's playing:
//!
//! - `window_title`: the window title, through the player's title format
//! - `open_files`: files the player process has open
//! - `ui_automation`: a browser's address bar, tabs and title
//!
//! Linux uses MPRIS over D-Bus ([`mpris`]) and macOS asks players through
//! AppleScript ([`mac`]); neither reports the focused window, so "pause when
//! unfocused" only works on Windows.
//!
//! [erengy/anisthesia]: https://github.com/erengy/anisthesia

pub mod mac;
pub mod matching;
pub mod mpris;
pub mod players;
#[cfg(windows)]
mod win;

use serde::Serialize;

pub use players::{ParseError, Player, PlayerType, Strategy, parse_players};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaInfoKind {
    /// A window title that isn't a path.
    Unknown,
    File,
    Tab,
    Title,
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaInfo {
    pub kind: MediaInfoKind,
    pub value: String,
}

/// A player window and what its strategies found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub player: String,
    pub player_type: PlayerType,
    /// Window handle, to recognise the same window between polls.
    pub window: isize,
    pub window_title: String,
    pub process_id: u32,
    pub process_name: String,
    pub media: Vec<MediaInfo>,
}

/// Player windows, top-most first, with what each is playing. Windows whose
/// strategies found nothing are left out.
pub fn detect(players: &[Player]) -> Vec<Detection> {
    #[cfg(windows)]
    {
        win::detect(players)
    }
    // The player list describes Windows players; elsewhere players are found
    // by other means.
    #[cfg(target_os = "linux")]
    {
        let _ = players;
        mpris::detect()
    }
    #[cfg(target_os = "macos")]
    {
        let _ = players;
        mac::detect()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = players;
        Vec::new()
    }
}

/// The window that has the keyboard focus.
pub fn foreground_window() -> Option<isize> {
    #[cfg(windows)]
    {
        win::foreground_window()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
