//! Copying changes from AniList to MyAnimeList, against mock servers.

use std::sync::{Arc, Mutex};

use patchouli_core::{ListEntry, ListStatus, ServiceId};
use patchouli_db::queue::mirror as mirror_queue;
use patchouli_db::{Connection, id_map, list};
use patchouli_sync::anilist::AniList;
use patchouli_sync::myanimelist::{Endpoints, MyAnimeList};
use patchouli_sync::{Account, Client, Credentials, Synchronizer};
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn account() -> Account {
    Account {
        username: "someone".into(),
        ..Account::default()
    }
}

/// Main list on AniList, copied to MyAnimeList.
async fn mirrored() -> (Synchronizer, Arc<Mutex<Connection>>, MockServer, MockServer) {
    let anilist = MockServer::start().await;
    let mal = MockServer::start().await;
    let db = Arc::new(Mutex::new(patchouli_db::open_in_memory().unwrap()));
    let sync = Synchronizer::new(Arc::clone(&db), |_| {});
    sync.set_session(Some((
        Client::AniList(AniList::with_url("tok", anilist.uri())),
        account(),
    )));
    let credentials = Credentials {
        access_token: "mal".into(),
        refresh_token: None,
        expires_at: None,
    };
    sync.set_mirror_session(Some((
        Client::MyAnimeList(MyAnimeList::with_endpoints(
            "client-id",
            credentials,
            None,
            Endpoints {
                api: mal.uri(),
                token: format!("{}/oauth2/token", mal.uri()),
            },
        )),
        account(),
    )));
    sync.set_mirroring(true);
    (sync, db, anilist, mal)
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
async fn copies_saves_and_deletes_under_matching_ids() {
    let (sync, db, anilist, mal) = mirrored().await;
    // 154587 is 52991 on MyAnimeList; 999 has no match.
    Mock::given(method("POST"))
        .and(body_string_contains("idMal_in"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "Page": {
            "media": [{ "id": 154587, "idMal": 52991 }, { "id": 999, "idMal": null }]
        } } })),
        )
        .expect(1)
        .mount(&anilist)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/anime/52991/my_list_status"))
        .and(body_string_contains("num_watched_episodes=3"))
        .and(body_string_contains("status=watching"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "watching", "num_episodes_watched": 3, "score": 0
        })))
        .expect(1)
        .mount(&mal)
        .await;

    sync.save_entry(watching(154587, 3)).unwrap();
    sync.save_entry(watching(999, 1)).unwrap();
    // The main queue isn't sent here; the mirror doesn't wait for it.
    sync.process_mirror().await.unwrap();

    let conn = db.lock().unwrap();
    let left = mirror_queue::all(&conn).unwrap();
    assert_eq!(left.len(), 1, "only the unmatched anime stays queued");
    assert_eq!(left[0].anime_id, 999);
    assert_eq!(
        left[0].last_error.as_deref(),
        Some("Not found on MyAnimeList.")
    );
    assert_eq!(
        id_map::get_many(&conn, ServiceId::AniList, ServiceId::MyAnimeList, &[154587]).unwrap()
            [&154587],
        52991
    );
    drop(conn);

    // Removing it deletes it there too, with the ID already known.
    Mock::given(method("DELETE"))
        .and(path("/anime/52991/my_list_status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&mal)
        .await;
    sync.remove_entry(154587).unwrap();
    assert!(
        list::get(&db.lock().unwrap(), 154587)
            .unwrap()
            .unwrap()
            .pending_delete
    );
    sync.process_mirror().await.unwrap();
    let left = mirror_queue::all(&db.lock().unwrap()).unwrap();
    assert_eq!(left.iter().map(|i| i.anime_id).collect::<Vec<_>>(), [999]);

    assert_eq!(sync.mirror_dismiss_failed().unwrap(), 1);
    assert!(mirror_queue::all(&db.lock().unwrap()).unwrap().is_empty());
}

#[tokio::test]
async fn deleting_on_anilist_looks_up_the_entry() {
    // Main list on MyAnimeList, copied to AniList.
    let anilist = MockServer::start().await;
    let mal = MockServer::start().await;
    let db = Arc::new(Mutex::new(patchouli_db::open_in_memory().unwrap()));
    let sync = Synchronizer::new(Arc::clone(&db), |_| {});
    sync.set_session(Some((
        Client::MyAnimeList(MyAnimeList::with_endpoints(
            "client-id",
            Credentials {
                access_token: "mal".into(),
                refresh_token: None,
                expires_at: None,
            },
            None,
            Endpoints {
                api: mal.uri(),
                token: format!("{}/oauth2/token", mal.uri()),
            },
        )),
        account(),
    )));
    sync.set_mirror_session(Some((
        Client::AniList(AniList::with_url("tok", anilist.uri())),
        account(),
    )));
    sync.set_mirroring(true);
    id_map::set_many(
        &db.lock().unwrap(),
        ServiceId::MyAnimeList,
        ServiceId::AniList,
        &[(52991, 154587)],
    )
    .unwrap();

    Mock::given(method("POST"))
        .and(body_string_contains("MediaList("))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "MediaList": { "id": 4242 } } })),
        )
        .expect(1)
        .mount(&anilist)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("DeleteMediaListEntry"))
        .and(body_string_contains("4242"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "DeleteMediaListEntry": { "deleted": true } } })),
        )
        .expect(1)
        .mount(&anilist)
        .await;

    list::upsert(&db.lock().unwrap(), &watching(52991, 2)).unwrap();
    sync.remove_entry(52991).unwrap();
    sync.process_mirror().await.unwrap();
    assert!(mirror_queue::all(&db.lock().unwrap()).unwrap().is_empty());
}
