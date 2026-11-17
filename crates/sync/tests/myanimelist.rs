//! MyAnimeList client against a mock REST server.

use std::sync::Arc;

use patchouli_core::{Fields, ListEntry, ListStatus, ServiceId};
use patchouli_sync::myanimelist::{Endpoints, MyAnimeList};
use patchouli_sync::oauth::exchange_code;
use patchouli_sync::{CredentialStore, Credentials, MemoryCredentialStore};
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn credentials(access: &str) -> Credentials {
    Credentials {
        access_token: access.into(),
        refresh_token: Some("refresh".into()),
        expires_at: None,
    }
}

fn client(server: &MockServer, store: Option<Arc<dyn CredentialStore>>) -> MyAnimeList {
    MyAnimeList::with_endpoints(
        "client-id",
        credentials("old"),
        store,
        Endpoints {
            api: server.uri(),
            token: format!("{}/oauth2/token", server.uri()),
        },
    )
}

fn node(id: i32) -> serde_json::Value {
    json!({ "id": id, "title": format!("Anime {id}"), "num_episodes": 12, "media_type": "tv" })
}

#[tokio::test]
async fn fetch_list_follows_paging() {
    let server = MockServer::start().await;
    let next = format!(
        "{}/users/someone/animelist?offset=1&limit=1000",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/users/someone/animelist"))
        .and(query_param("offset", "0"))
        .and(query_param("nsfw", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "node": node(1), "list_status": {
                "status": "watching", "score": 7, "num_episodes_watched": 3,
                "is_rewatching": false, "updated_at": "2024-01-01T00:00:00+00:00"
            } }],
            "paging": { "next": next }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/someone/animelist"))
        .and(query_param("offset", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "node": node(2), "list_status": { "status": "plan_to_watch" } }],
            "paging": {}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let remote = client(&server, None).fetch_list("someone").await.unwrap();
    assert_eq!(remote.anime.len(), 2);
    assert_eq!(remote.entries.len(), 2);
    assert_eq!(remote.entries[0].score, 70);
    assert_eq!(remote.entries[0].watched_episodes, 3);
    assert_eq!(remote.entries[1].status, ListStatus::PlanToWatch);
}

#[tokio::test]
async fn expired_token_is_refreshed_saved_and_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/@me"))
        .and(header("authorization", "Bearer old"))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "www-authenticate",
            r#"Bearer error="invalid_token", error_description="The access token expired""#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_type": "Bearer", "expires_in": 2678400,
            "access_token": "new", "refresh_token": "refresh2"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/@me"))
        .and(header("authorization", "Bearer new"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": 5, "name": "someone" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let store = Arc::new(MemoryCredentialStore::default());
    let mal = client(&server, Some(store.clone()));
    let account = mal.user().await.unwrap();
    assert_eq!(account.username, "someone");

    let saved = store.load(ServiceId::MyAnimeList).unwrap().unwrap();
    assert_eq!(saved.access_token, "new");
    assert_eq!(saved.refresh_token.as_deref(), Some("refresh2"));
    assert_eq!(mal.credentials().await.access_token, "new");
}

#[tokio::test]
async fn failed_refresh_means_log_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "message": "The refresh token is invalid."
        })))
        .mount(&server)
        .await;

    let error = client(&server, None).user().await.unwrap_err();
    assert!(error.is_unauthorized(), "{error:?}");
}

#[tokio::test]
async fn save_entry_patches_form_fields() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/anime/21/my_list_status"))
        .and(body_string_contains("num_watched_episodes=3"))
        .and(body_string_contains("score=9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "watching", "score": 9, "num_episodes_watched": 3,
            "is_rewatching": false, "updated_at": "2024-01-01T00:00:00+00:00"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let entry = ListEntry {
        anime_id: 21,
        watched_episodes: 3,
        score: 95,
        status: ListStatus::Watching,
        ..ListEntry::default()
    };
    let saved = client(&server, None)
        .save_entry(&entry, Fields::EPISODE | Fields::SCORE)
        .await
        .unwrap();
    let remote = saved.entry.unwrap();
    assert_eq!(remote.score, 90);
    assert_eq!(remote.watched_episodes, 3);
}

#[tokio::test]
async fn deleting_a_missing_entry_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/anime/21/my_list_status"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let entry = ListEntry {
        anime_id: 21,
        ..ListEntry::default()
    };
    client(&server, None).delete_entry(&entry).await.unwrap();
}

#[tokio::test]
async fn fetch_missing_anime_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/anime/99999"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert_eq!(
        client(&server, None).fetch_anime(99999).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn exchanges_authorization_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains("code_verifier=verifier"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_type": "Bearer", "expires_in": 100,
            "access_token": "access", "refresh_token": "refresh"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let credentials = exchange_code(
        &reqwest::Client::new(),
        &format!("{}/oauth2/token", server.uri()),
        "client-id",
        "the-code",
        "verifier",
        "http://localhost:1/callback/myanimelist",
    )
    .await
    .unwrap();
    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.refresh_token.as_deref(), Some("refresh"));
}
