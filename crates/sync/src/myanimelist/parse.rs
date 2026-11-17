use patchouli_core::anime::{AgeRating, Status, Titles, Type};
use patchouli_core::{Anime, FuzzyDate, ListEntry, ListStatus};
use serde_json::Value;

fn int32(value: &Value) -> i32 {
    value.as_i64().unwrap_or_default() as i32
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
fn fuzzy_date(value: &Value) -> FuzzyDate {
    let text = value.as_str().unwrap_or_default();
    match text.len() {
        4 => FuzzyDate::parse(&format!("{text}-00-00")),
        7 => FuzzyDate::parse(&format!("{text}-00")),
        _ => FuzzyDate::parse(text),
    }
}

fn age_rating(value: &str) -> AgeRating {
    match value.to_ascii_lowercase().as_str() {
        "g" => AgeRating::G,
        "pg" => AgeRating::Pg,
        "pg_13" => AgeRating::Pg13,
        "r" | "r+" => AgeRating::R17,
        "rx" => AgeRating::R18,
        _ => AgeRating::Unknown,
    }
}

fn status(value: &str) -> Status {
    match value {
        "currently_airing" => Status::Airing,
        "finished_airing" => Status::FinishedAiring,
        "not_yet_aired" => Status::NotYetAired,
        _ => Status::Unknown,
    }
}

fn media_type(value: &str) -> Type {
    match value.to_ascii_lowercase().as_str() {
        "tv" => Type::Tv,
        "ova" => Type::Ova,
        "movie" => Type::Movie,
        "special" | "tv_special" | "cm" | "pv" => Type::Special,
        "ona" => Type::Ona,
        "music" => Type::Music,
        _ => Type::Unknown,
    }
}

fn list_status(value: &str) -> ListStatus {
    match value {
        "watching" => ListStatus::Watching,
        "completed" => ListStatus::Completed,
        "on_hold" => ListStatus::OnHold,
        "dropped" => ListStatus::Dropped,
        "plan_to_watch" => ListStatus::PlanToWatch,
        _ => ListStatus::NotInList,
    }
}

/// ISO 8601 date-time (e.g. `2024-11-15T12:34:56+00:00`) to Unix seconds.
fn timestamp(value: &str) -> i64 {
    fn number(s: &str, range: std::ops::Range<usize>) -> Option<i64> {
        s.get(range)?.parse().ok()
    }
    (|| {
        let (y, m, d) = (
            number(value, 0..4)?,
            number(value, 5..7)?,
            number(value, 8..10)?,
        );
        let (h, min, s) = (
            number(value, 11..13)?,
            number(value, 14..16)?,
            number(value, 17..19)?,
        );
        // Offset: `Z`, `+HH:MM` or `-HH:MM` after the seconds (and any fraction).
        let rest = &value[19..];
        let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        let offset = match rest.chars().next() {
            Some(sign @ ('+' | '-')) => {
                let minutes = number(rest, 1..3)? * 60 + number(rest, 4..6)?;
                if sign == '+' { minutes } else { -minutes }
            }
            _ => 0,
        };
        Some(days_from_civil(y, m, d) * 86_400 + h * 3600 + min * 60 + s - offset * 60)
    })()
    .unwrap_or_default()
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses an anime `node`. `None` if it has no ID.
pub fn parse_anime(json: &Value) -> Option<Anime> {
    let id = int32(&json["id"]);
    if id == 0 {
        return None;
    }
    let alternative = &json["alternative_titles"];
    let names = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["name"].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    Some(Anime {
        id,
        last_modified: crate::now(),
        episode_count: int32(&json["num_episodes"]),
        // Seconds to minutes.
        episode_length: int32(&json["average_episode_duration"]) / 60,
        age_rating: age_rating(json["rating"].as_str().unwrap_or_default()),
        status: status(json["status"].as_str().unwrap_or_default()),
        r#type: media_type(json["media_type"].as_str().unwrap_or_default()),
        date_started: fuzzy_date(&json["start_date"]),
        date_finished: fuzzy_date(&json["end_date"]),
        score: json["mean"].as_f64().unwrap_or_default() as f32,
        popularity_rank: int32(&json["popularity"]),
        image_url: string(&json["main_picture"]["large"]),
        synopsis: string(&json["synopsis"]),
        titles: Titles {
            romaji: string(&json["title"]),
            english: string(&alternative["en"]),
            japanese: string(&alternative["ja"]),
            synonyms: alternative["synonyms"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        },
        genres: names(&json["genres"]),
        studios: names(&json["studios"]),
        ..Anime::default()
    })
}

/// Parses a `list_status` object. MyAnimeList entries have no ID of their own.
pub fn parse_list_entry(json: &Value, anime_id: i32) -> Option<ListEntry> {
    if anime_id == 0 || !json.is_object() {
        return None;
    }
    Some(ListEntry {
        id: 0,
        anime_id,
        watched_episodes: int32(&json["num_episodes_watched"]),
        // 0–10 to 0–100.
        score: int32(&json["score"]) * (patchouli_core::list::SCORE_MAX / 10),
        status: list_status(json["status"].as_str().unwrap_or_default()),
        is_private: false,
        rewatched_times: int32(&json["num_times_rewatched"]),
        rewatching: json["is_rewatching"].as_bool().unwrap_or_default(),
        rewatching_ep: 0,
        date_started: fuzzy_date(&json["start_date"]),
        date_completed: fuzzy_date(&json["finish_date"]),
        last_updated: timestamp(json["updated_at"].as_str().unwrap_or_default()),
        notes: string(&json["comments"]),
        pending_delete: false,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_anime() {
        let json = json!({
            "id": 21,
            "title": "One Piece",
            "main_picture": { "medium": "m.jpg", "large": "l.jpg" },
            "alternative_titles": { "synonyms": ["OP"], "en": "One Piece", "ja": "ONE PIECE" },
            "start_date": "1999-10",
            "end_date": "",
            "synopsis": "Pirates.",
            "mean": 8.72,
            "popularity": 20,
            "num_episodes": 0,
            "media_type": "tv",
            "status": "currently_airing",
            "genres": [{ "id": 1, "name": "Action" }],
            "average_episode_duration": 1440,
            "rating": "pg_13",
            "studios": [{ "id": 18, "name": "Toei Animation" }]
        });
        let anime = parse_anime(&json).unwrap();
        assert_eq!(anime.titles.romaji, "One Piece");
        assert_eq!(anime.titles.japanese, "ONE PIECE");
        assert_eq!(anime.titles.synonyms, ["OP"]);
        assert_eq!(anime.date_started, FuzzyDate::new(1999, 10, 0));
        assert!(anime.date_finished.is_empty());
        assert_eq!(anime.episode_length, 24);
        assert_eq!(anime.age_rating, AgeRating::Pg13);
        assert_eq!(anime.status, Status::Airing);
        assert_eq!(anime.genres, ["Action"]);
        assert_eq!(anime.studios, ["Toei Animation"]);
        assert_eq!(anime.image_url, "l.jpg");
    }

    #[test]
    fn parses_list_status() {
        let json = json!({
            "status": "completed",
            "score": 8,
            "num_episodes_watched": 12,
            "is_rewatching": false,
            "updated_at": "2024-11-15T12:00:00+09:00",
            "start_date": "2024",
            "finish_date": "2024-03-31",
            "num_times_rewatched": 1,
            "comments": "good"
        });
        let entry = parse_list_entry(&json, 21).unwrap();
        assert_eq!(entry.anime_id, 21);
        assert_eq!(entry.status, ListStatus::Completed);
        assert_eq!(entry.score, 80);
        assert_eq!(entry.watched_episodes, 12);
        assert_eq!(entry.date_started, FuzzyDate::new(2024, 0, 0));
        assert_eq!(entry.date_completed, FuzzyDate::new(2024, 3, 31));
        assert_eq!(entry.notes, "good");
        // 2024-11-15T03:00:00Z
        assert_eq!(entry.last_updated, 1_731_639_600);
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp("1970-01-01T00:00:00Z"), 0);
        assert_eq!(timestamp("2024-11-15T03:00:00.123Z"), 1_731_639_600);
        assert_eq!(timestamp("2024-11-15T00:00:00-03:00"), 1_731_639_600);
        assert_eq!(timestamp("garbage"), 0);
    }
}
