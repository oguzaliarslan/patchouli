//! Anime metadata (`anime` table).

use patchouli_core::anime::{AgeRating, Status, Titles, Type};
use patchouli_core::{Anime, FuzzyDate};
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::Result;
use crate::value::{int, int32, real, string_list, text, to_json};

const INSERT: &str = include_str!("../sql/insertAnime.sql");

pub fn get(conn: &Connection, id: i32) -> Result<Option<Anime>> {
    Ok(conn
        .query_row("SELECT * FROM anime WHERE id = ?1", [id], from_row)
        .optional()?)
}

pub fn all(conn: &Connection) -> Result<Vec<Anime>> {
    let mut stmt = conn.prepare("SELECT * FROM anime ORDER BY id")?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM anime", [], |row| row.get(0))?)
}

/// Inserts or replaces an anime.
pub fn upsert(conn: &Connection, anime: &Anime) -> Result<()> {
    upsert_many(conn, std::slice::from_ref(anime))
}

/// Inserts or replaces several anime in one transaction.
pub fn upsert_many(conn: &Connection, items: &[Anime]) -> Result<()> {
    crate::in_transaction(conn, |conn| {
        let mut stmt = conn.prepare_cached(INSERT)?;
        for a in items {
            stmt.execute(named_params! {
                ":id": a.id,
                ":title": a.titles.romaji,
                ":english": a.titles.english,
                ":japanese": a.titles.japanese,
                ":synonym": to_json(&a.titles.synonyms),
                ":type": a.r#type as i32,
                ":status": a.status as i32,
                ":episode_count": a.episode_count,
                ":episode_length": a.episode_length,
                ":date_start": a.date_started.to_string(),
                ":date_end": a.date_finished.to_string(),
                ":image": a.image_url,
                ":trailer_id": a.trailer_id,
                ":age_rating": a.age_rating as i32,
                ":genres": to_json(&a.genres),
                ":tags": to_json(&a.tags),
                ":producers": to_json(&a.producers),
                ":studios": to_json(&a.studios),
                ":score": f64::from(a.score),
                ":popularity": a.popularity_rank,
                ":synopsis": a.synopsis,
                ":last_aired_episode": a.last_aired_episode,
                ":next_episode_time": a.next_episode_time,
                ":modified": a.last_modified,
            })?;
        }
        Ok(())
    })
}

/// Returns whether a row was deleted.
pub fn delete(conn: &Connection, id: i32) -> Result<bool> {
    Ok(conn.execute("DELETE FROM anime WHERE id = ?1", [id])? > 0)
}

fn from_row(row: &Row) -> rusqlite::Result<Anime> {
    Ok(Anime {
        id: int32(row, "id")?,
        last_modified: int(row, "modified")?,
        episode_count: int32(row, "episode_count")?,
        episode_length: int32(row, "episode_length")?,
        age_rating: AgeRating::from_i32(int32(row, "age_rating")?).unwrap_or_default(),
        status: Status::from_i32(int32(row, "status")?).unwrap_or_default(),
        r#type: Type::from_i32(int32(row, "type")?).unwrap_or_default(),
        date_started: FuzzyDate::parse(&text(row, "date_start")?),
        date_finished: FuzzyDate::parse(&text(row, "date_end")?),
        score: real(row, "score")? as f32,
        popularity_rank: int32(row, "popularity")?,
        image_url: text(row, "image")?,
        slug: String::new(),
        synopsis: text(row, "synopsis")?,
        trailer_id: text(row, "trailer_id")?,
        titles: Titles {
            romaji: text(row, "title")?,
            english: text(row, "english")?,
            japanese: text(row, "japanese")?,
            synonyms: string_list(row, "synonym")?,
        },
        genres: string_list(row, "genres")?,
        producers: string_list(row, "producers")?,
        studios: string_list(row, "studios")?,
        tags: string_list(row, "tags")?,
        last_aired_episode: int32(row, "last_aired_episode")?,
        next_episode_time: int(row, "next_episode_time")?,
    })
}

#[cfg(test)]
pub(crate) fn sample(id: i32) -> Anime {
    Anime {
        id,
        last_modified: 1_700_000_000,
        episode_count: 12,
        episode_length: 24,
        age_rating: AgeRating::Pg13,
        status: Status::FinishedAiring,
        r#type: Type::Tv,
        date_started: FuzzyDate::new(2024, 4, 7),
        date_finished: FuzzyDate::new(2024, 6, 0),
        score: 8.25,
        popularity_rank: 42,
        image_url: "https://example.com/a.jpg".into(),
        slug: String::new(),
        synopsis: "A story.".into(),
        trailer_id: "abc".into(),
        titles: Titles {
            romaji: format!("Title {id}"),
            english: "English".into(),
            japanese: "日本語".into(),
            synonyms: vec!["Syn \"quoted\"".into(), "Other".into()],
        },
        genres: vec!["Action".into()],
        producers: vec![],
        studios: vec!["Studio".into()],
        tags: vec!["Tag".into(), "Tag 2".into()],
        last_aired_episode: 12,
        next_episode_time: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_in_memory;

    #[test]
    fn round_trip() {
        let conn = open_in_memory().unwrap();
        let anime = sample(1);
        upsert(&conn, &anime).unwrap();
        assert_eq!(get(&conn, 1).unwrap(), Some(anime));
        assert_eq!(get(&conn, 2).unwrap(), None);
    }

    #[test]
    fn upsert_replaces_and_delete_removes() {
        let conn = open_in_memory().unwrap();
        upsert_many(&conn, &[sample(1), sample(2)]).unwrap();
        let mut changed = sample(1);
        changed.episode_count = 24;
        upsert(&conn, &changed).unwrap();

        let items = all(&conn).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].episode_count, 24);

        assert!(delete(&conn, 2).unwrap());
        assert!(!delete(&conn, 2).unwrap());
        assert_eq!(count(&conn).unwrap(), 1);
    }
}
