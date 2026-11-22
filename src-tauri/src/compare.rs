//! Compares the lists on AniList and MyAnimeList, and copies entries from
//! one to the other.
//!
//! Both lists are fetched fresh. Anime are matched through `id_map` (looked
//! up on AniList when new). Scores are compared at MyAnimeList's precision
//! (whole points out of 10), since that's all it stores.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use patchouli_core::{Anime, Fields, ListEntry, ListStatus, ServiceId};
use patchouli_db::{anime, list};
use patchouli_sync::{Account, Client, RemoteList};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::error::{CommandError, CommandResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Same,
    Different,
    OnlyMain,
    OnlyOther,
    /// The anime has no counterpart on the other service.
    NoMatch,
}

/// One service's side of a row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    anime_id: i32,
    title: String,
    entry: Option<ListEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// `main:other` anime IDs, 0 for unknown.
    key: String,
    kind: Kind,
    main: Option<Side>,
    other: Option<Side>,
    /// Fields that differ: `status`, `progress`, `score`, `rewatches`,
    /// `started`, `completed`.
    fields: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    main_service: ServiceId,
    main_name: &'static str,
    other_service: ServiceId,
    other_name: &'static str,
    /// Unix seconds.
    time: i64,
    rows: Vec<Row>,
}

/// Fields of `a` and `b` that differ, scores at whole points out of 10.
pub fn differences(a: &ListEntry, b: &ListEntry) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if a.status != b.status || a.rewatching != b.rewatching {
        fields.push("status");
    }
    if a.watched_episodes != b.watched_episodes {
        fields.push("progress");
    }
    if a.score / 10 != b.score / 10 {
        fields.push("score");
    }
    if a.rewatched_times != b.rewatched_times {
        fields.push("rewatches");
    }
    if a.date_started != b.date_started {
        fields.push("started");
    }
    if a.date_completed != b.date_completed {
        fields.push("completed");
    }
    fields
}

struct Lists<'a> {
    main: &'a RemoteList,
    other: &'a RemoteList,
    /// Main ID to other ID, 0 for no match.
    to_other: &'a HashMap<i32, i32>,
    /// Other ID to main ID, 0 for no match.
    to_main: &'a HashMap<i32, i32>,
}

fn titles(list: &RemoteList) -> HashMap<i32, String> {
    list.anime
        .iter()
        .map(|a: &Anime| {
            let t = &a.titles;
            let title = if t.romaji.is_empty() {
                t.english.clone()
            } else {
                t.romaji.clone()
            };
            (a.id, title)
        })
        .collect()
}

fn rows(lists: &Lists) -> Vec<Row> {
    let main_titles = titles(lists.main);
    let other_titles = titles(lists.other);
    let other_entries: HashMap<i32, &ListEntry> = lists
        .other
        .entries
        .iter()
        .map(|e| (e.anime_id, e))
        .collect();
    let side = |id: i32, titles: &HashMap<i32, String>, entry: Option<&ListEntry>| Side {
        anime_id: id,
        title: titles.get(&id).cloned().unwrap_or_else(|| format!("#{id}")),
        entry: entry.cloned(),
    };

    let mut rows = Vec::new();
    let mut matched = HashSet::new();
    for entry in &lists.main.entries {
        let target = lists.to_other.get(&entry.anime_id).copied().unwrap_or(0);
        let main = Some(side(entry.anime_id, &main_titles, Some(entry)));
        let (kind, other, fields) = if target == 0 {
            (Kind::NoMatch, None, Vec::new())
        } else {
            matched.insert(target);
            let title = other_titles
                .get(&target)
                .or_else(|| main_titles.get(&entry.anime_id))
                .cloned()
                .unwrap_or_default();
            let other_side = |e: Option<&ListEntry>| Side {
                anime_id: target,
                title: title.clone(),
                entry: e.cloned(),
            };
            match other_entries.get(&target) {
                Some(theirs) => {
                    let fields = differences(entry, theirs);
                    let kind = if fields.is_empty() {
                        Kind::Same
                    } else {
                        Kind::Different
                    };
                    (kind, Some(other_side(Some(theirs))), fields)
                }
                None => (Kind::OnlyMain, Some(other_side(None)), Vec::new()),
            }
        };
        rows.push(Row {
            key: format!("{}:{target}", entry.anime_id),
            kind,
            main,
            other,
            fields,
        });
    }
    for entry in &lists.other.entries {
        if matched.contains(&entry.anime_id) {
            continue;
        }
        let source = lists.to_main.get(&entry.anime_id).copied().unwrap_or(0);
        let other = Some(side(entry.anime_id, &other_titles, Some(entry)));
        let main = (source != 0).then(|| Side {
            anime_id: source,
            title: other_titles
                .get(&entry.anime_id)
                .cloned()
                .unwrap_or_default(),
            entry: None,
        });
        rows.push(Row {
            key: format!("{source}:{}", entry.anime_id),
            kind: if source == 0 {
                Kind::NoMatch
            } else {
                Kind::OnlyOther
            },
            main,
            other,
            fields: Vec::new(),
        });
    }
    let order = |k: Kind| match k {
        Kind::Different => 0,
        Kind::OnlyMain => 1,
        Kind::OnlyOther => 2,
        Kind::NoMatch => 3,
        Kind::Same => 4,
    };
    let title = |r: &Row| {
        r.main
            .as_ref()
            .or(r.other.as_ref())
            .map(|s| s.title.to_lowercase())
            .unwrap_or_default()
    };
    rows.sort_by_key(|r| (order(r.kind), title(r)));
    rows
}

