//! AniList client and sync engine against a mock GraphQL server.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tracker_core::{Fields, ListEntry, ListStatus};
use tracker_db::{Connection, list, queue};
use tracker_sync::anilist::AniList;
use tracker_sync::{Account, Client, Error, Synchronizer};
use wiremock::matchers::{body_partial_json, body_string_contains, header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn media(id: i32) -> Value {
    json!({ "id": id, "title": { "romaji": format!("Anime {id}") }, "format": "TV", "episodes": 12 })
}

fn list_entry(id: i64, anime_id: i32, progress: i32) -> Value {
    json!({
        "id": id, "status": "CURRENT", "score": 0, "progress": progress, "repeat": 0,
        "private": false, "notes": null, "updatedAt": 1700000000,
        "startedAt": {}, "completedAt": {}, "media": media(anime_id)
    })
}

fn client(server: &MockServer) -> AniList {
    AniList::with_url("tok", server.uri())
}

/// A synchronizer logged in to the mock server, and its database.
fn synchronizer(server: &MockServer) -> (Synchronizer, Arc<Mutex<Connection>>) {
    let db = Arc::new(Mutex::new(tracker_db::open_in_memory().unwrap()));
    let sync = Synchronizer::new(Arc::clone(&db), |_| {});
    let account = Account {
        username: "someone".into(),
        ..Account::default()
    };
    sync.set_session(Some((Client::AniList(client(server)), account)));
    (sync, db)
}

fn watching(anime_id: i32, episodes: i32) -> ListEntry {
    ListEntry {
        anime_id,
        watched_episodes: episodes,
        status: ListStatus::Watching,
        ..ListEntry::default()
    }
}

#[tokio::test]
async fn reads_viewer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("authorization", "Bearer tok"))
        .and(body_string_contains("Viewer"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "Viewer": {
            "id": 7, "name": "someone", "mediaListOptions": { "scoreFormat": "POINT_10" }
        } } })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let account = client(&server).user().await.unwrap();
    assert_eq!(account.username, "someone");
    assert_eq!(account.user_id, 7);
    assert_eq!(account.score_format.as_deref(), Some("POINT_10"));
}

#[tokio::test]
async fn fetch_list_skips_entries_repeated_in_custom_lists() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(
            json!({ "variables": { "userName": "someone" } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "MediaListCollection": { "lists": [
                { "entries": [list_entry(1, 10, 3), list_entry(2, 20, 0)] },
                { "entries": [list_entry(1, 10, 3)] }
            ] }
        } })))
        .mount(&server)
        .await;

    let remote = client(&server).fetch_list("someone").await.unwrap();
    assert_eq!(remote.entries.len(), 2);
    assert_eq!(remote.anime.len(), 2);
    assert_eq!(remote.entries[0].id, 1);
    assert_eq!(remote.entries[0].watched_episodes, 3);
    assert_eq!(remote.anime[0].titles.romaji, "Anime 10");
}

#[tokio::test]
async fn invalid_token_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errors": [{ "message": "Invalid token", "status": 400 }], "data": null
        })))
        .mount(&server)
        .await;

    let error = client(&server).user().await.unwrap_err();
    assert!(error.is_unauthorized(), "{error:?}");
}

#[tokio::test]
async fn non_json_response_is_an_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("<html>down</html>"))
        .mount(&server)
        .await;

    let error = client(&server).user().await.unwrap_err();
    assert!(matches!(error, Error::Api { status: 503, .. }), "{error:?}");
}

#[tokio::test]
async fn queue_sends_dirty_fields_and_stores_the_new_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("SaveMediaListEntry"))
        .and(body_partial_json(json!({ "variables": {
            "mediaId": 10, "progress": 1, "status": "CURRENT"
        } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "SaveMediaListEntry": list_entry(555, 10, 1)
        } })))
        .expect(1)
        .mount(&server)
        .await;

    let (sync, db) = synchronizer(&server);
    sync.save_entry(watching(10, 1)).unwrap();
    sync.process_queue().await.unwrap();

    let db = db.lock().unwrap();
    assert!(queue::all(&db).unwrap().is_empty());
    let entry = list::get(&db, 10).unwrap().unwrap();
    assert_eq!(entry.id, 555);
    // AniList returns the anime too.
    assert_eq!(
        tracker_db::anime::get(&db, 10)
            .unwrap()
            .unwrap()
            .titles
            .romaji,
        "Anime 10"
    );
}

#[tokio::test]
async fn failed_item_stays_queued_with_its_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "errors": [{ "message": "Internal Server Error", "status": 500 }]
        })))
        .mount(&server)
        .await;

    let (sync, db) = synchronizer(&server);
    sync.save_entry(watching(10, 1)).unwrap();
    let error = sync.process_queue().await.unwrap_err();
    assert!(matches!(error, Error::Api { status: 500, .. }));

    let item = queue::get(&db.lock().unwrap(), 10).unwrap().unwrap();
    assert_eq!(item.retry_count, 1);
    assert_eq!(item.dirty, Fields::EPISODE | Fields::STATUS);
    assert!(item.last_error.unwrap().contains("Internal Server Error"));
}

#[tokio::test]
async fn delete_removes_the_entry_after_the_server_confirms() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({ "variables": { "id": 77 } })))
        .and(body_string_contains("DeleteMediaListEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "DeleteMediaListEntry": { "deleted": true }
        } })))
        .expect(1)
        .mount(&server)
        .await;

    let (sync, db) = synchronizer(&server);
    list::upsert(
        &db.lock().unwrap(),
        &ListEntry {
            id: 77,
            ..watching(10, 5)
        },
    )
    .unwrap();
    sync.remove_entry(10).unwrap();
    sync.process_queue().await.unwrap();

    let db = db.lock().unwrap();
    assert_eq!(list::get(&db, 10).unwrap(), None);
    assert!(queue::all(&db).unwrap().is_empty());
}

#[tokio::test]
async fn synchronize_prunes_entries_missing_remotely_but_keeps_queued_ones() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("SaveMediaListEntry"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "errors": [{ "message": "down" }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("MediaListCollection"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "MediaListCollection": { "lists": [{ "entries": [list_entry(1, 10, 3)] }] }
        } })))
        .mount(&server)
        .await;

    let (sync, db) = synchronizer(&server);
    {
        let db = db.lock().unwrap();
        list::upsert(
            &db,
            &ListEntry {
                id: 2,
                ..watching(20, 1)
            },
        )
        .unwrap(); // gone remotely
        list::upsert(
            &db,
            &ListEntry {
                id: 1,
                ..watching(10, 1)
            },
        )
        .unwrap(); // updated remotely
    }

    // With a failing queue, synchronize stops before fetching.
    sync.save_entry(watching(30, 2)).unwrap();
    assert!(sync.synchronize().await.is_err());

    // Fetching alone keeps the queued entry and prunes the stale one.
    sync.fetch_list().await.unwrap();
    let db = db.lock().unwrap();
    let ids: Vec<i32> = list::all(&db).unwrap().iter().map(|e| e.anime_id).collect();
    assert_eq!(ids, [10, 30]);
    assert_eq!(list::get(&db, 10).unwrap().unwrap().watched_episodes, 3);
}
