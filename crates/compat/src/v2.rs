//! Taiga v2's data folder: `media.sqlite`, `settings.json` and
//! `accounts.json` (only the username is read from it).
//!
//! The database is opened read-only and isn't migrated, so Taiga can keep
//! using it. Its tables are read with the same queries as ours: the columns
//! are the same, only `anime_list`'s key differs.

use std::path::Path;

use patchouli_core::ServiceId;
use patchouli_db::{anime, anime_settings, history, list, queue, table_names};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Value};

use crate::{ImportData, read_text};

fn read_json(path: &Path) -> Option<Map<String, Value>> {
    let text = read_text(path).ok()?;
    match serde_json::from_str(&text).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

pub fn read(dir: &Path) -> Result<ImportData, String> {
    let mut data = ImportData::default();

    if let Some(settings) = read_json(&dir.join("settings.json")) {
        data.settings = settings;
    } else {
        data.warnings
            .push("settings.json couldn't be read; settings weren't imported.".into());
    }
    // Taiga v2 defaults to AniList.
    let slug = data
        .settings
        .get("sync.service")
        .and_then(Value::as_str)
        .unwrap_or("anilist")
        .to_owned();
    let service =
        ServiceId::from_slug(&slug).ok_or_else(|| format!("Unknown service \"{slug}\"."))?;
    data.service = Some(service);
    data.username = read_json(&dir.join("accounts.json"))
        .and_then(|a| {
            a.get(&format!("{slug}.username"))?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|u| !u.is_empty());
    // Our own key, not Taiga's.
    data.settings.remove("meta.version");

    let path = dir.join(patchouli_db::FILE_NAME);
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    let tables = table_names(&conn).map_err(|e| e.to_string())?;
    let has = |name: &str| tables.iter().any(|t| t == name);
    let err = |e: patchouli_db::Error| format!("{}: {e}", path.display());
    if has("anime") {
        data.anime = anime::all(&conn).map_err(err)?;
    }
    if has("anime_list") {
        data.entries = list::all(&conn).map_err(err)?;
    }
    if has("anime_settings") {
        data.anime_settings = anime_settings::all(&conn).map_err(err)?;
    }
    if has("history") {
        data.history = history::all(&conn).map_err(err)?;
    }
    if has("queue") {
        data.queue = queue::all(&conn).map_err(err)?;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_v2_data_folder_without_changing_it() {
        let dir = std::env::temp_dir().join(format!("tracker-v2-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            "\u{feff}{\"sync.service\": \"myanimelist\", \"track.update.delay\": 60, \"meta.version\": \"2.0.0\"}",
        )
        .unwrap();
        std::fs::write(
            dir.join("accounts.json"),
            r#"{"myanimelist.username": "someone", "myanimelist.accessToken": "secret"}"#,
        )
        .unwrap();

        // Taiga's schema, as Taiga writes it (foreign keys off).
        let path = dir.join(patchouli_db::FILE_NAME);
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "foreign_keys", false).unwrap();
            for sql in [
                include_str!("../../db/sql/createMeta.sql"),
                include_str!("../../db/sql/createAnime.sql"),
                include_str!("../../db/sql/createAnimeList.sql"),
            ] {
                conn.execute_batch(sql).unwrap();
            }
            conn.execute_batch(
                "INSERT INTO anime(id, title, episode_count) VALUES(21, 'One Piece', -1);
                 INSERT INTO anime_list(id, media_id, progress, status, pending_delete) VALUES(0, 21, 5, 1, 0);",
            )
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();

        let data = read(&dir).unwrap();
        assert_eq!(data.service, Some(ServiceId::MyAnimeList));
        assert_eq!(data.username.as_deref(), Some("someone"));
        assert_eq!(data.settings["track.update.delay"], 60);
        assert!(!data.settings.contains_key("meta.version"));
        assert_eq!(data.anime.len(), 1);
        assert_eq!(data.entries[0].watched_episodes, 5);
        // Tables Taiga hadn't created yet are skipped.
        assert!(data.history.is_empty());

        // Read-only: the file is untouched (no migration).
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
