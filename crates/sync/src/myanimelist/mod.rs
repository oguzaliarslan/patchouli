//! MyAnimeList API v2 client. See
//! <https://myanimelist.net/apiconfig/references/api/v2>.
//!
//! Ported from Taiga `sync/myanimelist/*`. Access tokens expire after about a
//! month; on a 401 the client refreshes the token once, saves it, and retries.

mod parse;

use std::sync::Arc;

use patchouli_core::{Anime, Fields, ListEntry, ListStatus, SeasonName, ServiceId};
use reqwest::{Method, RequestBuilder, Response, StatusCode, Url};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::accounts::Account;
use crate::client::{RemoteList, SavedEntry};
use crate::credentials::{CredentialStore, Credentials};
use crate::search::SearchParams;
use crate::{Error, Result, http};

pub use parse::{parse_anime, parse_list_entry};

pub const API_URL: &str = "https://api.myanimelist.net/v2";
pub const AUTHORIZE_URL: &str = "https://myanimelist.net/v1/oauth2/authorize";
pub const TOKEN_URL: &str = "https://myanimelist.net/v1/oauth2/token";

const LIST_PAGE_LIMIT: u32 = 1000;
const SEARCH_PAGE_LIMIT: u32 = 100;
const SEASON_PAGE_LIMIT: u32 = 500;

const ANIME_FIELDS: &str = "alternative_titles,average_episode_duration,end_date,genres,id,\
    main_picture,mean,media_type,num_episodes,popularity,rating,start_date,status,studios,\
    synopsis,title";
const LIST_STATUS_FIELDS: &str = "comments,finish_date,is_rewatching,num_times_rewatched,\
    num_watched_episodes,score,start_date,status,updated_at";

pub fn anime_page_url(id: i32) -> String {
    format!("https://myanimelist.net/anime/{id}/")
}

/// Where the client sends requests; tests point these at a mock server.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub api: String,
    pub token: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            api: API_URL.into(),
            token: TOKEN_URL.into(),
        }
    }
}

pub struct MyAnimeList {
    http: reqwest::Client,
    endpoints: Endpoints,
    client_id: String,
    credentials: Mutex<Credentials>,
    store: Option<Arc<dyn CredentialStore>>,
}

impl MyAnimeList {
    /// `store` receives refreshed tokens.
    pub fn new(
        client_id: impl Into<String>,
        credentials: Credentials,
        store: Option<Arc<dyn CredentialStore>>,
    ) -> Self {
        Self::with_endpoints(client_id, credentials, store, Endpoints::default())
    }

    pub fn with_endpoints(
        client_id: impl Into<String>,
        credentials: Credentials,
        store: Option<Arc<dyn CredentialStore>>,
        endpoints: Endpoints,
    ) -> Self {
        Self {
            http: http::client(),
            endpoints,
            client_id: client_id.into(),
            credentials: Mutex::new(credentials),
            store,
        }
    }

    pub async fn credentials(&self) -> Credentials {
        self.credentials.lock().await.clone()
    }

    pub async fn user(&self) -> Result<Account> {
        let json = self.get_json(&["users", "@me"], &[]).await?;
        let username = json["name"].as_str().unwrap_or_default().to_owned();
        if username.is_empty() {
            return Err(Error::parse("user"));
        }
        Ok(Account {
            username,
            user_id: json["id"].as_i64().unwrap_or_default(),
            score_format: None,
            avatar_url: json["picture"].as_str().map(str::to_owned),
        })
    }

