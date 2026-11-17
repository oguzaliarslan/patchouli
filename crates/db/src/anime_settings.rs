//! Per-anime user overrides (`anime_settings` table).

use rusqlite::{Connection, OptionalExtension, Row, named_params};
use tracker_core::AnimeSettings;

use crate::Result;
use crate::value::{int32, string_list, text, to_json};

const INSERT: &str = include_str!("../sql/insertAnimeSettings.sql");

pub fn get(conn: &Connection, id: i32) -> Result<Option<AnimeSettings>> {
    Ok(conn
        .query_row("SELECT * FROM anime_settings WHERE id = ?1", [id], from_row)
        .optional()?)
}

pub fn all(conn: &Connection) -> Result<Vec<AnimeSettings>> {
    let mut stmt = conn.prepare("SELECT * FROM anime_settings ORDER BY id")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn upsert(conn: &Connection, settings: &AnimeSettings) -> Result<()> {
    conn.execute(
        INSERT,
        named_params! {
            ":id": settings.id,
            ":display_title": settings.display_title,
            ":folder": settings.folder,
            ":synonyms": to_json(&settings.synonyms),
        },
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i32) -> Result<bool> {
    Ok(conn.execute("DELETE FROM anime_settings WHERE id = ?1", [id])? > 0)
}

fn from_row(row: &Row) -> rusqlite::Result<AnimeSettings> {
    Ok(AnimeSettings {
        id: int32(row, "id")?,
        display_title: text(row, "display_title")?,
        folder: text(row, "folder")?,
        synonyms: string_list(row, "synonyms")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn round_trip() {
        let conn = open_in_memory().unwrap();
        let settings = AnimeSettings {
            id: 7,
            display_title: "Custom".into(),
            folder: r"D:\Anime\Show".into(),
            synonyms: vec!["Alt".into()],
        };
        upsert(&conn, &settings).unwrap();
        assert_eq!(get(&conn, 7).unwrap(), Some(settings.clone()));
        assert_eq!(all(&conn).unwrap(), vec![settings]);
        assert!(delete(&conn, 7).unwrap());
    }
}
