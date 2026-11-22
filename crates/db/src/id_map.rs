//! Anime IDs matched between services (`id_map` table).
//!
//! A `target_id` of 0 records that the anime has no match on the target
//! service, so it isn't looked up again.

use std::collections::HashMap;

use patchouli_core::ServiceId;
use rusqlite::{Connection, params};

use crate::{Result, in_transaction};

/// IDs on `target` for `ids` on `service`. IDs never looked up are left out.
pub fn get_many(
    conn: &Connection,
    service: ServiceId,
    target: ServiceId,
    ids: &[i32],
) -> Result<HashMap<i32, i32>> {
    let mut stmt = conn.prepare_cached(
        "SELECT target_id FROM id_map WHERE service = ?1 AND anime_id = ?2 AND target = ?3",
    )?;
    let mut result = HashMap::new();
    for &id in ids {
        let mut rows = stmt.query(params![service.slug(), id, target.slug()])?;
        if let Some(row) = rows.next()? {
            result.insert(id, row.get(0)?);
        }
    }
    Ok(result)
}

/// Records matches in both directions. `(id, 0)` records "no match".
pub fn set_many(
    conn: &Connection,
    service: ServiceId,
    target: ServiceId,
    pairs: &[(i32, i32)],
) -> Result<()> {
    in_transaction(conn, |conn| {
        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO id_map(service, anime_id, target, target_id) \
             VALUES(?1, ?2, ?3, ?4)",
        )?;
        for &(id, target_id) in pairs {
            stmt.execute(params![service.slug(), id, target.slug(), target_id])?;
            if target_id != 0 {
                stmt.execute(params![target.slug(), target_id, service.slug(), id])?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn stores_both_directions() {
        let conn = open_in_memory().unwrap();
        let (al, mal) = (ServiceId::AniList, ServiceId::MyAnimeList);
        set_many(&conn, al, mal, &[(154587, 52991), (5, 0)]).unwrap();
        assert_eq!(
            get_many(&conn, al, mal, &[154587, 5, 6]).unwrap(),
            HashMap::from([(154587, 52991), (5, 0)])
        );
        assert_eq!(
            get_many(&conn, mal, al, &[52991]).unwrap(),
            HashMap::from([(52991, 154587)])
        );
    }
}
