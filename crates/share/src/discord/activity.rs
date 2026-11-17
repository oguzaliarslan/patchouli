//! The `activity` object of a `SET_ACTIVITY` command (Taiga
//! `share/discord_rpc.cpp`). See
//! <https://docs.discord.com/developers/events/gateway-events#activity-object>.

use serde_json::{Map, Value, json};

const MIN_TEXT_LENGTH: usize = 2;
const MAX_TEXT_LENGTH: usize = 128;
const MAX_BUTTON_LABEL_LENGTH: usize = 32;
const MAX_BUTTON_URL_LENGTH: usize = 512;
const MAX_BUTTONS: usize = 2;

/// Types allowed by `SET_ACTIVITY`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivityType {
    #[default]
    Playing = 0,
    Listening = 2,
    Watching = 3,
    Competing = 5,
}

/// Which field the member list shows as the user's status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusDisplayType {
    /// The application name.
    #[default]
    Name = 0,
    State = 1,
    Details = 2,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Activity {
    pub kind: ActivityType,
    pub status_display_type: StatusDisplayType,
    pub details: String,
    pub details_url: String,
    pub state: String,
    pub state_url: String,
    /// Seconds since the epoch, shown as elapsed time.
    pub start_time: i64,
    /// Asset key or image URL.
    pub large_image: String,
    pub large_text: String,
    pub large_url: String,
    pub small_image: String,
    pub small_text: String,
    pub small_url: String,
    /// At most 2; not shown to the user themselves.
    pub buttons: Vec<Button>,
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Discord rejects the whole activity if a text is too short, so such texts
/// are left out; long ones are shortened.
fn insert_text(object: &mut Map<String, Value>, key: &str, text: &str) {
    let trimmed = text.trim();
    if trimmed.chars().count() >= MIN_TEXT_LENGTH {
        object.insert(key.into(), json!(truncate(trimmed, MAX_TEXT_LENGTH)));
    }
}

fn insert_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        object.insert(key.into(), json!(value));
    }
}

pub fn to_json(activity: &Activity) -> Value {
    let mut object = Map::new();
    object.insert("type".into(), json!(activity.kind as i32));
    insert_text(&mut object, "details", &activity.details);
    insert_string(&mut object, "details_url", &activity.details_url);
    insert_text(&mut object, "state", &activity.state);
    insert_string(&mut object, "state_url", &activity.state_url);

    // Discord would show nothing if the field to display were missing.
    let displayed = if activity.status_display_type == StatusDisplayType::State {
        "state"
    } else {
        "details"
    };
    if activity.status_display_type != StatusDisplayType::Name && object.contains_key(displayed) {
        object.insert(
            "status_display_type".into(),
            json!(activity.status_display_type as i32),
        );
    }
    if activity.start_time > 0 {
        object.insert("timestamps".into(), json!({ "start": activity.start_time }));
    }

    let mut assets = Map::new();
    insert_string(&mut assets, "large_image", &activity.large_image);
    insert_text(&mut assets, "large_text", &activity.large_text);
    insert_string(&mut assets, "large_url", &activity.large_url);
    insert_string(&mut assets, "small_image", &activity.small_image);
    insert_text(&mut assets, "small_text", &activity.small_text);
    insert_string(&mut assets, "small_url", &activity.small_url);
    if !assets.is_empty() {
        object.insert("assets".into(), Value::Object(assets));
    }

    let buttons: Vec<Value> = activity
        .buttons
        .iter()
        .filter(|b| {
            !b.label.trim().is_empty() && !b.url.is_empty() && b.url.len() <= MAX_BUTTON_URL_LENGTH
        })
        .take(MAX_BUTTONS)
        .map(
            |b| json!({ "label": truncate(b.label.trim(), MAX_BUTTON_LABEL_LENGTH), "url": b.url }),
        )
        .collect();
    if !buttons.is_empty() {
        object.insert("buttons".into(), Value::Array(buttons));
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_activity() {
        let activity = Activity {
            kind: ActivityType::Watching,
            status_display_type: StatusDisplayType::Details,
            details: "Toradora!".into(),
            details_url: "https://anilist.co/anime/4224".into(),
            state: "Episode 3/25".into(),
            start_time: 1_700_000_000,
            large_image: "https://img/1.jpg".into(),
            large_text: "Toradora!".into(),
            small_image: "anilist".into(),
            small_text: "AniList".into(),
            buttons: vec![Button {
                label: "View Anime".into(),
                url: "https://anilist.co/anime/4224".into(),
            }],
            ..Activity::default()
        };
        assert_eq!(
            to_json(&activity),
            json!({
                "type": 3,
                "details": "Toradora!",
                "details_url": "https://anilist.co/anime/4224",
                "state": "Episode 3/25",
                "status_display_type": 2,
                "timestamps": { "start": 1_700_000_000 },
                "assets": {
                    "large_image": "https://img/1.jpg",
                    "large_text": "Toradora!",
                    "small_image": "anilist",
                    "small_text": "AniList"
                },
                "buttons": [{ "label": "View Anime", "url": "https://anilist.co/anime/4224" }]
            })
        );
    }

    #[test]
    fn short_texts_are_left_out_and_long_ones_shortened() {
        let activity = Activity {
            details: " x ".into(),
            state: "y".repeat(200),
            status_display_type: StatusDisplayType::Details,
            ..Activity::default()
        };
        let value = to_json(&activity);
        assert!(value.get("details").is_none());
        // `details` is missing, so the display type isn't set.
        assert!(value.get("status_display_type").is_none());
        let state = value["state"].as_str().unwrap();
        assert_eq!(state.chars().count(), MAX_TEXT_LENGTH);
        assert!(state.ends_with('…'));
    }

    #[test]
    fn at_most_two_valid_buttons() {
        let button = |label: &str, url: &str| Button {
            label: label.into(),
            url: url.into(),
        };
        let activity = Activity {
            buttons: vec![
                button("", "https://a"),
                button("One", "https://1"),
                button("Two", &"x".repeat(600)),
                button("Three", "https://3"),
                button("Four", "https://4"),
            ],
            ..Activity::default()
        };
        let value = to_json(&activity);
        let labels: Vec<&str> = value["buttons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, ["One", "Three"]);
    }
}