    /// Fetches every page of the user's list.
    pub async fn fetch_list(&self, username: &str) -> Result<RemoteList> {
        let fields = format!("{ANIME_FIELDS},list_status{{{LIST_STATUS_FIELDS}}}");
        let mut list = RemoteList::default();
        let mut offset = 0;
        loop {
            let offset_text = offset.to_string();
            let limit = LIST_PAGE_LIMIT.to_string();
            let query = [
                ("limit", limit.as_str()),
                ("offset", offset_text.as_str()),
                ("nsfw", "true"),
                ("fields", fields.as_str()),
            ];
            let json = self
                .get_json(&["users", username, "animelist"], &query)
                .await?;
            for item in json["data"].as_array().into_iter().flatten() {
                let Some(anime) = parse_anime(&item["node"]) else {
                    continue;
                };
                if let Some(entry) = parse_list_entry(&item["list_status"], anime.id) {
                    list.entries.push(entry);
                }
                list.anime.push(anime);
            }
            match next_offset(&json) {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        Ok(list)
    }

    /// Sends the `dirty` fields of `entry`. MyAnimeList adds the entry if needed.
    pub async fn save_entry(&self, entry: &ListEntry, dirty: Fields) -> Result<SavedEntry> {
        let form = update_form(entry, dirty);
        let id = entry.anime_id.to_string();
        let response = self
            .send(|http, token| {
                let url = self.url(&["anime", &id, "my_list_status"], &[])?;
                Ok(http
                    .request(Method::PATCH, url)
                    .bearer_auth(token)
                    .form(&form))
            })
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::Api {
                status: 404,
                message: "Anime list entry does not exist.".into(),
            });
        }
        let json = check(response).await?;
        Ok(SavedEntry {
            entry: parse_list_entry(&json, entry.anime_id),
            anime: None,
        })
    }

