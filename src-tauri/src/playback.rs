//! Reads the playback position from players that can report it:
//!
//! - MPC-HC and MPC-BE: the web interface (*Options › Player › Web Interface*),
//!   `variables.html`.
//! - VLC: the HTTP interface (*Preferences › All › Interface › Main
//!   interfaces › Web*, with a Lua HTTP password), `requests/status.json`.
//! - mpv and mpv.net: JSON IPC, started with `--input-ipc-server=mpvsocket`
//!   (a named pipe on Windows, a socket elsewhere).
//!
//! Anything that fails just means no position: the update falls back to the
//! timer.

use std::sync::OnceLock;
use std::time::Duration;

use patchouli_settings::Settings;
use patchouli_track::Progress;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Mpc,
    Vlc,
    Mpv,
}

/// Which interface a player (named as in the player list) has, if any.
pub fn source_for(player: &str) -> Option<Source> {
    let name = player.to_ascii_lowercase();
    if name.starts_with("mpc-hc") || name.starts_with("mpc-be") {
        Some(Source::Mpc)
    } else if name.starts_with("vlc") {
        Some(Source::Vlc)
    } else if name.starts_with("mpv") {
        Some(Source::Mpv)
    } else {
        None
    }
}

/// Local requests never go through the proxy.
fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

pub async fn read(source: Source, settings: &Settings) -> Option<Progress> {
    match source {
        Source::Mpc => {
            let url = format!(
                "http://127.0.0.1:{}/variables.html",
                settings.position_mpc_port
            );
            let text = http().get(url).send().await.ok()?.text().await.ok()?;
            parse_mpc(&text)
        }
        Source::Vlc => {
            let url = format!(
                "http://127.0.0.1:{}/requests/status.json",
                settings.position_vlc_port
            );
            let response = http()
                .get(url)
                .basic_auth("", Some(&settings.position_vlc_password))
                .send()
                .await
                .ok()?;
            parse_vlc(&response.json().await.ok()?)
        }
        Source::Mpv => {
            let name = settings.position_mpv_ipc.clone();
            let task = tauri::async_runtime::spawn_blocking(move || read_mpv(&name));
            tokio::time::timeout(TIMEOUT, task).await.ok()?.ok()?
        }
    }
}

/// The text of `<p id="…">` in MPC's `variables.html`.
fn mpc_value<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let start = html.find(&format!("id=\"{id}\">"))? + id.len() + 6;
    let rest = &html[start..];
    Some(rest[..rest.find('<')?].trim())
}

fn parse_mpc(html: &str) -> Option<Progress> {
    // Milliseconds. State: -1 nothing open, 0 stopped, 1 paused, 2 playing.
    let state: i32 = mpc_value(html, "state")?.parse().ok()?;
    let position: f64 = mpc_value(html, "position")?.parse().ok()?;
    let duration: f64 = mpc_value(html, "duration")?.parse().ok()?;
    (state >= 0 && duration > 0.0).then_some(Progress {
        position_secs: position / 1000.0,
        duration_secs: duration / 1000.0,
        playing: state == 2,
    })
}

fn parse_vlc(json: &Value) -> Option<Progress> {
    let length = json["length"].as_f64().filter(|&l| l > 0.0)?;
    // `position` (0–1) is more precise than whole-second `time`.
    let position = json["position"]
        .as_f64()
        .map(|p| p * length)
        .or_else(|| json["time"].as_f64())?;
    Some(Progress {
        position_secs: position,
        duration_secs: length,
        playing: json["state"] == "playing",
    })
}

const MPV_REQUEST: &str = concat!(
    r#"{"command":["get_property","time-pos"],"request_id":1}"#,
    "\n",
    r#"{"command":["get_property","duration"],"request_id":2}"#,
    "\n",
    r#"{"command":["get_property","pause"],"request_id":3}"#,
    "\n",
);

