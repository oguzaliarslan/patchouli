//! Schema migrations, tracked with `PRAGMA user_version`.
//!
//! Taiga never sets `user_version`, so a Taiga database starts at 0 like a new
//! one. Migration 1 creates Taiga's tables only if they're missing, which makes
//! it safe for both.

use rusqlite::{Connection, Transaction};

use crate::{Error, Result};

type Migration = fn(&Transaction) -> rusqlite::Result<()>;

const MIGRATIONS: &[Migration] = &[taiga_schema, key_anime_list_by_media_id];

/// Schema version after all migrations have run.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Brings the database up to [`SCHEMA_VERSION`], one transaction per migration.
pub fn run(conn: &mut Connection) -> Result<()> {
    let current = version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(Error::UnsupportedVersion {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        migration(&tx)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

/// 1: Taiga v2's tables, as shipped in `src/resources/sql`.
fn taiga_schema(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(include_str!("../sql/createMeta.sql"))?;
    tx.execute_batch(include_str!("../sql/createAnime.sql"))?;
    tx.execute_batch(include_str!("../sql/createAnimeList.sql"))?;
    tx.execute_batch(include_str!("../sql/createAnimeSettings.sql"))?;
    tx.execute_batch(include_str!("../sql/createHistory.sql"))?;
    tx.execute_batch(include_str!("../sql/createQueue.sql"))
}

/// 2: Rebuilds `anime_list` with `media_id` as the primary key.
///
/// In Taiga's schema the primary key is `id`, the service's entry ID, which is
/// 0 for entries that haven't been synced yet, so two new entries would
/// overwrite each other. Its foreign key also points at a `media` table that
/// doesn't exist. If an anime has several rows, the last one is kept.
fn key_anime_list_by_media_id(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE TABLE anime_list_new(
           media_id INTEGER PRIMARY KEY REFERENCES anime(id),
           id INTEGER NOT NULL DEFAULT 0,
           progress INTEGER NOT NULL DEFAULT 0,
           date_start TEXT NOT NULL DEFAULT '0000-00-00',
           date_end TEXT NOT NULL DEFAULT '0000-00-00',
           score INTEGER NOT NULL DEFAULT 0,
           status INTEGER NOT NULL DEFAULT 0,
           private INTEGER NOT NULL DEFAULT 0,
           rewatched_times INTEGER NOT NULL DEFAULT 0,
           rewatching INTEGER NOT NULL DEFAULT 0,
           rewatching_ep INTEGER NOT NULL DEFAULT 0,
           notes TEXT NOT NULL DEFAULT '',
           last_updated INTEGER NOT NULL DEFAULT 0,
           pending_delete INTEGER NOT NULL DEFAULT 0
         );
         INSERT OR REPLACE INTO anime_list_new
           SELECT
             media_id,
             COALESCE(id, 0),
             COALESCE(progress, 0),
             COALESCE(date_start, '0000-00-00'),
             COALESCE(date_end, '0000-00-00'),
             COALESCE(score, 0),
             COALESCE(status, 0),
             COALESCE(private, 0),
             COALESCE(rewatched_times, 0),
             COALESCE(rewatching, 0),
             COALESCE(rewatching_ep, 0),
             COALESCE(notes, ''),
             CAST(COALESCE(last_updated, 0) AS INTEGER),
             pending_delete
           FROM anime_list
           ORDER BY rowid;
         DROP TABLE anime_list;
         ALTER TABLE anime_list_new RENAME TO anime_list;",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_once_and_sets_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), SCHEMA_VERSION);
        run(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn rejects_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            run(&mut conn),
            Err(Error::UnsupportedVersion { .. })
        ));
    }
}