    pub async fn delete_entry(&self, entry: &ListEntry) -> Result<()> {
        let id = entry.anime_id.to_string();
        let response = self
            .send(|http, token| {
                let url = self.url(&["anime", &id, "my_list_status"], &[])?;
                Ok(http.delete(url).bearer_auth(token))
            })
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(()); // already gone
        }
        check(response).await.map(|_| ())
    }

    /// `None` if the anime doesn't exist.
    pub async fn fetch_anime(&self, id: i32) -> Result<Option<Anime>> {
        let id = id.to_string();
        let query = [("fields", ANIME_FIELDS)];
        let response = self
            .send(|http, token| {
                Ok(http
                    .get(self.url(&["anime", &id], &query)?)
                    .bearer_auth(token))
            })
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let json = check(response).await?;
        parse_anime(&json)
            .map(Some)
            .ok_or_else(|| Error::parse("anime"))
    }

    /// Every anime of a season (MyAnimeList returns up to 500).
    pub async fn season(&self, season: SeasonName, year: i32) -> Result<Vec<Anime>> {
        self.search(&SearchParams {
            season: Some(season),
            year: Some(year),
            ..SearchParams::default()
        })
        .await
    }

    /// First page of results. With a season and year, lists that season.
    pub async fn search(&self, params: &SearchParams) -> Result<Vec<Anime>> {
        let season = params.season.and_then(season_name);
        let json = match (season, params.year) {
            (Some(season), Some(year)) => {
                let year = year.to_string();
                let limit = SEASON_PAGE_LIMIT.to_string();
                let query = [
                    ("limit", limit.as_str()),
                    ("nsfw", "true"),
                    ("fields", ANIME_FIELDS),
                ];
                self.get_json(&["anime", "season", &year, season], &query)
                    .await?
            }
            _ => {
                let limit = SEARCH_PAGE_LIMIT.to_string();
                let query = [
                    ("q", params.text.as_str()),
                    ("limit", limit.as_str()),
                    ("nsfw", "true"),
                    ("fields", ANIME_FIELDS),
                ];
                self.get_json(&["anime"], &query).await?
            }
        };
        Ok(json["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| parse_anime(&item["node"]))
            .collect())
    }

    fn url(&self, segments: &[&str], query: &[(&str, &str)]) -> Result<Url> {
        let mut url = Url::parse(&self.endpoints.api)
            .map_err(|e| Error::NotConfigured(format!("Invalid API URL: {e}")))?;
        url.path_segments_mut()
            .map_err(|()| Error::NotConfigured("Invalid API URL.".into()))?
            .pop_if_empty()
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }

    async fn get_json(&self, segments: &[&str], query: &[(&str, &str)]) -> Result<Value> {
        let response = self
            .send(|http, token| Ok(http.get(self.url(segments, query)?).bearer_auth(token)))
            .await?;
        check(response).await
    }

    /// Sends a request, refreshing the token and retrying once on a 401.
    async fn send(
        &self,
        build: impl Fn(&reqwest::Client, &str) -> Result<RequestBuilder>,
    ) -> Result<Response> {
        let token = self.credentials.lock().await.access_token.clone();
        let response = build(&self.http, &token)?.send().await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        self.refresh().await?;
        let token = self.credentials.lock().await.access_token.clone();
        Ok(build(&self.http, &token)?.send().await?)
    }

    async fn refresh(&self) -> Result<()> {
        let mut credentials = self.credentials.lock().await;
        let Some(refresh_token) = credentials.refresh_token.clone() else {
            return Err(expired());
        };
        let new = request_token(
            &self.http,
            &self.endpoints.token,
            &[
                ("client_id", self.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
            ],
        )
        .await
        .map_err(|e| match e {
            Error::Network(_) => e,
            _ => expired(),
        })?;
        if let Some(store) = &self.store {
            store.save(ServiceId::MyAnimeList, &new)?;
        }
        *credentials = new;
        Ok(())
    }
}

fn expired() -> Error {
    Error::Unauthorized("MyAnimeList login has expired. Please log in again.".into())
}

/// Requests tokens from the OAuth token endpoint.
pub(crate) async fn request_token(
    http: &reqwest::Client,
    token_url: &str,
    form: &[(&str, &str)],
) -> Result<Credentials> {
    let response = http.post(token_url).form(form).send().await?;
    let json = check(response).await?;
    let access_token = json["access_token"].as_str().unwrap_or_default().to_owned();
    if access_token.is_empty() {
        return Err(Error::parse("authentication data"));
    }
    Ok(Credentials {
        access_token,
        refresh_token: json["refresh_token"].as_str().map(str::to_owned),
        expires_at: json["expires_in"].as_i64().map(|s| crate::now() + s),
    })
}

/// Returns the JSON body of a successful response, or the error it describes.
async fn check(response: Response) -> Result<Value> {
    let status = response.status();
    let authenticate = response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let text = response.text().await?;
    let json: Option<Value> = serde_json::from_str(&text).ok();

    if status.is_success() {
        return json.ok_or_else(|| Error::parse("response"));
    }
    let message = authenticate
        .as_deref()
        .and_then(|h| header_param(h, "error_description"))
        .or_else(|| json.as_ref().and_then(error_message))
        .unwrap_or_else(|| "MyAnimeList request failed.".into());
    if status == StatusCode::UNAUTHORIZED {
        return Err(Error::Unauthorized(message));
    }
    Err(Error::Api {
        status: status.as_u16(),
        message,
    })
}

/// A quoted parameter of a `WWW-Authenticate` header, e.g. `error="invalid_token"`.
fn header_param(header: &str, key: &str) -> Option<String> {
    let start = header.find(&format!("{key}=\""))? + key.len() + 2;
    let end = header[start..].find('"')? + start;
    Some(header[start..end].to_owned()).filter(|v| !v.is_empty())
}

fn error_message(json: &Value) -> Option<String> {
    let message = json["message"].as_str().unwrap_or_default();
    let hint = json["hint"].as_str().unwrap_or_default();
    let error = json["error"].as_str().unwrap_or_default();
    let text = match (message.is_empty(), hint.is_empty()) {
        (false, false) => format!("{message} ({hint})"),
        (false, true) => message.to_owned(),
        (true, false) => hint.to_owned(),
        (true, true) if error == "forbidden" => "Please log in to MyAnimeList again.".into(),
        (true, true) => error.to_owned(),
    };
    Some(text).filter(|t| !t.is_empty())
}

/// `offset` of the `paging.next` URL, if there's another page.
fn next_offset(json: &Value) -> Option<u32> {
    let next = Url::parse(json["paging"]["next"].as_str()?).ok()?;
    let offset = next
        .query_pairs()
        .find(|(k, _)| k == "offset")?
        .1
        .parse()
        .ok()?;
    Some(offset)
}

fn update_form(entry: &ListEntry, dirty: Fields) -> Vec<(&'static str, String)> {
    let mut form = Vec::new();
    if dirty.contains(Fields::EPISODE) {
        form.push(("num_watched_episodes", entry.watched_episodes.to_string()));
    }
    if dirty.intersects(Fields::STATUS | Fields::REWATCHING) {
        form.push(("status", list_status(entry.status).into()));
        form.push(("is_rewatching", entry.rewatching.to_string()));
    }
    if dirty.contains(Fields::SCORE) {
        // 0–100 to MyAnimeList's 0–10.
        form.push((
            "score",
            (entry.score * 10 / patchouli_core::list::SCORE_MAX).to_string(),
        ));
    }
    if dirty.contains(Fields::REWATCHED_TIMES) {
        form.push(("num_times_rewatched", entry.rewatched_times.to_string()));
    }
    if dirty.contains(Fields::DATE_STARTED) {
        form.push(("start_date", entry.date_started.to_string()));
    }
    if dirty.contains(Fields::DATE_COMPLETED) {
        form.push(("finish_date", entry.date_completed.to_string()));
    }
    if dirty.contains(Fields::NOTES) {
        form.push(("comments", entry.notes.clone()));
    }
    form
}

fn list_status(status: ListStatus) -> &'static str {
    match status {
        ListStatus::Watching | ListStatus::NotInList => "watching",
        ListStatus::Completed => "completed",
        ListStatus::OnHold => "on_hold",
        ListStatus::Dropped => "dropped",
        ListStatus::PlanToWatch => "plan_to_watch",
    }
}

