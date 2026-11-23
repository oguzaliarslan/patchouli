//! Poster disk cache (Taiga v1's `data/db/image`) and the local data figures
//! Taiga shows in its statistics.
//!
//! The web view loads posters through the `poster` URI scheme, which carries
//! the service's image URL. The first request downloads it with the shared
//! HTTP client (so the proxy setting applies) into `<data>/images/`; later
//! requests, offline ones too, are served from disk.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, State, UriSchemeContext, UriSchemeResponder};

use crate::AppState;
use crate::error::CommandResult;

/// The URI scheme posters are requested through.
pub const SCHEME: &str = "poster";
/// Folder in the data directory.
const IMAGES_DIR: &str = "images";
/// Larger responses aren't posters.
const MAX_BYTES: usize = 10 * 1024 * 1024;

pub fn images_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(IMAGES_DIR)
}

/// Decodes `%XX` escapes; `None` if the result isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The image URL a `poster` request carries (`convertFileSrc(url, "poster")`
/// on the UI side), if it's http(s).
fn image_url(request_path: &str) -> Option<String> {
    let url = percent_decode(request_path.trim_start_matches('/'))?;
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

/// FNV-1a: a stable file name for a URL, the same in every build.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

const EXTENSIONS: [(&str, &str); 4] = [
    ("jpg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];

fn content_type(extension: &str) -> &'static str {
    match extension {
        "jpeg" => "image/jpeg",
        _ => EXTENSIONS
            .iter()
            .find(|(ext, _)| *ext == extension)
            .map_or("application/octet-stream", |(_, mime)| mime),
    }
}

/// Where a URL is cached: its hash plus the URL's own extension.
fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let extension = url
        .split(['?', '#'])
        .next()
        .and_then(|path| path.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| ext == "jpeg" || EXTENSIONS.iter().any(|(e, _)| e == ext))
        .unwrap_or_else(|| "jpg".into());
    dir.join(format!("{:016x}.{extension}", fnv1a(url)))
}

async fn download(url: &str, path: &Path) -> Result<Vec<u8>, String> {
    let response = patchouli_sync::http::client()
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    let is_image = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("image/"));
    if !is_image {
        return Err("not an image".into());
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("image too large".into());
    }
    // Written beside the target and renamed, so a half-written file is never served.
    let dir = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "max-age=604800")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Handles a `poster` request: from disk, or downloaded and saved first.
pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn(async move {
        let Some(url) = image_url(&path) else {
            responder.respond(respond(StatusCode::BAD_REQUEST, "text/plain", Vec::new()));
            return;
        };
        let Some(state) = app.try_state::<AppState>() else {
            responder.respond(respond(
                StatusCode::SERVICE_UNAVAILABLE,
                "text/plain",
                Vec::new(),
            ));
            return;
        };
        let file = cache_path(&images_dir(&state.data_dir), &url);
        let mime = content_type(
            file.extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default(),
        );
        if let Ok(bytes) = tokio::fs::read(&file).await {
            responder.respond(respond(StatusCode::OK, mime, bytes));
            return;
        }
        match download(&url, &file).await {
            Ok(bytes) => responder.respond(respond(StatusCode::OK, mime, bytes)),
            Err(e) => {
                log::debug!("poster {url}: {e}");
                responder.respond(respond(StatusCode::BAD_GATEWAY, "text/plain", Vec::new()));
            }
        }
    });
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCount {
    pub count: usize,
    pub bytes: u64,
}

/// Files under `dir` (and its subfolders if `recursive`) with an extension in `extensions`.
fn count_files(dir: &Path, extensions: &[&str], recursive: bool) -> FileCount {
    let mut total = FileCount::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return total;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let path = entry.path();
        if meta.is_dir() {
            if recursive {
                let sub = count_files(&path, extensions, true);
                total.count += sub.count;
                total.bytes += sub.bytes;
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        {
            total.count += 1;
            total.bytes += meta.len();
        }
    }
    total
}

/// Taiga v1's "Local data" statistics.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalData {
    pub posters: FileCount,
    pub posters_folder: String,
    pub torrents: FileCount,
    pub torrents_folder: String,
}

#[tauri::command]
pub async fn local_data_get(state: State<'_, AppState>) -> CommandResult<LocalData> {
    let posters_dir = images_dir(&state.data_dir);
    let torrents_dir = crate::torrents::file_folder(&state.data_dir, &state.settings.get());
    let (posters, torrents) = tauri::async_runtime::spawn_blocking({
        let (p, t) = (posters_dir.clone(), torrents_dir.clone());
        move || {
            let images: Vec<&str> = EXTENSIONS.iter().map(|(e, _)| *e).chain(["jpeg"]).collect();
            (
                count_files(&p, &images, false),
                count_files(&t, &["torrent"], true),
            )
        }
    })
    .await
    .map_err(|e| crate::error::CommandError::new(e.to_string()))?;
    Ok(LocalData {
        posters,
        posters_folder: posters_dir.display().to_string(),
        torrents,
        torrents_folder: torrents_dir.display().to_string(),
    })
}

/// Deletes the cached posters; they're downloaded again when shown.
#[tauri::command]
pub async fn posters_clear(state: State<'_, AppState>) -> CommandResult<usize> {
    let dir = images_dir(&state.data_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(0);
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_image_url() {
        assert_eq!(
            image_url("/https%3A%2F%2Fs4.anilist.co%2Ffile%2Fcover%2Fbx1.png").as_deref(),
            Some("https://s4.anilist.co/file/cover/bx1.png")
        );
        assert_eq!(image_url("/file%3A%2F%2F%2FC%3A%2Fsecret"), None);
        assert_eq!(image_url("/%ZZ"), None);
        assert_eq!(percent_decode("100%").as_deref(), Some("100%"));
    }

    #[test]
    fn cache_names_are_stable() {
        let dir = Path::new("images");
        assert_eq!(
            cache_path(dir, "https://cdn.myanimelist.net/images/anime/1/2.jpg?s=x"),
            dir.join(format!(
                "{:016x}.jpg",
                fnv1a("https://cdn.myanimelist.net/images/anime/1/2.jpg?s=x")
            ))
        );
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert!(
            cache_path(dir, "https://x/y.webp")
                .ends_with(format!("{:016x}.webp", fnv1a("https://x/y.webp")))
        );
        assert!(
            cache_path(dir, "https://x/no-extension")
                .to_string_lossy()
                .ends_with(".jpg")
        );
    }

    #[test]
    fn counts_files_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.torrent"), b"12").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("b.TORRENT"), b"345").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"x").unwrap();
        assert_eq!(
            count_files(dir.path(), &["torrent"], true),
            FileCount { count: 2, bytes: 5 }
        );
        assert_eq!(
            count_files(dir.path(), &["torrent"], false),
            FileCount { count: 1, bytes: 2 }
        );
        assert_eq!(
            count_files(&dir.path().join("missing"), &["torrent"], true),
            FileCount::default()
        );
    }
}
