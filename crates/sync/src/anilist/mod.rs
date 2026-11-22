//! AniList GraphQL client. See <https://docs.anilist.co/>.
//!
//! Ported from Taiga `sync/anilist/*`.

mod parse;

use std::collections::HashSet;

use patchouli_core::anime::{Status, Type};
use patchouli_core::{Anime, Fields, FuzzyDate, ListEntry, ListStatus, SeasonName};
use serde_json::{Value, json};

use crate::accounts::Account;
use crate::client::{RemoteList, SavedEntry};
use crate::search::{SearchParams, SearchSort};
use crate::{Error, Result, gql, http};

/// Pages of 50 fetched for a season; no season has had more than 500 anime.
const SEASON_MAX_PAGES: u32 = 10;

pub use parse::{parse_list_entry, parse_media};

pub const API_URL: &str = "https://graphql.anilist.co";
pub const AUTHORIZE_URL: &str = "https://anilist.co/api/v2/oauth/authorize";

pub fn anime_page_url(id: i32) -> String {
    format!("https://anilist.co/anime/{id}")
}

pub struct AniList {
    http: reqwest::Client,
    url: String,
    token: String,
}

impl AniList {
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_url(token, API_URL)
    }

    /// Uses another endpoint, for tests.
    pub fn with_url(token: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            http: http::client(),
            url: url.into(),
            token: token.into(),
        }
    }

    pub async fn user(&self) -> Result<Account> {
        let data = self.query(gql::VIEWER, json!({})).await?;
        let viewer = &data["Viewer"];
        let username = viewer["name"].as_str().unwrap_or_default().to_owned();
        if username.is_empty() {
            return Err(Error::parse("user"));
        }
        Ok(Account {
            username,
            user_id: viewer["id"].as_i64().unwrap_or_default(),
            score_format: viewer["mediaListOptions"]["scoreFormat"]
                .as_str()
                .map(str::to_owned),
            avatar_url: viewer["avatar"]["large"].as_str().map(str::to_owned),
        })
    }

    pub async fn fetch_list(&self, username: &str) -> Result<RemoteList> {
        let data = self
            .query(gql::MEDIA_LIST_COLLECTION, json!({ "userName": username }))
            .await?;
        let lists = data["MediaListCollection"]["lists"]
            .as_array()
            .ok_or_else(|| Error::parse("anime list"))?;

        // An entry appears once per custom list it's in, so skip repeats.
        let mut seen = HashSet::new();
        let mut list = RemoteList::default();
        for value in lists
            .iter()
            .filter_map(|l| l["entries"].as_array())
            .flatten()
        {
            let Some(entry) = parse_list_entry(value) else {
                continue;
            };
            if !seen.insert(entry.anime_id) {
                continue;
            }
            if let Some(anime) = parse_media(&value["media"]) {
                list.anime.push(anime);
            }
            list.entries.push(entry);
        }
        Ok(list)
    }

    /// Sends the `dirty` fields of `entry`. Adds the entry if it has no ID yet.
    pub async fn save_entry(&self, entry: &ListEntry, dirty: Fields) -> Result<SavedEntry> {
        let data = self
            .query(gql::SAVE_MEDIA_LIST_ENTRY, save_variables(entry, dirty))
            .await?;
        let saved = &data["SaveMediaListEntry"];
        if !saved.is_object() {
            return Err(Error::parse("list entry"));
        }
        Ok(SavedEntry {
            entry: parse_list_entry(saved),
            anime: parse_media(&saved["media"]),
        })
    }

    pub async fn delete_entry(&self, entry: &ListEntry) -> Result<()> {
        if entry.id == 0 {
            return Ok(()); // never reached the server
        }
        match self
            .query(gql::DELETE_MEDIA_LIST_ENTRY, json!({ "id": entry.id }))
            .await
        {
            Ok(_) | Err(Error::Api { status: 404, .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// `None` if the anime doesn't exist.
    pub async fn fetch_anime(&self, id: i32) -> Result<Option<Anime>> {
        match self.query(gql::MEDIA, json!({ "id": id })).await {
            Ok(data) => parse_media(&data["Media"])
                .map(Some)
                .ok_or_else(|| Error::parse("media")),
            Err(Error::Api { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// First page of results.
    pub async fn search(&self, params: &SearchParams) -> Result<Vec<Anime>> {
        let data = self
            .query(gql::MEDIA_SEARCH, search_variables(params))
            .await?;
        let media = data["Page"]["media"]
            .as_array()
            .ok_or_else(|| Error::parse("search results"))?;
        Ok(media.iter().filter_map(parse_media).collect())
    }

    /// Every anime of a season, most popular first (up to 10 pages).
    pub async fn season(&self, season: SeasonName, year: i32) -> Result<Vec<Anime>> {
        let mut params = SearchParams {
            season: Some(season),
            year: Some(year),
            ..SearchParams::default()
        };
        params.sort = None;
        let mut all = Vec::new();
        for page in 1..=SEASON_MAX_PAGES {
            let mut variables = search_variables(&params);
            variables["page"] = json!(page);
            variables["sort"] = json!(["POPULARITY_DESC"]);
            let data = self.query(gql::MEDIA_SEARCH, variables).await?;
            let media = data["Page"]["media"]
                .as_array()
                .ok_or_else(|| Error::parse("season"))?;
            all.extend(media.iter().filter_map(parse_media));
            if data["Page"]["pageInfo"]["hasNextPage"] != json!(true) {
                break;
            }
        }
        Ok(all)
    }

    /// Matches anime between AniList and MyAnimeList, as `(anilist, myanimelist)`
    /// pairs. `ids` are AniList IDs, or MyAnimeList IDs if `from_mal`. Anime
    /// with no match on the other service are left out.
    pub async fn match_ids(&self, ids: &[i32], from_mal: bool) -> Result<Vec<(i32, i32)>> {
        let mut pairs = Vec::new();
        for chunk in ids.chunks(50) {
            let variables = if from_mal {
                json!({ "malIds": chunk })
            } else {
                json!({ "ids": chunk })
            };
            let data = self.query(gql::MEDIA_IDS, variables).await?;
            let media = data["Page"]["media"]
                .as_array()
                .ok_or_else(|| Error::parse("anime IDs"))?;
            pairs.extend(media.iter().filter_map(|m| {
                let id = m["id"].as_i64()? as i32;
                let mal = m["idMal"].as_i64()? as i32;
                (id > 0 && mal > 0).then_some((id, mal))
            }));
        }
        Ok(pairs)
    }

    /// The ID of `username`'s list entry for `media_id`, if there is one.
    pub async fn entry_id(&self, username: &str, media_id: i32) -> Result<Option<i64>> {
        match self
            .query(
                gql::MEDIA_LIST_ID,
                json!({ "userName": username, "mediaId": media_id }),
            )
            .await
        {
            Ok(data) => Ok(data["MediaList"]["id"].as_i64().filter(|&id| id > 0)),
            Err(Error::Api { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Runs a query and returns its `data`.
    async fn query(&self, document: &str, variables: Value) -> Result<Value> {
        let body = json!({ "query": with_fragments(document), "variables": variables });
        let response = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        let json: Value = serde_json::from_str(&text).map_err(|_| Error::Api {
            status: status.as_u16(),
            message: "Unexpected response from AniList.".into(),
        })?;

        if let Some(message) = error_message(&json) {
            if status == reqwest::StatusCode::UNAUTHORIZED || message == "Invalid token" {
                return Err(Error::Unauthorized(
                    "AniList login has expired. Please log in again.".into(),
                ));
            }
            return Err(Error::Api {
                status: status.as_u16(),
                message,
            });
        }
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                message: "AniList request failed.".into(),
            });
        }
        Ok(json["data"].clone())
    }
}

/// Appends the fragments a document uses.
fn with_fragments(document: &str) -> String {
    let mut query = document.to_owned();
    for (name, fragment) in [
        ("MediaFragment", gql::MEDIA_FRAGMENT),
        ("MediaListFragment", gql::MEDIA_LIST_FRAGMENT),
    ] {
        if document.contains(&format!("...{name}")) {
            query.push('\n');
            query.push_str(fragment);
        }
    }
    query
}

/// First error message, rewriting validation errors like Taiga does.
fn error_message(json: &Value) -> Option<String> {
    json["errors"].as_array()?.iter().find_map(|error| {
        let validation = error["validation"]
            .as_object()
            .and_then(|v| v.values().next())
            .and_then(|messages| messages[0].as_str());
        match validation {
            Some(message) => Some(format!("Validation error: {message}")),
            None => error["message"]
                .as_str()
                .filter(|m| !m.is_empty())
                .map(str::to_owned),
        }
    })
}

fn save_variables(entry: &ListEntry, dirty: Fields) -> Value {
    let mut variables = json!({ "mediaId": entry.anime_id });
    if entry.id != 0 {
        variables["id"] = json!(entry.id);
    }
    if dirty.intersects(Fields::STATUS | Fields::REWATCHING) {
        variables["status"] = json!(if entry.rewatching {
            "REPEATING"
        } else {
            list_status(entry.status)
        });
    }
    if dirty.contains(Fields::SCORE) {
        variables["scoreRaw"] = json!(entry.score);
    }
    if dirty.contains(Fields::EPISODE) {
        variables["progress"] = json!(entry.watched_episodes);
    }
    if dirty.contains(Fields::REWATCHED_TIMES) {
        variables["repeat"] = json!(entry.rewatched_times);
    }
    if dirty.contains(Fields::NOTES) {
        variables["notes"] = json!(entry.notes);
    }
    if dirty.contains(Fields::DATE_STARTED) {
        variables["startedAt"] = fuzzy_date(entry.date_started);
    }
    if dirty.contains(Fields::DATE_COMPLETED) {
        variables["completedAt"] = fuzzy_date(entry.date_completed);
    }
    variables
}

fn search_variables(params: &SearchParams) -> Value {
    let mut variables = json!({ "page": 1, "sort": [search_sort(params)] });
    if !params.text.is_empty() {
        variables["query"] = json!(params.text);
    }
    if let Some(season) = params.season.and_then(season_name) {
        variables["season"] = json!(season);
    }
    if let Some(year) = params.year {
        variables["seasonYear"] = json!(year);
    }
    if let Some(format) = params.r#type.and_then(media_format) {
        variables["format"] = json!(format);
    }
    if let Some(status) = params.status.and_then(media_status) {
        variables["status"] = json!(status);
    }
    variables
}

/// Unknown parts are sent as `null`.
fn fuzzy_date(date: FuzzyDate) -> Value {
    let part = |v: u32| if v == 0 { Value::Null } else { json!(v) };
    json!({
        "year": part(date.year.into()),
        "month": part(date.month.into()),
        "day": part(date.day.into()),
    })
}

fn list_status(status: ListStatus) -> &'static str {
    match status {
        ListStatus::Watching | ListStatus::NotInList => "CURRENT",
        ListStatus::Completed => "COMPLETED",
        ListStatus::OnHold => "PAUSED",
        ListStatus::Dropped => "DROPPED",
        ListStatus::PlanToWatch => "PLANNING",
    }
}

fn search_sort(params: &SearchParams) -> &'static str {
    if !params.text.is_empty() {
        return "SEARCH_MATCH";
    }
    let (ascending, descending) = match params.sort {
        Some(SearchSort::Title) => ("TITLE_ROMAJI", "TITLE_ROMAJI_DESC"),
        Some(SearchSort::Duration) => ("DURATION", "DURATION_DESC"),
        Some(SearchSort::Score) => ("SCORE", "SCORE_DESC"),
        Some(SearchSort::Type) => ("FORMAT", "FORMAT_DESC"),
        Some(SearchSort::StartDate) => ("START_DATE", "START_DATE_DESC"),
        None => return "START_DATE",
    };
    if params.descending {
        descending
    } else {
        ascending
    }
}

fn season_name(season: SeasonName) -> Option<&'static str> {
    match season {
        SeasonName::Winter => Some("WINTER"),
        SeasonName::Spring => Some("SPRING"),
        SeasonName::Summer => Some("SUMMER"),
        SeasonName::Fall => Some("FALL"),
        SeasonName::Unknown => None,
    }
}

fn media_format(r#type: Type) -> Option<&'static str> {
    match r#type {
        Type::Tv => Some("TV"),
        Type::Ova => Some("OVA"),
        Type::Movie => Some("MOVIE"),
        Type::Special => Some("SPECIAL"),
        Type::Ona => Some("ONA"),
        Type::Music => Some("MUSIC"),
        Type::Unknown => None,
    }
}

fn media_status(status: Status) -> Option<&'static str> {
    match status {
        Status::FinishedAiring => Some("FINISHED"),
        Status::Airing => Some("RELEASING"),
        Status::NotYetAired => Some("NOT_YET_RELEASED"),
        Status::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_used_fragments() {
        let query = with_fragments(gql::SAVE_MEDIA_LIST_ENTRY);
        assert!(query.contains("fragment MediaFragment"));
        assert!(query.contains("fragment MediaListFragment"));
        assert!(!with_fragments(gql::VIEWER).contains("fragment"));
    }

    #[test]
    fn save_sends_only_dirty_fields() {
        let entry = ListEntry {
            id: 9,
            anime_id: 21,
            watched_episodes: 5,
            score: 85,
            status: ListStatus::Watching,
            rewatching: true,
            date_started: FuzzyDate::new(2024, 4, 0),
            ..ListEntry::default()
        };
        let variables = save_variables(
            &entry,
            Fields::EPISODE | Fields::STATUS | Fields::DATE_STARTED,
        );
        assert_eq!(
            variables,
            json!({
                "id": 9,
                "mediaId": 21,
                "progress": 5,
                "status": "REPEATING",
                "startedAt": { "year": 2024, "month": 4, "day": null },
            })
        );
    }

    #[test]
    fn new_entries_have_no_id() {
        let entry = ListEntry {
            anime_id: 21,
            ..ListEntry::default()
        };
        assert_eq!(
            save_variables(&entry, Fields::SCORE),
            json!({ "mediaId": 21, "scoreRaw": 0 })
        );
    }

    #[test]
    fn error_messages() {
        let json = json!({ "errors": [{ "message": "Invalid token", "status": 400 }] });
        assert_eq!(error_message(&json).as_deref(), Some("Invalid token"));
        let json = json!({ "errors": [{
            "message": "validation",
            "validation": { "progress": ["The progress must be an integer."] }
        }] });
        assert_eq!(
            error_message(&json).as_deref(),
            Some("Validation error: The progress must be an integer.")
        );
        assert_eq!(error_message(&json!({ "data": {} })), None);
    }
}
