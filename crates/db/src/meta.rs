//! Key-value metadata (`meta` table). Taiga stores the app `version` here.

use rusqlite::{Connection, OptionalExtension};

use crate::Result;

pub fn get(conn: &Connection, name: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM meta WHERE name = ?1 ORDER BY id DESC LIMIT 1",
            [name],
            |row| row.get(0),
        )
        .optional()?)
}

/// Sets `name` to `value`. Taiga's table has no unique constraint on `name`,
/// so this updates existing rows and inserts only if there are none.
pub fn set(conn: &Connection, name: &str, value: &str) -> Result<()> {
    let updated = conn.execute("UPDATE meta SET value = ?2 WHERE name = ?1", [name, value])?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO meta(name, value) VALUES(?1, ?2)",
            [name, value],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn get_and_set() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get(&conn, "version").unwrap(), None);
        set(&conn, "version", "1").unwrap();
        set(&conn, "version", "2").unwrap();
        assert_eq!(get(&conn, "version").unwrap().as_deref(), Some("2"));
    }
}
