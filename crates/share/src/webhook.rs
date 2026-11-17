//! Sharing events posted as JSON to a user-defined URL (Taiga `share/event.cpp`
//! and `share/http.cpp`).
//!
//! ```json
//! {
//!   "version": 1,
//!   "event": "playing" | "stopped" | "updated",
//!   "timestamp": "2026-01-01T00:00:00Z",
//!   "service": "anilist" | "kitsu" | "myanimelist",
//!   "anime": { "id", "title", "episodes"?, "image"?, "url" },
//!   "episode": { "number" }?,
//!   "entry": { "status", "progress", "score"?, "rewatching" }?
//! }
//! ```
//!
//! `anime.id` belongs to `service`; `entry.score` ranges from 1 to 100.
//! Optional fields are left out when unknown.

use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};

/// Version of the event document. Bump only on breaking changes.
pub const EVENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// An episode started playing.
    Playing,
    /// The episode that was playing no longer is.
    Stopped,
    /// The list entry was updated after watching an episode.
    Updated,
}

impl EventType {
    fn name(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Stopped => "stopped",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAnime {
    pub id: i32,
    pub title: String,
    pub url: String,
    /// 0 if unknown.
    pub episodes: i32,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEntry {
    /// `watching`, `completed`, `onHold`, `dropped` or `planToWatch`.
    pub status: &'static str,
    pub progress: i32,
    /// 0–100, 0 if not scored.
    pub score: i32,
    pub rewatching: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: EventType,
    /// ISO 8601 in UTC, e.g. `2026-01-01T00:00:00Z`.
    pub timestamp: String,
    pub service: &'static str,
    /// `None` for anime that can't be shared (unknown, or a private entry).
    pub anime: Option<EventAnime>,
    pub entry: Option<EventEntry>,
    /// The last episode number of the file, 0 if unknown.
    pub episode: i32,
}

pub fn to_json(event: &Event) -> Value {
    let mut object = json!({
        "version": EVENT_VERSION,
        "event": event.kind.name(),
        "timestamp": event.timestamp,
        "service": event.service,
    });
    if let Some(anime) = &event.anime {
        let mut value = json!({ "id": anime.id, "title": anime.title, "url": anime.url });
        if anime.episodes > 0 {
            value["episodes"] = json!(anime.episodes);
        }
        if !anime.image.is_empty() {
            value["image"] = json!(anime.image);
        }
        object["anime"] = value;
        if let Some(entry) = &event.entry {
            let mut value = json!({
                "status": entry.status,
                "progress": entry.progress,
                "rewatching": entry.rewatching,
            });
            if entry.score > 0 {
                value["score"] = json!(entry.score);
            }
            object["entry"] = value;
        }
    }
    if event.episode > 0 {
        object["episode"] = json!({ "number": event.episode });
    }
    object
}

/// An `http`/`https` URL with a host, or `None`.
pub fn target_url(url: &str) -> Option<Url> {
    let url = Url::parse(url.trim()).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty()))
        .then_some(url)
}

/// `Name: value` lines; lines without a name are skipped.
pub fn parse_headers(lines: &[String]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (name.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Posts an event. User headers may replace the defaults.
pub async fn send(
    http: &reqwest::Client,
    url: Url,
    headers: &[(String, String)],
    event: &Event,
) -> Result<(), String> {
    let mut request = http
        .post(url)
        .timeout(Duration::from_secs(15))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(to_json(event).to_string());
    for (name, value) in headers {
        let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) else {
            log::warn!("Webhook: invalid header {name}");
            continue;
        };
        request = request.header(name, value);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", response.status().as_u16()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: EventType) -> Event {
        Event {
            kind,
            timestamp: "2026-01-01T00:00:00Z".into(),
            service: "anilist",
            anime: Some(EventAnime {
                id: 4224,
                title: "Toradora!".into(),
                url: "https://anilist.co/anime/4224".into(),
                episodes: 25,
                image: String::new(),
            }),
            entry: Some(EventEntry {
                status: "watching",
                progress: 3,
                score: 0,
                rewatching: false,
            }),
            episode: 3,
        }
    }

    #[test]
    fn schema() {
        assert_eq!(
            to_json(&event(EventType::Playing)),
            json!({
                "version": 1,
                "event": "playing",
                "timestamp": "2026-01-01T00:00:00Z",
                "service": "anilist",
                "anime": { "id": 4224, "title": "Toradora!", "url": "https://anilist.co/anime/4224", "episodes": 25 },
                "entry": { "status": "watching", "progress": 3, "rewatching": false },
                "episode": { "number": 3 }
            })
        );
    }

    #[test]
    fn private_anime_are_not_shared() {
        let mut e = event(EventType::Stopped);
        e.anime = None;
        e.episode = 0;
        assert_eq!(
            to_json(&e),
            json!({ "version": 1, "event": "stopped", "timestamp": "2026-01-01T00:00:00Z", "service": "anilist" })
        );
    }

    #[test]
    fn urls_and_headers() {
        assert!(target_url("https://example.com/hook").is_some());
        assert!(target_url("ftp://example.com").is_none());
        assert!(target_url("not a url").is_none());
        let headers = parse_headers(&[
            "Authorization: Bearer x".into(),
            ": nothing".into(),
            "bad".into(),
        ]);
        assert_eq!(
            headers,
            [("Authorization".to_owned(), "Bearer x".to_owned())]
        );
    }

    #[tokio::test]
    async fn posts_json_with_headers() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            while !String::from_utf8_lossy(&request).contains("\"version\"") {
                let n = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        let url = target_url(&format!("http://127.0.0.1:{port}/hook")).unwrap();
        let headers = parse_headers(&["X-Token: secret".into()]);
        send(
            &reqwest::Client::new(),
            url,
            &headers,
            &event(EventType::Updated),
        )
        .await
        .unwrap();
        let request = server.await.unwrap().to_lowercase();
        assert!(request.starts_with("post /hook"));
        assert!(request.contains("x-token: secret"));
        assert!(request.contains("content-type: application/json"));
        assert!(request.contains("\"event\":\"updated\""));
    }
}
