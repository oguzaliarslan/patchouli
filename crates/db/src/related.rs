//! Sequels and recommendations of anime (`related` and `related_checked`).

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::{Result, in_transaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Sequel,
    Recommendation,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sequel => "sequel",
            Self::Recommendation => "recommendation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub anime_id: i32,
    pub related_id: i32,
    pub kind: Kind,
    /// Order among the anime's recommendations, best first.
    pub rank: i32,
}

/// Replaces what's stored for `anime_id` and records when it was checked.
pub fn replace(
    conn: &Connection,
    anime_id: i32,
    sequels: &[i32],
    recommendations: &[i32],
    time: i64,
) -> Result<()> {
    in_transaction(conn, |conn| {
        conn.execute("DELETE FROM related WHERE anime_id = ?1", [anime_id])?;
        let mut insert = conn.prepare_cached(
            "INSERT OR IGNORE INTO related(anime_id, related_id, kind, rank) VALUES(?1, ?2, ?3, ?4)",
        )?;
        for (kind, ids) in [
            (Kind::Sequel, sequels),
            (Kind::Recommendation, recommendations),
        ] {
            for (rank, id) in ids.iter().enumerate() {
                insert.execute(params![anime_id, id, kind.as_str(), rank as i32])?;
            }
        }
        conn.execute(
            "INSERT OR REPLACE INTO related_checked(anime_id, time) VALUES(?1, ?2)",
            params![anime_id, time],
        )?;
        Ok(())
    })
}

pub fn all(conn: &Connection) -> Result<Vec<Row>> {
    let mut stmt = conn
        .prepare("SELECT anime_id, related_id, kind, rank FROM related ORDER BY anime_id, rank")?;
    let rows = stmt.query_map([], |row| {
        Ok(Row {
            anime_id: row.get(0)?,
            related_id: row.get(1)?,
            kind: if row.get::<_, String>(2)? == "sequel" {
                Kind::Sequel
            } else {
                Kind::Recommendation
            },
            rank: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// When each anime was last checked.
pub fn checked(conn: &Connection) -> Result<HashMap<i32, i64>> {
    let mut stmt = conn.prepare("SELECT anime_id, time FROM related_checked")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn replaces_per_anime() {
        let conn = open_in_memory().unwrap();
        replace(&conn, 1, &[2], &[5, 4], 100).unwrap();
        replace(&conn, 1, &[3], &[], 200).unwrap();
        replace(&conn, 9, &[], &[4], 150).unwrap();
        assert_eq!(
            all(&conn).unwrap(),
            [
                Row {
                    anime_id: 1,
                    related_id: 3,
                    kind: Kind::Sequel,
                    rank: 0
                },
                Row {
                    anime_id: 9,
                    related_id: 4,
                    kind: Kind::Recommendation,
                    rank: 0
                },
            ]
        );
        assert_eq!(checked(&conn).unwrap(), HashMap::from([(1, 200), (9, 150)]));
    }
}
