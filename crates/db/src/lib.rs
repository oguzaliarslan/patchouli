//! SQLite storage. The schema in `sql/` is copied verbatim from Taiga v2
//! (`src/resources/sql`) so that existing databases can be opened directly.
//!
//! Note: `createAnimeList.sql` declares a foreign key on `media(id)`, a table that
//! does not exist. It is harmless while `PRAGMA foreign_keys` is off. Fix it in a migration.

use std::path::Path;

pub use rusqlite::{Connection, Error, Result};

/// `(table, CREATE TABLE IF NOT EXISTS ...)` in creation order.
pub const SCHEMA: &[(&str, &str)] = &[
    ("meta", include_str!("../sql/createMeta.sql")),
    ("anime", include_str!("../sql/createAnime.sql")),
    ("anime_list", include_str!("../sql/createAnimeList.sql")),
    (
        "anime_settings",
        include_str!("../sql/createAnimeSettings.sql"),
    ),
    ("history", include_str!("../sql/createHistory.sql")),
    ("queue", include_str!("../sql/createQueue.sql")),
];

pub mod queries {
    pub const INSERT_ANIME: &str = include_str!("../sql/insertAnime.sql");
    pub const INSERT_ANIME_LIST: &str = include_str!("../sql/insertAnimeList.sql");
    pub const INSERT_ANIME_SETTINGS: &str = include_str!("../sql/insertAnimeSettings.sql");
    pub const INSERT_HISTORY: &str = include_str!("../sql/insertHistory.sql");
    pub const INSERT_QUEUE: &str = include_str!("../sql/insertQueue.sql");
}

/// Opens (or creates) the database at `path` and ensures the schema exists.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    init(&conn)?;
    Ok(conn)
}

/// Opens an in-memory database with the schema, for tests.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init(&conn)?;
    Ok(conn)
}

fn init(conn: &Connection) -> Result<()> {
    for (_, sql) in SCHEMA {
        conn.execute_batch(sql)?;
    }
    Ok(())
}

/// Names of the user tables in the database, sorted.
pub fn table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_all_tables() {
        let conn = open_in_memory().unwrap();
        let mut expected: Vec<String> = SCHEMA.iter().map(|(t, _)| t.to_string()).collect();
        expected.sort();
        assert_eq!(table_names(&conn).unwrap(), expected);
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = open_in_memory().unwrap();
        init(&conn).unwrap();
    }
}
