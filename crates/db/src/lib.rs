//! SQLite storage for anime metadata, the user's list, history and the sync queue.
//!
//! The baseline schema (`sql/create*.sql`) is Taiga v2's, so a Taiga `media.sqlite`
//! can be opened directly and is upgraded in place by [`migrations`].
//!
//! Each table has a module of plain functions that take a [`Connection`]:
//! [`anime`], [`list`], [`anime_settings`], [`history`], [`queue`] and [`meta`].
//! Functions that write several rows use a transaction.

use std::fmt;
use std::path::Path;

pub use rusqlite::Connection;

pub mod anime;
pub mod anime_settings;
pub mod history;
pub mod list;
pub mod meta;
pub mod migrations;
pub mod queue;
mod value;

/// File name Taiga uses for its database.
pub const FILE_NAME: &str = "media.sqlite";

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    /// The database was written by a newer version of the app.
    UnsupportedVersion {
        found: i64,
        supported: i64,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "database error: {e}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "database schema version {found} is newer than supported version {supported}"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            Self::UnsupportedVersion { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

/// Opens (or creates) the database at `path` and migrates it to the current schema.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&mut conn)?;
    Ok(conn)
}

/// Opens an in-memory database with the current schema, for tests.
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure(&mut conn)?;
    Ok(conn)
}

fn configure(conn: &mut Connection) -> Result<()> {
    // Foreign keys are documentation only, as in Taiga: history, queue and list
    // rows may refer to anime that are no longer in the `anime` table. The
    // bundled SQLite enables enforcement by default, so turn it off.
    conn.pragma_update(None, "foreign_keys", false)?;
    migrations::run(conn)
}

/// Names of the user tables in the database, sorted.
pub fn table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_all_tables() {
        let conn = open_in_memory().unwrap();
        assert_eq!(
            table_names(&conn).unwrap(),
            [
                "anime",
                "anime_list",
                "anime_settings",
                "history",
                "meta",
                "queue"
            ]
        );
    }

    #[test]
    fn reopening_a_file_keeps_data() {
        let dir = std::env::temp_dir().join(format!("tracker-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let _ = std::fs::remove_file(&path);

        let conn = open(&path).unwrap();
        meta::set(&conn, "version", "1.2.3").unwrap();
        drop(conn);

        let conn = open(&path).unwrap();
        assert_eq!(
            meta::get(&conn, "version").unwrap().as_deref(),
            Some("1.2.3")
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
