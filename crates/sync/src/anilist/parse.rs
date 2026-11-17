use patchouli_core::anime::{AgeRating, Status, Titles, Type};
use patchouli_core::{Anime, FuzzyDate, ListEntry, ListStatus};
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap_or_default()
}

fn int32(value: &Value) -> i32 {
    int(value) as i32
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn strings(value: &Value) -> impl Iterator<Item = String> + '_ {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn fuzzy_date(value: &Value) -> FuzzyDate {
    FuzzyDate::new(
        int(&value["year"]) as u16,
        int(&value["month"]) as u8,
        int(&value["day"]) as u8,
    )
}

fn status(value: &str) -> Status {
    match value {
        "FINISHED" => Status::FinishedAiring,
        "RELEASING" => Status::Airing,
        // Taiga treats cancelled anime as not yet aired.
        "NOT_YET_RELEASED" | "CANCELLED" => Status::NotYetAired,
        _ => Status::Unknown,
    }
}

fn media_type(value: &str) -> Type {
    match value {
        "TV" | "TV_SHORT" => Type::Tv,
        "MOVIE" => Type::Movie,
        "SPECIAL" => Type::Special,
        "OVA" => Type::Ova,
        "ONA" => Type::Ona,
        "MUSIC" => Type::Music,
        _ => Type::Unknown,
    }
}

fn list_status(value: &str) -> ListStatus {
    match value {
        "CURRENT" | "REPEATING" => ListStatus::Watching,
        "PLANNING" => ListStatus::PlanToWatch,
        "COMPLETED" => ListStatus::Completed,
        "DROPPED" => ListStatus::Dropped,
        "PAUSED" => ListStatus::OnHold,
        _ => ListStatus::NotInList,
    }
}

