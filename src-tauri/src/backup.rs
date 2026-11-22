//! Dated copies of the local data, made daily and kept up to a number.
//!
//! Each backup is a folder in `backups/` named by its local time
//! (`2026-11-22_093000`), holding a copy of the database (written with
//! `VACUUM INTO`, which is safe while it's open), settings, accounts and the
//! torrent archive.
//!
//! Restoring can't replace the open database, so it copies the backup to
//! `restore/` and restarts; [`apply_pending_restore`] moves the files into
//! place before anything is opened.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;
use crate::error::{CommandError, CommandResult};

const BACKUPS_DIR: &str = "backups";
const RESTORE_DIR: &str = "restore";
/// Copied as they are; the database is written separately.
const FILES: [&str; 3] = [
    patchouli_settings::FILE_NAME,
    patchouli_sync::accounts::FILE_NAME,
    crate::torrents::ARCHIVE_FILE,
];
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
const NAME_FORMAT: &str = "%Y-%m-%d_%H%M%S";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    name: String,
    /// Unix seconds.
    time: i64,
    /// Bytes, all files.
    size: u64,
}

fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUPS_DIR)
}

fn parse_time(name: &str) -> Option<i64> {
    let time = chrono::NaiveDateTime::parse_from_str(name, NAME_FORMAT).ok()?;
    Some(
        time.and_local_timezone(chrono::Local)
            .earliest()?
            .timestamp(),
    )
}

/// Backups, newest first. Folders not named like a backup are left out.
fn list(data_dir: &Path) -> Vec<BackupInfo> {
    let Ok(dir) = std::fs::read_dir(backups_dir(data_dir)) else {
        return Vec::new();
    };
    let mut result: Vec<BackupInfo> = dir
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let time = parse_time(&name)?;
            let size = std::fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .filter_map(|f| f.ok()?.metadata().ok())
                .map(|m| m.len())
                .sum();
            Some(BackupInfo { name, time, size })
        })
        .collect();
    result.sort_by(|a, b| b.name.cmp(&a.name));
    result
}

