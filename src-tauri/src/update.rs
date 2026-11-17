//! App updates from GitHub Releases (`tauri-plugin-updater`).
//!
//! Releases come from [`GITHUB_REPO`]; a build can use another repository
//! with `TRACKER_GITHUB_REPO=owner/repo`. The repository must be public. Releases carry a `latest.json` (written by
//! `scripts/release.ps1`) and installers signed with the updater key, whose
//! public half is in `tauri.conf.json`.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Url};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;
use crate::error::{CommandError, CommandResult};

/// Event carrying an [`UpdateInfo`] when a check finds a new version.
pub const UPDATE_EVENT: &str = "update-available";
/// Progress events while downloading: `{ downloaded, total }`.
pub const PROGRESS_EVENT: &str = "update-progress";

/// `owner/repo` that releases are published to.
pub const GITHUB_REPO: Option<&str> = match option_env!("TRACKER_GITHUB_REPO") {
    Some(repo) => Some(repo),
    None => Some("oguzaliarslan/patchouli"),
};

/// The manifest URL for a repository.
pub fn manifest_url(repo: &str) -> Option<Url> {
    let repo = repo.trim().trim_matches('/');
    let valid = repo.split_once('/').is_some_and(|(owner, name)| {
        !owner.is_empty() && !name.is_empty() && name != "." && name != ".." && !name.contains('/')
    }) && repo
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if !valid {
        return None;
    }
    Url::parse(&format!(
        "https://github.com/{repo}/releases/latest/download/latest.json"
    ))
    .ok()
}

#[derive(Default)]
pub struct Updates {
    /// The update found by the last check, ready to install.
    pending: Mutex<Option<Update>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// A repository was set at build time.
    configured: bool,
    available: Option<UpdateInfo>,
}

async fn check(app: &AppHandle) -> CommandResult<Option<UpdateInfo>> {
    if app.state::<AppState>().portable {
        return Err(CommandError::new(
            "Download a new portable ZIP from GitHub Releases.",
        ));
    }
    let url = GITHUB_REPO
        .and_then(manifest_url)
        .ok_or_else(|| CommandError::new("Updates aren't set up in this build."))?;
    let update = app
        .updater_builder()
        .endpoints(vec![url])?
        .timeout(Duration::from_secs(30))
        .build()?
        .check()
        .await?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    let state = app.state::<AppState>();
    *state
        .updates
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = update;
    if let Some(info) = &info {
        log::info!("Update available: {}", info.version);
        let _ = app.emit(UPDATE_EVENT, info);
    }
    Ok(info)
}

#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> UpdateStatus {
    let pending = state
        .updates
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    UpdateStatus {
        configured: !state.portable && GITHUB_REPO.and_then(manifest_url).is_some(),
        available: pending.as_ref().map(|u| UpdateInfo {
            version: u.version.clone(),
            current_version: u.current_version.clone(),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CommandResult<Option<UpdateInfo>> {
    check(&app).await
}

/// Downloads and installs the pending update, then restarts. On Windows the
/// installer closes the app itself.
#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    if state.portable {
        return Err(CommandError::new(
            "Download a new portable ZIP from GitHub Releases.",
        ));
    }
    let update = state
        .updates
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .ok_or_else(|| CommandError::new("There's no update to install. Check again."))?;
    let progress = app.clone();
    let mut downloaded = 0u64;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(PROGRESS_EVENT, Progress { downloaded, total });
            },
            || log::info!("Update downloaded; installing"),
        )
        .await?;
    app.restart();
}

/// Checks once, a little after start, when enabled.
pub fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.portable || !state.settings.get().app_check_for_updates || GITHUB_REPO.is_none() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        match check(&app).await {
            Ok(Some(info)) => {
                let _ = app
                    .notification()
                    .builder()
                    .title("Update available")
                    .body(format!(
                        "Patchouli {} is available. Install it from Settings › Application.",
                        info.version
                    ))
                    .show();
            }
            Ok(None) => {}
            Err(e) => log::warn!("Update check: {}", e.message),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_urls() {
        assert_eq!(
            manifest_url("someone/patchouli").unwrap().as_str(),
            "https://github.com/someone/patchouli/releases/latest/download/latest.json"
        );
        assert!(manifest_url("someone").is_none());
        assert!(manifest_url("a/b/c").is_none());
        assert!(manifest_url("a/b?x=1").is_none());
    }
}
