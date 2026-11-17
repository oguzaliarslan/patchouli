//! The user's list (`anime_list` table), one entry per anime.

use rusqlite::{Connection, OptionalExtension, Row, named_params};
use tracker_core::{FuzzyDate, ListEntry, ListStatus};

use crate::Result;
use crate::value::{boolean, int, int32, text};

const INSERT: &str = "INSERT OR REPLACE INTO anime_list(
    media_id, id, progress, date_start, date_end, score, status, private,
    rewatched_times, rewatching, rewatching_ep, notes, last_updated, pending_delete
  ) VALUES(
    :media_id, :id, :progress, :date_start, :date_end, :score, :status, :private,
    :rewatched_times, :rewatching, :rewatching_ep, :notes, :last_updated, :pending_delete
  )";

pub fn get(conn: &Connection, anime_id: i32) -> Result<Option<ListEntry>> {
    Ok(conn
        .query_row(
            "SELECT * FROM anime_list WHERE media_id = ?1",
            [anime_id],
            from_row,
        )
        .optional()?)
}

pub fn all(conn: &Connection) -> Result<Vec<ListEntry>> {
    let mut stmt = conn.prepare("SELECT * FROM anime_list ORDER BY media_id")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Inserts or replaces the entry for `entry.anime_id`.
pub fn upsert(conn: &Connection, entry: &ListEntry) -> Result<()> {
    upsert_many(conn, std::slice::from_ref(entry))
}

/// Inserts or replaces several entries in one transaction.
pub fn upsert_many(conn: &Connection, entries: &[ListEntry]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(INSERT)?;
        for e in entries {
            stmt.execute(named_params! {
                ":media_id": e.anime_id,
                ":id": e.id,
                ":progress": e.watched_episodes,
                ":date_start": e.date_started.to_string(),
                ":date_end": e.date_completed.to_string(),
                ":score": e.score,
                ":status": e.status as i32,
                ":private": e.is_private,
                ":rewatched_times": e.rewatched_times,
                ":rewatching": e.rewatching,
                ":rewatching_ep": e.rewatching_ep,
                ":notes": e.notes,
                ":last_updated": e.last_updated,
                ":pending_delete": e.pending_delete,
            })?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Returns whether an entry was deleted.
pub fn delete(conn: &Connection, anime_id: i32) -> Result<bool> {
    Ok(conn.execute("DELETE FROM anime_list WHERE media_id = ?1", [anime_id])? > 0)
}

fn from_row(row: &Row) -> rusqlite::Result<ListEntry> {
    Ok(ListEntry {
        id: int(row, "id")?,
        anime_id: int32(row, "media_id")?,
        watched_episodes: int32(row, "progress")?,
        score: int32(row, "score")?,
        status: ListStatus::from_i32(int32(row, "status")?).unwrap_or_default(),
        is_private: boolean(row, "private")?,
        rewatched_times: int32(row, "rewatched_times")?,
        rewatching: boolean(row, "rewatching")?,
        rewatching_ep: int32(row, "rewatching_ep")?,
        date_started: FuzzyDate::parse(&text(row, "date_start")?),
        date_completed: FuzzyDate::parse(&text(row, "date_end")?),
        last_updated: int(row, "last_updated")?,
        notes: text(row, "notes")?,
        pending_delete: boolean(row, "pending_delete")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    fn entry(anime_id: i32) -> ListEntry {
        ListEntry {
            id: 0,
            anime_id,
            watched_episodes: 5,
            score: 85,
            status: ListStatus::Watching,
            is_private: true,
            rewatched_times: 1,
            rewatching: false,
            rewatching_ep: 0,
            date_started: FuzzyDate::new(2024, 1, 2),
            date_completed: FuzzyDate::default(),
            last_updated: 1_700_000_000,
            notes: "notes".into(),
            pending_delete: false,
        }
    }

    #[test]
    fn round_trip() {
        let conn = open_in_memory().unwrap();
        upsert(&conn, &entry(1)).unwrap();
        assert_eq!(get(&conn, 1).unwrap(), Some(entry(1)));
    }

    /// Unsynced entries all have service ID 0. They must not overwrite each other.
    #[test]
    fn unsynced_entries_are_kept_apart() {
        let conn = open_in_memory().unwrap();
        upsert_many(&conn, &[entry(1), entry(2), entry(3)]).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 3);

        let mut updated = entry(2);
        updated.watched_episodes = 6;
        upsert(&conn, &updated).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 3);
        assert_eq!(get(&conn, 2).unwrap().unwrap().watched_episodes, 6);

        assert!(delete(&conn, 2).unwrap());
        assert_eq!(get(&conn, 2).unwrap(), None);
    }
}
