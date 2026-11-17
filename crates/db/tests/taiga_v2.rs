//! Opens a database laid out the way Taiga v2 writes `media.sqlite` and checks
//! that it migrates and reads back correctly.

use std::path::PathBuf;

use rusqlite::Connection;
use tracker_core::anime::{Status, Type};
use tracker_core::{Fields, FuzzyDate, ListStatus};
use tracker_db::{anime, anime_settings, history, list, meta, migrations, queue};

const TAIGA_SCHEMA: &[&str] = &[
    include_str!("../sql/createMeta.sql"),
    include_str!("../sql/createAnime.sql"),
    include_str!("../sql/createAnimeList.sql"),
    include_str!("../sql/createAnimeSettings.sql"),
    include_str!("../sql/createHistory.sql"),
    include_str!("../sql/createQueue.sql"),
];

/// Taiga binds most numbers with `QString::number`, so they are stored as text.
const TAIGA_DATA: &str = r#"
INSERT INTO meta(name, value) VALUES('version', '2.0.0');

INSERT INTO anime VALUES(
  21, 'One Piece', 'One Piece', 'ワンピース', '["OP"]', 1, 2, -1, 24,
  '1999-10-20', '0000-00-00', 'https://img/op.jpg', '', 3,
  '["Action","Adventure"]', '[]', '["Fuji TV"]', '["Toei Animation"]',
  '8.72', 20, 'Pirates.', 1100, '1731700000', '1731600000'
);
INSERT INTO anime VALUES(
  30, 'Neon Genesis Evangelion', NULL, NULL, NULL, 1, 1, 26, 24,
  '1995-10-04', '1996-03-27', NULL, NULL, 0, NULL, NULL, NULL, NULL,
  NULL, NULL, NULL, NULL, NULL, NULL
);
INSERT INTO anime VALUES(
  99, 'Not Yet Synced', '', '', 'not json', 0, 0, 0, 0,
  '', '', '', '', 0, '', '', '', '', '', 0, '', 0, '0', '0'
);

-- Synced entry with a service ID.
INSERT INTO anime_list VALUES(
  5001, 21, 1100, '2020-01-00', '0000-00-00', 90, 1, 0, 0, 0, 0, 'Still going',
  '1731700000', 0
);
-- Two unsynced entries with ID 0: in Taiga's schema the second one replaced
-- the first, so only the last survives in the file.
INSERT OR REPLACE INTO anime_list VALUES(
  0, 99, 1, '0000-00-00', '0000-00-00', 0, 5, 0, 0, 0, 0, '', '0', 0
);
INSERT OR REPLACE INTO anime_list VALUES(
  0, 30, 26, '2001-01-01', '2001-02-03', 100, 2, 1, 2, 0, 0, NULL, '1600000000', 1
);

INSERT INTO anime_settings VALUES(21, 'OP', 'D:\Anime\One Piece', '["Wan Pisu"]');

INSERT INTO history(anime_id, episode, time) VALUES(21, 1099, 1731000000);
INSERT INTO history(anime_id, episode, time) VALUES(21, 1100, 1731700000);
-- History may refer to anime that are no longer in the database.
INSERT INTO history(anime_id, episode, time) VALUES(12345, 3, 1600000000);

INSERT INTO queue VALUES(21, 5, 1731700001, 1, 'Connection timed out');
INSERT INTO queue VALUES(30, 64, 1731700000, 0, NULL);
"#;

struct TempDb(PathBuf);

