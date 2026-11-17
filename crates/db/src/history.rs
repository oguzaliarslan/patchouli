//! Watched episodes (`history` table), oldest first.

use rusqlite::{Connection, Row, named_params};
use tracker_core::HistoryItem;

use crate::Result;
use crate::value::{int, int32};

const INSERT: &str = include_str!("../sql/insertHistory.sql");

/// Appends an item and returns it with its new ID.
pub fn add(conn: &Connection, anime_id: i32, episode: i32, time: i64) -> Result<HistoryItem> {
    conn.execute(
        INSERT,
        named_params! { ":anime_id": anime_id, ":episode": episode, ":time": time },
    )?;
    Ok(HistoryItem {
        id: conn.last_insert_rowid(),
        anime_id,
        episode,
        time,
    })
}

pub fn all(conn: &Connection) -> Result<Vec<HistoryItem>> {
    let mut stmt = conn.prepare("SELECT * FROM history ORDER BY id ASC")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn remove(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM history WHERE id = ?1", [id])? > 0)
}

pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}

fn from_row(row: &Row) -> rusqlite::Result<HistoryItem> {
    Ok(HistoryItem {
        id: int(row, "id")?,
        anime_id: int32(row, "anime_id")?,
        episode: int32(row, "episode")?,
        time: int(row, "time")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn add_list_remove_clear() {
        let conn = open_in_memory().unwrap();
        let first = add(&conn, 1, 1, 100).unwrap();
        let second = add(&conn, 1, 2, 200).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(all(&conn).unwrap(), vec![first.clone(), second.clone()]);

        assert!(remove(&conn, first.id).unwrap());
        assert_eq!(all(&conn).unwrap(), vec![second]);

        clear(&conn).unwrap();
        assert!(all(&conn).unwrap().is_empty());
    }
}
