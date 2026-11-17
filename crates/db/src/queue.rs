//! Pending list updates (`queue` table), oldest first.

use patchouli_core::{Fields, QueueItem};
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::Result;
use crate::value::{int, int32, optional_text};

const INSERT: &str = include_str!("../sql/insertQueue.sql");

pub fn all(conn: &Connection) -> Result<Vec<QueueItem>> {
    let mut stmt = conn.prepare("SELECT * FROM queue ORDER BY time ASC")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn get(conn: &Connection, anime_id: i32) -> Result<Option<QueueItem>> {
    Ok(conn
        .query_row(
            "SELECT * FROM queue WHERE anime_id = ?1",
            [anime_id],
            from_row,
        )
        .optional()?)
}

/// Inserts or replaces the item for `item.anime_id`.
pub fn upsert(conn: &Connection, item: &QueueItem) -> Result<()> {
    conn.execute(
        INSERT,
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

pub fn remove(conn: &Connection, anime_id: i32) -> Result<bool> {
    Ok(conn.execute("DELETE FROM queue WHERE anime_id = ?1", [anime_id])? > 0)
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM queue", [])?;
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
}
