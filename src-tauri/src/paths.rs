//! Where the app keeps its data.
//!
//! Portable mode, like Taiga's: if a file named `portable` sits next to the
//! executable, data goes in `data/` beside it. Otherwise it goes in the
//! per-user app data folder (`%APPDATA%\<identifier>` on Windows).

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Name of the marker file that turns on portable mode.
pub const PORTABLE_MARKER: &str = "portable";

pub struct DataDir {
    pub path: PathBuf,
    pub portable: bool,
}

pub fn data_dir(app: &AppHandle) -> tauri::Result<DataDir> {
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        && exe_dir.join(PORTABLE_MARKER).is_file()
    {
        return Ok(DataDir {
            path: exe_dir.join("data"),
            portable: true,
        });
    }
    Ok(DataDir {
        path: app.path().app_data_dir()?,
        portable: false,
    })
}
