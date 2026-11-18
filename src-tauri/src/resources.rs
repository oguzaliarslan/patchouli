//! Refreshes public recognition data into an app-managed cache.
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::AppState;

const DAY: Duration = Duration::from_secs(86_400);
const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const CACHE: &str = "resource-cache";
pub const RELATIONS: &str = "anime-relations.txt";
const PLAYERS: &str = "players.anisthesia";

pub fn players(data: &Path) -> Vec<patchouli_detect::Player> {
    std::fs::read_to_string(data.join(CACHE).join(PLAYERS))
        .ok()
        .and_then(|s| patchouli_detect::parse_players(&s).ok())
        .filter(|players| !players.is_empty())
        .unwrap_or_else(|| {
            patchouli_detect::parse_players(include_str!("../resources/players.anisthesia"))
                .expect("bundled player list")
        })
}

async fn refresh(
    http: &reqwest::Client,
    directory: &Path,
    name: &str,
    url: &str,
    valid: impl Fn(&str) -> bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let destination = directory.join(name);
    if destination
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age < DAY)
    {
        return Ok(());
    }
    let mut response = http.get(url).send().await?.error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_BYTES {
            return Err("resource exceeds the size limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    if !valid(std::str::from_utf8(&body)?) {
        return Err("resource failed format validation".into());
    }
    std::fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(&body)?;
    temporary.persist(destination)?;
    Ok(())
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let state = app.state::<AppState>();
            // Made each time, so a changed proxy applies.
            let http = patchouli_sync::http::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build();
            if let Ok(http) = http
                && state.settings.get().app_check_for_updates
            {
                let cache = state.data_dir.join(CACHE);
                let relations = refresh(&http, &cache, RELATIONS,
                    "https://raw.githubusercontent.com/erengy/anime-relations/master/anime-relations.txt",
                    |text| !patchouli_recognition::Relations::parse(text, patchouli_core::ServiceId::AniList).is_empty(),
                ).await;
                match relations {
                    Ok(()) => state.recognition.invalidate(),
                    Err(error) => log::warn!("Relation refresh: {error}"),
                }
                if let Err(error) = refresh(&http, &cache, PLAYERS,
                    "https://raw.githubusercontent.com/erengy/anisthesia/main/data/players.anisthesia",
                    |text| patchouli_detect::parse_players(text).is_ok_and(|players| !players.is_empty()),
                ).await {
                    log::warn!("Player list refresh: {error}");
                }
            }
            tokio::time::sleep(DAY).await;
        }
    });
}
