//! Linux: media players that implement MPRIS (`org.mpris.MediaPlayer2.*` on
//! the D-Bus session bus) — mpv (with mpv-mpris), VLC, Celluloid, Haruna,
//! SMPlayer, Firefox, Chromium and most others.
//!
//! Anisthesia has no Linux support; this replaces its window strategies:
//! the player's `xesam:url` is the file (or page) and `xesam:title` the title.

use crate::{Detection, MediaInfo, MediaInfoKind, PlayerType};

/// Browsers report the page URL and title rather than a file.
const BROWSERS: [&str; 7] = [
    "firefox", "chromium", "chrome", "brave", "vivaldi", "opera", "edge",
];

/// What an MPRIS player reports, before conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerState {
    /// The bus name, e.g. `org.mpris.MediaPlayer2.vlc`.
    pub bus_name: String,
    /// `Identity`, e.g. `VLC media player`.
    pub identity: String,
    /// `Playing`, `Paused` or `Stopped`.
    pub playback_status: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub process_id: u32,
}

/// `file:///home/a/My%20Show.mkv` → `/home/a/My Show.mkv`.
pub fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    // `file://host/path`: only local paths.
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&path[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// A stable number for a bus name, used as the "window" between polls.
fn window_id(bus_name: &str) -> isize {
    // FNV-1a.
    let hash = bus_name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    (hash >> 1) as isize
}

/// Converts a player's state. Stopped players, and players reporting
/// nothing, are skipped.
pub fn to_detection(state: &PlayerState) -> Option<Detection> {
    if state.playback_status.eq_ignore_ascii_case("stopped") {
        return None;
    }
    let short_name = state
        .bus_name
        .strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or(&state.bus_name)
        .split('.')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let browser = BROWSERS.iter().any(|b| short_name.contains(b));
    let mut media = Vec::new();
    if let Some(url) = state.url.as_deref().filter(|u| !u.is_empty()) {
        match file_url_to_path(url) {
            Some(path) => media.push(MediaInfo {
                kind: MediaInfoKind::File,
                value: path,
            }),
            None if url.starts_with("http://") || url.starts_with("https://") => {
                media.push(MediaInfo {
                    kind: MediaInfoKind::Url,
                    value: url.to_owned(),
                })
            }
            None => {}
        }
    }
    if let Some(title) = state.title.as_deref().filter(|t| !t.is_empty()) {
        media.push(MediaInfo {
            kind: MediaInfoKind::Title,
            value: title.to_owned(),
        });
    }
    if media.is_empty() {
        return None;
    }
    let name = if state.identity.is_empty() {
        short_name.clone()
    } else {
        state.identity.clone()
    };
    Some(Detection {
        player: name.clone(),
        player_type: if browser {
            PlayerType::WebBrowser
        } else {
            PlayerType::Default
        },
        window: window_id(&state.bus_name),
        window_title: state.title.clone().unwrap_or_default(),
        process_id: state.process_id,
        process_name: short_name,
        media,
    })
}

#[cfg(target_os = "linux")]
pub fn detect() -> Vec<Detection> {
    match dbus::players() {
        Ok(players) => players.iter().filter_map(to_detection).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
mod dbus {
    use std::collections::HashMap;

    use zbus::blocking::fdo::DBusProxy;
    use zbus::blocking::{Connection, Proxy};
    use zbus::names::BusName;
    use zbus::zvariant::{OwnedValue, Value};

    use super::PlayerState;

    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    const PATH: &str = "/org/mpris/MediaPlayer2";

    fn string(value: &Value<'_>) -> Option<String> {
        match value {
            Value::Str(s) => Some(s.to_string()),
            Value::Value(inner) => string(inner),
            // `xesam:artist` and the like are arrays; only the first item.
            Value::Array(items) => items.iter().next().and_then(string),
            _ => None,
        }
    }

    pub fn players() -> zbus::Result<Vec<PlayerState>> {
        let connection = Connection::session()?;
        let bus = DBusProxy::new(&connection)?;
        let mut states = Vec::new();
        for name in bus.list_names()? {
            let name = name.as_str();
            if !name.starts_with(PREFIX) {
                continue;
            }
            let root = Proxy::new(&connection, name, PATH, "org.mpris.MediaPlayer2")?;
            let player = Proxy::new(&connection, name, PATH, "org.mpris.MediaPlayer2.Player")?;
            let metadata: HashMap<String, OwnedValue> =
                player.get_property("Metadata").unwrap_or_default();
            let text = |key: &str| metadata.get(key).and_then(|v| string(v));
            let process_id = BusName::try_from(name)
                .ok()
                .and_then(|bus_name| bus.get_connection_unix_process_id(bus_name).ok())
                .unwrap_or(0);
            states.push(PlayerState {
                bus_name: name.to_owned(),
                identity: root.get_property("Identity").unwrap_or_default(),
                playback_status: player.get_property("PlaybackStatus").unwrap_or_default(),
                url: text("xesam:url"),
                title: text("xesam:title"),
                process_id,
            });
        }
        Ok(states)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(bus: &str, url: Option<&str>, title: Option<&str>) -> PlayerState {
        PlayerState {
            bus_name: bus.into(),
            identity: String::new(),
            playback_status: "Playing".into(),
            url: url.map(Into::into),
            title: title.map(Into::into),
            process_id: 42,
        }
    }

    #[test]
    fn converts_file_urls() {
        assert_eq!(
            file_url_to_path("file:///home/a/%5BGrp%5D%20Show%20-%2001.mkv").as_deref(),
            Some("/home/a/[Grp] Show - 01.mkv")
        );
        assert_eq!(
            file_url_to_path("file://localhost/x%C3%A9.mkv").as_deref(),
            Some("/xé.mkv")
        );
        assert_eq!(file_url_to_path("file://server/share/x.mkv"), None);
        assert_eq!(file_url_to_path("https://x"), None);
    }

    #[test]
    fn players_and_browsers() {
        let vlc = to_detection(&PlayerState {
            identity: "VLC media player".into(),
            ..state(
                "org.mpris.MediaPlayer2.vlc",
                Some("file:///a/Show%2001.mkv"),
                Some("Show 01"),
            )
        })
        .unwrap();
        assert_eq!(vlc.player, "VLC media player");
        assert_eq!(vlc.player_type, PlayerType::Default);
        assert_eq!(
            vlc.media[0],
            MediaInfo {
                kind: MediaInfoKind::File,
                value: "/a/Show 01.mkv".into()
            }
        );
        assert_eq!(vlc.process_id, 42);

        let firefox = to_detection(&state(
            "org.mpris.MediaPlayer2.firefox.instance_1_83",
            Some("https://example.com/watch/1"),
            Some("Show Episode 1"),
        ))
        .unwrap();
        assert_eq!(firefox.player, "firefox");
        assert_eq!(firefox.player_type, PlayerType::WebBrowser);
        assert_eq!(firefox.media.len(), 2);
        assert_eq!(
            firefox.window,
            to_detection(&state(
                "org.mpris.MediaPlayer2.firefox.instance_1_83",
                None,
                Some("x")
            ))
            .unwrap()
            .window
        );

        let stopped = PlayerState {
            playback_status: "Stopped".into(),
            ..state("org.mpris.MediaPlayer2.mpv", Some("file:///a.mkv"), None)
        };
        assert_eq!(to_detection(&stopped), None);
        assert_eq!(
            to_detection(&state("org.mpris.MediaPlayer2.mpv", None, None)),
            None
        );
    }
}