/// Parses a `Media` object (`MediaFragment`). `None` if it has no ID.
pub fn parse_media(json: &Value) -> Option<Anime> {
    let id = int32(&json["id"]);
    if id == 0 {
        return None;
    }
    let mut anime = Anime {
        id,
        last_modified: crate::now(),
        episode_count: int32(&json["episodes"]),
        episode_length: int32(&json["duration"]),
        age_rating: if json["isAdult"].as_bool() == Some(true) {
            AgeRating::R18
        } else {
            AgeRating::Unknown
        },
        status: status(json["status"].as_str().unwrap_or_default()),
        r#type: media_type(json["format"].as_str().unwrap_or_default()),
        date_started: fuzzy_date(&json["startDate"]),
        date_finished: fuzzy_date(&json["endDate"]),
        score: json["averageScore"].as_f64().unwrap_or_default() as f32 / 10.0,
        popularity_rank: int32(&json["popularity"]),
        image_url: string(&json["coverImage"]["extraLarge"]),
        synopsis: string(&json["description"]),
        titles: Titles {
            romaji: string(&json["title"]["romaji"]),
            english: string(&json["title"]["english"]),
            ..Titles::default()
        },
        ..Anime::default()
    };

    // The native title is only Japanese for Japanese anime.
    let native = string(&json["title"]["native"]);
    if !native.is_empty() {
        if json["countryOfOrigin"] == "JP" {
            anime.titles.japanese = native;
        } else {
            anime.titles.synonyms.push(native);
        }
    }
    if json["trailer"]["site"] == "youtube" {
        anime.trailer_id = string(&json["trailer"]["id"]);
    }
    anime.genres.extend(strings(&json["genres"]));
    anime.titles.synonyms.extend(strings(&json["synonyms"]));
    anime.tags.extend(
        json["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|tag| tag["isMediaSpoiler"].as_bool() != Some(true))
            .map(|tag| string(&tag["name"]))
            .filter(|name| !name.is_empty()),
    );
    for edge in json["studios"]["edges"].as_array().into_iter().flatten() {
        let name = string(&edge["node"]["name"]);
        if edge["isMain"].as_bool() == Some(true) {
            anime.studios.push(name);
        } else {
            anime.producers.push(name);
        }
    }
    if json["nextAiringEpisode"].is_object() {
        anime.next_episode_time = int(&json["nextAiringEpisode"]["airingAt"]);
        anime.last_aired_episode = (int32(&json["nextAiringEpisode"]["episode"]) - 1).max(0);
    }
    Some(anime)
}

/// Parses a `MediaList` object (`MediaListFragment` with `media { id }`).
/// `None` if it has no media ID.
pub fn parse_list_entry(json: &Value) -> Option<ListEntry> {
    let anime_id = int32(&json["media"]["id"]);
    if anime_id == 0 {
        return None;
    }
    let status = json["status"].as_str().unwrap_or_default();
    Some(ListEntry {
        id: int(&json["id"]),
        anime_id,
        watched_episodes: int32(&json["progress"]),
        // Requested with `score(format: POINT_100)`.
        score: json["score"].as_f64().unwrap_or_default() as i32,
        status: list_status(status),
        is_private: json["private"].as_bool().unwrap_or_default(),
        rewatched_times: int32(&json["repeat"]),
        rewatching: status == "REPEATING",
        rewatching_ep: 0,
        date_started: fuzzy_date(&json["startedAt"]),
        date_completed: fuzzy_date(&json["completedAt"]),
        last_updated: int(&json["updatedAt"]),
        notes: string(&json["notes"]),
        pending_delete: false,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_media() {
        let json = json!({
            "id": 21,
            "title": { "romaji": "ONE PIECE", "english": "One Piece", "native": "ONE PIECE" },
            "format": "TV",
            "status": "RELEASING",
            "description": "Pirates.",
            "startDate": { "year": 1999, "month": 10, "day": 20 },
            "endDate": { "year": null, "month": null, "day": null },
            "episodes": null,
            "duration": 24,
            "isAdult": false,
            "countryOfOrigin": "JP",
            "trailer": { "id": "abc", "site": "youtube" },
            "coverImage": { "extraLarge": "https://img/21.jpg" },
            "genres": ["Action", "Adventure"],
            "synonyms": ["OP", ""],
            "averageScore": 87,
            "popularity": 600000,
            "tags": [
                { "name": "Pirates", "isMediaSpoiler": false },
                { "name": "Secret", "isMediaSpoiler": true }
            ],
            "studios": { "edges": [
                { "isMain": true, "node": { "name": "Toei Animation" } },
                { "isMain": false, "node": { "name": "Fuji TV" } }
            ] },
            "nextAiringEpisode": { "airingAt": 1731700000, "episode": 1101 }
        });
        let anime = parse_media(&json).unwrap();
        assert_eq!(anime.id, 21);
        assert_eq!(anime.titles.japanese, "ONE PIECE");
        assert_eq!(anime.titles.synonyms, ["OP"]);
        assert_eq!(anime.r#type, Type::Tv);
        assert_eq!(anime.status, Status::Airing);
        assert_eq!(anime.episode_count, 0);
        assert_eq!(anime.date_started, FuzzyDate::new(1999, 10, 20));
        assert!(anime.date_finished.is_empty());
        assert!((anime.score - 8.7).abs() < 1e-5);
        assert_eq!(anime.trailer_id, "abc");
        assert_eq!(anime.tags, ["Pirates"]);
        assert_eq!(anime.studios, ["Toei Animation"]);
        assert_eq!(anime.producers, ["Fuji TV"]);
        assert_eq!(anime.last_aired_episode, 1100);
        assert_eq!(anime.next_episode_time, 1_731_700_000);
    }

    #[test]
    fn native_title_of_non_japanese_anime_is_a_synonym() {
        let json = json!({ "id": 1, "title": { "native": "原神" }, "countryOfOrigin": "CN" });
        let anime = parse_media(&json).unwrap();
        assert_eq!(anime.titles.japanese, "");
        assert_eq!(anime.titles.synonyms, ["原神"]);
    }

    #[test]
    fn parses_list_entry() {
        let json = json!({
            "id": 5001,
            "status": "REPEATING",
            "score": 85.0,
            "progress": 3,
            "repeat": 1,
            "private": true,
            "notes": null,
            "startedAt": { "year": 2024, "month": 1, "day": null },
            "completedAt": { "year": null, "month": null, "day": null },
            "updatedAt": 1731700000,
            "media": { "id": 21 }
        });
        let entry = parse_list_entry(&json).unwrap();
        assert_eq!(entry.id, 5001);
        assert_eq!(entry.anime_id, 21);
        assert_eq!(entry.status, ListStatus::Watching);
        assert!(entry.rewatching);
        assert_eq!(entry.score, 85);
        assert!(entry.is_private);
        assert_eq!(entry.notes, "");
        assert_eq!(entry.date_started, FuzzyDate::new(2024, 1, 0));
        assert!(parse_list_entry(&json!({ "media": {} })).is_none());
    }
}
