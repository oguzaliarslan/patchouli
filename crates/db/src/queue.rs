//! Pending list updates (`queue` table), oldest first.
//!
//! [`mirror`] has the same functions for `mirror_queue`: changes waiting to be
//! copied to the second service.

use patchouli_core::{Fields, QueueItem};
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::Result;
use crate::value::{int, int32, optional_text};

const INSERT: &str = include_str!("../sql/insertQueue.sql");
const TABLE: &str = "queue";

pub fn all(conn: &Connection) -> Result<Vec<QueueItem>> {
    all_in(conn, TABLE)
}

pub fn get(conn: &Connection, anime_id: i32) -> Result<Option<QueueItem>> {
    get_in(conn, TABLE, anime_id)
}

/// Inserts or replaces the item for `item.anime_id`.
pub fn upsert(conn: &Connection, item: &QueueItem) -> Result<()> {
    upsert_in(conn, INSERT, item)
}

pub fn remove(conn: &Connection, anime_id: i32) -> Result<bool> {
    remove_in(conn, TABLE, anime_id)
}

pub fn clear(conn: &Connection) -> Result<()> {
    clear_in(conn, TABLE)
}

/// Changes waiting to be copied to the second service (`mirror_queue`).
pub mod mirror {
    use patchouli_core::QueueItem;
    use rusqlite::Connection;

    use crate::Result;

    const TABLE: &str = "mirror_queue";
    const INSERT: &str = "INSERT OR REPLACE INTO mirror_queue(anime_id, dirty, time, retry_count, last_error) \
         VALUES(:anime_id, :dirty, :time, :retry_count, :last_error)";

    pub fn all(conn: &Connection) -> Result<Vec<QueueItem>> {
        super::all_in(conn, TABLE)
    }

    pub fn get(conn: &Connection, anime_id: i32) -> Result<Option<QueueItem>> {
        super::get_in(conn, TABLE, anime_id)
    }

    pub fn upsert(conn: &Connection, item: &QueueItem) -> Result<()> {
        super::upsert_in(conn, INSERT, item)
    }

    pub fn remove(conn: &Connection, anime_id: i32) -> Result<bool> {
        super::remove_in(conn, TABLE, anime_id)
    }

    pub fn clear(conn: &Connection) -> Result<()> {
        super::clear_in(conn, TABLE)
    }
}

fn all_in(conn: &Connection, table: &str) -> Result<Vec<QueueItem>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY time ASC"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn get_in(conn: &Connection, table: &str, anime_id: i32) -> Result<Option<QueueItem>> {
    Ok(conn
        .query_row(
            &format!("SELECT * FROM {table} WHERE anime_id = ?1"),
            [anime_id],
            from_row,
        )
        .optional()?)
}

fn upsert_in(conn: &Connection, insert: &str, item: &QueueItem) -> Result<()> {
    conn.execute(
        insert,
        named_params! {
            ":anime_id": item.anime_id,
            ":dirty": item.dirty.bits(),
            ":time": item.time,
            ":retry_count": item.retry_count,
            ":last_error": item.last_error.as_deref().filter(|e| !e.is_empty()),
        },
    )?;
    Ok(())
}

fn remove_in(conn: &Connection, table: &str, anime_id: i32) -> Result<bool> {
    Ok(conn.execute(
        &format!("DELETE FROM {table} WHERE anime_id = ?1"),
        [anime_id],
    )? > 0)
}

fn clear_in(conn: &Connection, table: &str) -> Result<()> {
    conn.execute(&format!("DELETE FROM {table}"), [])?;
    Ok(())
}

fn from_row(row: &Row) -> rusqlite::Result<QueueItem> {
    Ok(QueueItem {
        anime_id: int32(row, "anime_id")?,
        dirty: Fields::from_bits_truncate(int(row, "dirty")? as u32),
        time: int(row, "time")?,
        retry_count: int32(row, "retry_count")?,
        last_error: optional_text(row, "last_error")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn upsert_replaces_per_anime_and_orders_by_time() {
        let conn = open_in_memory().unwrap();
        let late = QueueItem {
            anime_id: 1,
            dirty: Fields::EPISODE,
            time: 200,
            retry_count: 0,
            last_error: None,
        };
        let early = QueueItem {
            anime_id: 2,
            dirty: Fields::SCORE | Fields::NOTES,
            time: 100,
            retry_count: 2,
            last_error: Some("timeout".into()),
        };
        upsert(&conn, &late).unwrap();
        upsert(&conn, &early).unwrap();
        assert_eq!(all(&conn).unwrap(), vec![early.clone(), late.clone()]);

        let merged = QueueItem {
            dirty: late.dirty | Fields::STATUS,
            ..late
        };
        upsert(&conn, &merged).unwrap();
        assert_eq!(all(&conn).unwrap(), vec![early, merged]);

        assert!(remove(&conn, 2).unwrap());
        clear(&conn).unwrap();
        assert!(all(&conn).unwrap().is_empty());
    }

    #[test]
    fn mirror_queue_is_separate() {
        let conn = open_in_memory().unwrap();
        let item = QueueItem {
            anime_id: 1,
            time: 100,
            ..QueueItem::default()
        };
        mirror::upsert(&conn, &item).unwrap();
        assert!(all(&conn).unwrap().is_empty());
        assert_eq!(mirror::get(&conn, 1).unwrap(), Some(item));
        assert!(mirror::remove(&conn, 1).unwrap());
        assert!(mirror::all(&conn).unwrap().is_empty());
    }
}