fn season_name(season: SeasonName) -> Option<&'static str> {
    match season {
        SeasonName::Winter => Some("winter"),
        SeasonName::Spring => Some("spring"),
        SeasonName::Summer => Some("summer"),
        SeasonName::Fall => Some("fall"),
        SeasonName::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use patchouli_core::FuzzyDate;
    use serde_json::json;

    use super::*;

    #[test]
    fn update_form_has_only_dirty_fields() {
        let entry = ListEntry {
            anime_id: 1,
            watched_episodes: 4,
            score: 85,
            status: ListStatus::OnHold,
            date_completed: FuzzyDate::new(2024, 5, 0),
            ..ListEntry::default()
        };
        let form = update_form(
            &entry,
            Fields::EPISODE | Fields::STATUS | Fields::SCORE | Fields::DATE_COMPLETED,
        );
        assert_eq!(
            form,
            [
                ("num_watched_episodes", "4".to_owned()),
                ("status", "on_hold".to_owned()),
                ("is_rewatching", "false".to_owned()),
                ("score", "8".to_owned()),
                ("finish_date", "2024-05-00".to_owned()),
            ]
        );
    }

    #[test]
    fn reads_paging_offset() {
        let json = json!({ "paging": {
            "next": "https://api.myanimelist.net/v2/users/x/animelist?offset=1000&limit=1000"
        } });
        assert_eq!(next_offset(&json), Some(1000));
        assert_eq!(next_offset(&json!({ "paging": {} })), None);
    }

    #[test]
    fn reads_www_authenticate() {
        let header = r#"Bearer realm="", error="invalid_token", error_description="The access token expired""#;
        assert_eq!(
            header_param(header, "error").as_deref(),
            Some("invalid_token")
        );
        assert_eq!(
            header_param(header, "error_description").as_deref(),
            Some("The access token expired")
        );
        assert_eq!(header_param(header, "missing"), None);
    }

    #[test]
    fn error_messages() {
        let json = json!({ "message": "invalid q", "error": "bad_request" });
        assert_eq!(error_message(&json).as_deref(), Some("invalid q"));
        let json = json!({ "error": "forbidden" });
        assert_eq!(
            error_message(&json).as_deref(),
            Some("Please log in to MyAnimeList again.")
        );
    }
}