/// Writes a new backup and removes the oldest beyond `keep`.
fn create(state: &AppState) -> CommandResult<BackupInfo> {
    let name = chrono::Local::now().format(NAME_FORMAT).to_string();
    let dir = backups_dir(&state.data_dir).join(&name);
    std::fs::create_dir_all(&dir)?;
    let result = (|| -> CommandResult<()> {
        let db_copy = dir.join(patchouli_db::FILE_NAME);
        let db = state.db.lock()?;
        db.execute("VACUUM INTO ?1", [db_copy.to_string_lossy().as_ref()])
            .map_err(|e| CommandError::new(format!("Couldn't copy the database: {e}")))?;
        drop(db);
        for file in FILES {
            let source = state.data_dir.join(file);
            if source.is_file() {
                std::fs::copy(&source, dir.join(file))?;
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    prune(
        &state.data_dir,
        state.settings.get().app_backup_keep.max(1) as usize,
    );
    list(&state.data_dir)
        .into_iter()
        .find(|b| b.name == name)
        .ok_or_else(|| CommandError::new("The backup wasn't saved."))
}

fn prune(data_dir: &Path, keep: usize) {
    for old in list(data_dir).into_iter().skip(keep) {
        if let Err(e) = std::fs::remove_dir_all(backups_dir(data_dir).join(&old.name)) {
            log::warn!("Couldn't remove old backup {}: {e}", old.name);
        }
    }
}

/// Moves a backup staged by [`backup_restore`] into place. Call before the
/// settings and database are opened.
pub fn apply_pending_restore(data_dir: &Path) {
    let staged = data_dir.join(RESTORE_DIR);
    if !staged.is_dir() {
        return;
    }
    let db = staged.join(patchouli_db::FILE_NAME);
    if db.is_file() {
        // The write-ahead log belongs to the database being replaced.
        for suffix in ["-wal", "-shm"] {
            let _ =
                std::fs::remove_file(data_dir.join(format!("{}{suffix}", patchouli_db::FILE_NAME)));
        }
    }
    for file in FILES.iter().copied().chain([patchouli_db::FILE_NAME]) {
        let source = staged.join(file);
        if source.is_file() {
            if let Err(e) = std::fs::copy(&source, data_dir.join(file)) {
                log::error!("Restoring {file}: {e}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&staged);
    log::info!("Restored a backup");
}

/// Backs up once a day while backups are on.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(CHECK_INTERVAL);
        loop {
            tick.tick().await;
            let state = app.state::<AppState>();
            if !state.settings.get().app_backup_enabled {
                continue;
            }
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            if list(&state.data_dir)
                .iter()
                .any(|b| b.name.starts_with(&today))
            {
                continue;
            }
            match create(&state) {
                Ok(b) => log::info!("Backed up to {}", b.name),
                Err(e) => log::warn!("Daily backup: {}", e.message),
            }
        }
    });
}

#[tauri::command]
pub fn backups_get(state: State<'_, AppState>) -> Vec<BackupInfo> {
    list(&state.data_dir)
}

#[tauri::command]
pub fn backup_now(state: State<'_, AppState>) -> CommandResult<BackupInfo> {
    create(&state)
}

#[tauri::command]
pub fn backup_open_folder(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    let dir = backups_dir(&state.data_dir);
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| CommandError::new(format!("Couldn't open the folder: {e}")))
}

/// Backs up the current data, stages backup `name` and restarts to apply it.
#[tauri::command]
pub fn backup_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> CommandResult<()> {
    if parse_time(&name).is_none() {
        return Err(CommandError::new("No such backup."));
    }
    let source = backups_dir(&state.data_dir).join(&name);
    if !source.join(patchouli_db::FILE_NAME).is_file() {
        return Err(CommandError::new("That backup has no database."));
    }
    // So the restore can be undone.
    create(&state)?;
    let staged = state.data_dir.join(RESTORE_DIR);
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged)?;
    for entry in std::fs::read_dir(&source)? {
        let entry = entry?;
        std::fs::copy(entry.path(), staged.join(entry.file_name()))?;
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_local_times() {
        assert!(parse_time("2026-11-22_093000").is_some());
        assert!(parse_time("2026-11-22").is_none());
        assert!(parse_time("../media").is_none());
    }

    #[test]
    fn lists_newest_first_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "2026-11-20_090000",
            "2026-11-22_090000",
            "2026-11-21_090000",
            "notes",
        ] {
            std::fs::create_dir_all(dir.path().join(BACKUPS_DIR).join(name)).unwrap();
        }
        std::fs::write(
            dir.path()
                .join(BACKUPS_DIR)
                .join("2026-11-22_090000")
                .join(patchouli_db::FILE_NAME),
            b"1234",
        )
        .unwrap();
        let names = |d: &Path| list(d).into_iter().map(|b| b.name).collect::<Vec<_>>();
        assert_eq!(
            names(dir.path()),
            [
                "2026-11-22_090000",
                "2026-11-21_090000",
                "2026-11-20_090000"
            ]
        );
        assert_eq!(list(dir.path())[0].size, 4);
        prune(dir.path(), 2);
        assert_eq!(
            names(dir.path()),
            ["2026-11-22_090000", "2026-11-21_090000"]
        );
        // Not a backup, so never removed.
        assert!(dir.path().join(BACKUPS_DIR).join("notes").is_dir());
    }

    #[test]
    fn applies_a_staged_restore() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        std::fs::write(data.join(patchouli_db::FILE_NAME), b"current").unwrap();
        std::fs::write(data.join("media.sqlite-wal"), b"log").unwrap();
        std::fs::write(data.join(patchouli_settings::FILE_NAME), b"{}").unwrap();
        std::fs::create_dir_all(data.join(RESTORE_DIR)).unwrap();
        std::fs::write(data.join(RESTORE_DIR).join(patchouli_db::FILE_NAME), b"old").unwrap();

        apply_pending_restore(data);
        assert_eq!(
            std::fs::read(data.join(patchouli_db::FILE_NAME)).unwrap(),
            b"old"
        );
        assert!(!data.join("media.sqlite-wal").exists());
        // Files the backup didn't have are kept.
        assert!(data.join(patchouli_settings::FILE_NAME).is_file());
        assert!(!data.join(RESTORE_DIR).exists());
    }
}