impl TempDb {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tracker-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(tracker_db::FILE_NAME);
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn write_taiga_db(path: &PathBuf) {
    let conn = Connection::open(path).unwrap();
    // Qt's SQLite driver leaves foreign keys off, which Taiga's schema relies on.
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    for sql in TAIGA_SCHEMA {
        conn.execute_batch(sql).unwrap();
    }
    conn.execute_batch(TAIGA_DATA).unwrap();
}

#[test]
fn opens_and_migrates_a_taiga_database() {
    let db = TempDb::new("taiga-v2");
    write_taiga_db(&db.0);

    let conn = tracker_db::open(&db.0).unwrap();
    assert_eq!(
        migrations::version(&conn).unwrap(),
        migrations::SCHEMA_VERSION
    );
    assert_eq!(
        meta::get(&conn, "version").unwrap().as_deref(),
        Some("2.0.0")
    );

    // Anime: text-encoded numbers, JSON lists and NULLs.
    let op = anime::get(&conn, 21).unwrap().unwrap();
    assert_eq!(op.titles.romaji, "One Piece");
    assert_eq!(op.titles.japanese, "ワンピース");
    assert_eq!(op.titles.synonyms, ["OP"]);
    assert_eq!(op.r#type, Type::Tv);
    assert_eq!(op.status, Status::Airing);
    assert_eq!(op.episode_count, -1);
    assert_eq!(op.date_started, FuzzyDate::new(1999, 10, 20));
    assert!(op.date_finished.is_empty());
    assert_eq!(op.genres, ["Action", "Adventure"]);
    assert_eq!(op.studios, ["Toei Animation"]);
    assert!((op.score - 8.72).abs() < 1e-5);
    assert_eq!(op.next_episode_time, 1_731_700_000);
    assert_eq!(op.last_modified, 1_731_600_000);

    let eva = anime::get(&conn, 30).unwrap().unwrap();
    assert_eq!(eva.titles.english, "");
    assert!(eva.titles.synonyms.is_empty());
    assert_eq!(eva.score, 0.0);

    let broken = anime::get(&conn, 99).unwrap().unwrap();
    assert!(
        broken.titles.synonyms.is_empty(),
        "invalid JSON reads as empty"
    );

    // List: re-keyed by anime ID, the surviving unsynced entry kept.
    let entries = list::all(&conn).unwrap();
    assert_eq!(
        entries.iter().map(|e| e.anime_id).collect::<Vec<_>>(),
        [21, 30]
    );
    let op_entry = list::get(&conn, 21).unwrap().unwrap();
    assert_eq!(op_entry.id, 5001);
    assert_eq!(op_entry.watched_episodes, 1100);
    assert_eq!(op_entry.status, ListStatus::Watching);
    assert_eq!(op_entry.date_started, FuzzyDate::new(2020, 1, 0));
    assert_eq!(op_entry.last_updated, 1_731_700_000);
    let eva_entry = list::get(&conn, 30).unwrap().unwrap();
    assert_eq!(eva_entry.id, 0);
    assert_eq!(eva_entry.notes, "");
    assert!(eva_entry.is_private);
    assert!(eva_entry.pending_delete);
    assert_eq!(eva_entry.status, ListStatus::Completed);

    // After the migration, unsynced entries no longer overwrite each other.
    let mut new_entry = eva_entry.clone();
    new_entry.anime_id = 99;
    list::upsert(&conn, &new_entry).unwrap();
    assert_eq!(list::all(&conn).unwrap().len(), 3);

    let settings = anime_settings::get(&conn, 21).unwrap().unwrap();
    assert_eq!(settings.folder, r"D:\Anime\One Piece");
    assert_eq!(settings.synonyms, ["Wan Pisu"]);

    let items = history::all(&conn).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[1].episode, 1100);
    assert_eq!(items[2].anime_id, 12345);

    let pending = queue::all(&conn).unwrap();
    assert_eq!(
        pending.iter().map(|q| q.anime_id).collect::<Vec<_>>(),
        [30, 21]
    );
    assert_eq!(pending[0].dirty, Fields::DATE_COMPLETED);
    assert_eq!(pending[0].last_error, None);
    assert_eq!(pending[1].dirty, Fields::EPISODE | Fields::STATUS);
    assert_eq!(pending[1].retry_count, 1);
    assert_eq!(
        pending[1].last_error.as_deref(),
        Some("Connection timed out")
    );
}

#[test]
fn reopening_a_migrated_database_is_a_no_op() {
    let db = TempDb::new("taiga-v2-reopen");
    write_taiga_db(&db.0);
    drop(tracker_db::open(&db.0).unwrap());
    let conn = tracker_db::open(&db.0).unwrap();
    assert_eq!(list::all(&conn).unwrap().len(), 2);
    assert_eq!(anime::count(&conn).unwrap(), 3);
}