#[derive(Default)]
pub struct Compare {
    last: tokio::sync::Mutex<Option<Comparison>>,
}

/// Clients for the main service and the other one, if both are logged in.
type Sessions = ((Arc<Client>, Account), (Client, Account));

fn sessions(state: &AppState) -> CommandResult<Sessions> {
    let main = state
        .sync
        .session()
        .ok_or_else(|| CommandError::new("Log in to your main list first."))?;
    let other_service = crate::sync::mirror_service(main.0.service());
    let other = crate::sync::saved_session(state, other_service).ok_or_else(|| {
        CommandError::with_code(
            "other-not-logged-in",
            format!(
                "Log in to {} under Settings › Accounts to compare.",
                other_service.name()
            ),
        )
    })?;
    Ok((main, other))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Fetches both lists and compares them.
#[tauri::command]
pub async fn compare_lists(state: State<'_, AppState>) -> CommandResult<Comparison> {
    let ((main, main_account), (other, other_account)) = sessions(&state)?;
    let (main_list, other_list) = tokio::try_join!(
        main.fetch_list(&main_account.username),
        other.fetch_list(&other_account.username),
    )?;
    let main_ids: Vec<i32> = main_list.entries.iter().map(|e| e.anime_id).collect();
    let other_ids: Vec<i32> = other_list.entries.iter().map(|e| e.anime_id).collect();
    let to_other = state.sync.match_ids(&main, &other, &main_ids).await?;
    let to_main = state.sync.match_ids(&other, &main, &other_ids).await?;
    let comparison = Comparison {
        main_service: main.service(),
        main_name: main.service().name(),
        other_service: other.service(),
        other_name: other.service().name(),
        time: now(),
        rows: rows(&Lists {
            main: &main_list,
            other: &other_list,
            to_other: &to_other,
            to_main: &to_main,
        }),
    };
    *state.compare.last.lock().await = Some(comparison.clone());
    Ok(comparison)
}

/// The last comparison, if any was made since the app started.
#[tauri::command]
pub async fn compare_last(state: State<'_, AppState>) -> CommandResult<Option<Comparison>> {
    Ok(state.compare.last.lock().await.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// Make the main list match the other one.
    Main,
    /// Make the other list match the main one.
    Other,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    comparison: Comparison,
    copied: usize,
    errors: Vec<String>,
}

/// Copies the rows with `keys` to `target`: the source's entry is saved
/// there, or removed there if the source doesn't have it.
#[tauri::command]
pub async fn compare_apply(
    state: State<'_, AppState>,
    keys: Vec<String>,
    target: Target,
) -> CommandResult<ApplyResult> {
    let ((main, _), (other, other_account)) = sessions(&state)?;
    let mut last = state.compare.last.lock().await;
    let comparison = last
        .as_mut()
        .ok_or_else(|| CommandError::new("Compare the lists first."))?;
    let wanted: HashSet<&str> = keys.iter().map(String::as_str).collect();
    let mut copied = 0;
    let mut errors = Vec::new();

    for row in comparison
        .rows
        .iter_mut()
        .filter(|r| wanted.contains(r.key.as_str()))
    {
        let (Some(main_side), Some(other_side)) = (row.main.as_mut(), row.other.as_mut()) else {
            continue; // no match: nothing to copy to
        };
        let result: patchouli_sync::Result<()> = match target {
            Target::Other => match &main_side.entry {
                Some(entry) => {
                    let copy = ListEntry {
                        id: 0,
                        anime_id: other_side.anime_id,
                        ..entry.clone()
                    };
                    other.save_entry(&copy, Fields::all()).await.map(|_| ())
                }
                None => {
                    other
                        .delete_anime(&other_account.username, other_side.anime_id)
                        .await
                }
            },
            Target::Main => {
                copy_to_main(&state, &main, main_side.anime_id, other_side.entry.as_ref()).await
            }
        };
        match result {
            Ok(()) => {
                copied += 1;
                match target {
                    Target::Other => {
                        other_side.entry = main_side.entry.clone().map(|e| ListEntry {
                            id: 0,
                            anime_id: other_side.anime_id,
                            ..e
                        });
                    }
                    Target::Main => {
                        main_side.entry = other_side.entry.clone().map(|e| ListEntry {
                            id: 0,
                            anime_id: main_side.anime_id,
                            ..e
                        });
                    }
                }
                // Both sides now hold the same entry, or neither has one.
                row.fields.clear();
                row.kind = Kind::Same;
            }
            Err(e) => {
                let title = &main_side.title;
                errors.push(format!("{title}: {e}"));
                if e.is_unauthorized() {
                    break;
                }
            }
        }
    }
    // Rows removed from both lists no longer belong in the comparison.
    comparison.rows.retain(|r| {
        r.main.as_ref().is_some_and(|s| s.entry.is_some())
            || r.other.as_ref().is_some_and(|s| s.entry.is_some())
    });
    Ok(ApplyResult {
        comparison: comparison.clone(),
        copied,
        errors,
    })
}

/// Saves `entry` (from the other list) to the local list, which sends it to
/// the main service; removes the local entry when `entry` is `None`.
async fn copy_to_main(
    state: &AppState,
    main: &Client,
    anime_id: i32,
    entry: Option<&ListEntry>,
) -> patchouli_sync::Result<()> {
    let Some(entry) = entry else {
        state.sync.remove_entry(anime_id)?;
        return Ok(());
    };
    let known = anime::get(
        &state.db.lock().unwrap_or_else(|e| e.into_inner()),
        anime_id,
    )?
    .is_some();
    if !known {
        match main.fetch_anime(anime_id).await? {
            Some(fetched) => anime::upsert(
                &state.db.lock().unwrap_or_else(|e| e.into_inner()),
                &fetched,
            )?,
            None => {
                return Err(patchouli_sync::Error::NotConfigured(
                    "This anime no longer exists.".into(),
                ));
            }
        }
    }
    let local_id = list::get(
        &state.db.lock().unwrap_or_else(|e| e.into_inner()),
        anime_id,
    )?
    .map_or(0, |e| e.id);
    state.sync.import_entry(ListEntry {
        id: local_id,
        anime_id,
        status: if entry.status == ListStatus::NotInList {
            ListStatus::PlanToWatch
        } else {
            entry.status
        },
        ..entry.clone()
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use patchouli_core::FuzzyDate;

    use super::*;

    fn entry(anime_id: i32, episodes: i32, score: i32) -> ListEntry {
        ListEntry {
            anime_id,
            watched_episodes: episodes,
            score,
            status: ListStatus::Watching,
            ..ListEntry::default()
        }
    }

    fn named(id: i32) -> Anime {
        let mut a = Anime {
            id,
            ..Anime::default()
        };
        a.titles.romaji = format!("Anime {id}");
        a
    }

    #[test]
    fn scores_compare_at_whole_points() {
        assert!(differences(&entry(1, 3, 85), &entry(1, 3, 80)).is_empty());
        assert_eq!(
            differences(&entry(1, 3, 85), &entry(1, 4, 70)),
            ["progress", "score"]
        );
        let mut started = entry(1, 3, 0);
        started.date_started = FuzzyDate::new(2024, 1, 2);
        started.rewatching = true;
        assert_eq!(
            differences(&started, &entry(1, 3, 0)),
            ["status", "started"]
        );
    }

    #[test]
    fn sorts_entries_into_kinds() {
        // Main (AniList) IDs 1–4, other (MyAnimeList) IDs 101–105.
        let main = RemoteList {
            anime: (1..=4).map(named).collect(),
            entries: vec![
                entry(1, 3, 80),
                entry(2, 5, 0),
                entry(3, 1, 0),
                entry(4, 2, 0),
            ],
        };
        let other = RemoteList {
            anime: (101..=105).map(named).collect(),
            entries: vec![
                entry(101, 3, 80),
                entry(102, 6, 0),
                entry(105, 1, 0),
                entry(104, 1, 0),
            ],
        };
        let to_other = HashMap::from([(1, 101), (2, 102), (3, 103), (4, 0)]);
        let to_main = HashMap::from([(101, 1), (102, 2), (105, 5), (104, 0)]);
        let result = rows(&Lists {
            main: &main,
            other: &other,
            to_other: &to_other,
            to_main: &to_main,
        });
        let summary: Vec<(String, Kind)> = result.iter().map(|r| (r.key.clone(), r.kind)).collect();
        assert_eq!(
            summary,
            [
                ("2:102".to_owned(), Kind::Different),
                ("3:103".to_owned(), Kind::OnlyMain),
                ("5:105".to_owned(), Kind::OnlyOther),
                ("0:104".to_owned(), Kind::NoMatch),
                ("4:0".to_owned(), Kind::NoMatch),
                ("1:101".to_owned(), Kind::Same),
            ]
        );
        assert_eq!(result[0].fields, ["progress"]);
    }
}