/// Reads replies to [`MPV_REQUEST`], skipping events, until all three came.
fn parse_mpv(lines: impl Iterator<Item = String>) -> Option<Progress> {
    let (mut position, mut duration, mut paused) = (None, None, None);
    for line in lines {
        let Ok(reply) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match reply["request_id"].as_i64() {
            Some(1) => position = Some(reply["data"].as_f64()?),
            Some(2) => duration = Some(reply["data"].as_f64()?),
            Some(3) => paused = Some(reply["data"].as_bool()?),
            _ => continue,
        }
        if let (Some(position), Some(duration), Some(paused)) = (position, duration, paused) {
            return (duration > 0.0).then_some(Progress {
                position_secs: position,
                duration_secs: duration,
                playing: !paused,
            });
        }
    }
    None
}

fn read_mpv(name: &str) -> Option<Progress> {
    use std::io::{BufRead, BufReader, Write};

    #[cfg(windows)]
    let mut stream = {
        let path = if name.starts_with(r"\\") {
            name.to_owned()
        } else {
            format!(r"\\.\pipe\{name}")
        };
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()?
    };
    #[cfg(unix)]
    let mut stream = {
        let path = if name.starts_with('/') {
            name.to_owned()
        } else {
            format!("/tmp/{name}")
        };
        let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
        stream.set_read_timeout(Some(TIMEOUT)).ok()?;
        stream
    };
    #[cfg(not(any(windows, unix)))]
    {
        let _ = name;
        return None;
    }

    stream.write_all(MPV_REQUEST.as_bytes()).ok()?;
    stream.flush().ok()?;
    let reader = BufReader::new(stream);
    parse_mpv(reader.lines().map_while(Result::ok).take(50))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn knows_which_players_report_position() {
        assert_eq!(source_for("MPC-HC"), Some(Source::Mpc));
        assert_eq!(source_for("MPC-BE"), Some(Source::Mpc));
        assert_eq!(source_for("VLC media player"), Some(Source::Vlc));
        assert_eq!(source_for("mpv.net"), Some(Source::Mpv));
        assert_eq!(source_for("PotPlayer"), None);
        assert_eq!(source_for("MPCSTAR"), None);
    }

    #[test]
    fn parses_mpc_variables() {
        let html = r#"<html><body><p id="file">ep.mkv</p><p id="state">2</p>
            <p id="statestring">Playing</p><p id="position">720000</p>
            <p id="duration">1440000</p></body></html>"#;
        let p = parse_mpc(html).unwrap();
        assert_eq!(
            (p.position_secs, p.duration_secs, p.playing),
            (720.0, 1440.0, true)
        );
        assert!(parse_mpc(&html.replace(">2<", ">-1<")).is_none());
        assert!(!parse_mpc(&html.replace(">2<", ">1<")).unwrap().playing);
    }

    #[test]
    fn parses_vlc_status() {
        let p = parse_vlc(
            &json!({ "time": 720, "length": 1440, "position": 0.5001, "state": "paused" }),
        )
        .unwrap();
        assert!((p.position_secs - 720.144).abs() < 0.01);
        assert!(!p.playing);
        assert!(parse_vlc(&json!({ "time": 0, "length": 0, "state": "stopped" })).is_none());
    }

    #[test]
    fn parses_mpv_replies_among_events() {
        let lines = [
            r#"{"event":"playback-restart"}"#,
            r#"{"data":1440.5,"error":"success","request_id":2}"#,
            r#"{"data":300.25,"error":"success","request_id":1}"#,
            r#"{"data":false,"error":"success","request_id":3}"#,
        ];
        let p = parse_mpv(lines.iter().map(|s| (*s).to_owned())).unwrap();
        assert_eq!(
            (p.position_secs, p.duration_secs, p.playing),
            (300.25, 1440.5, true)
        );
        // Nothing open: properties are unavailable.
        let missing = [r#"{"error":"property unavailable","request_id":1}"#];
        assert!(parse_mpv(missing.iter().map(|s| (*s).to_owned())).is_none());
    }
}
