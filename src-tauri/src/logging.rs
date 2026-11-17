//! Log files (Taiga `base/log.cpp`): `<data>/logs/tracker_<date>.log`.
//!
//! Warnings and errors by default; `--debug` adds debug messages and
//! `--verbose` everything. Debug builds log debug messages too.

use tauri::AppHandle;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use crate::paths;

pub fn level_from_args(args: &[String]) -> log::LevelFilter {
    if args.iter().any(|a| a == "--verbose") {
        log::LevelFilter::Trace
    } else if args.iter().any(|a| a == "--debug") || cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    }
}

pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let level = level_from_args(&args);
    let logs = paths::data_dir(app)?.path.join("logs");
    std::fs::create_dir_all(&logs)?;
    let date = chrono::Local::now().format("%Y-%m-%d");
    let plugin = tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::Folder {
            path: logs,
            file_name: Some(format!("tracker_{date}")),
        }))
        .target(Target::new(TargetKind::Stdout))
        .level(level)
        // Dependencies are noisy below warnings.
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("hyper_util", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .max_file_size(10_000_000)
        .rotation_strategy(RotationStrategy::KeepAll)
        .build();
    app.plugin(plugin)?;
    log::info!(
        "Version {} ({})",
        app.package_info().version,
        if args.len() > 1 {
            args[1..].join(" ")
        } else {
            "no options".into()
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            level_from_args(&args(&["app", "--verbose"])),
            log::LevelFilter::Trace
        );
        let default = level_from_args(&args(&["app"]));
        if cfg!(debug_assertions) {
            assert_eq!(default, log::LevelFilter::Debug);
        } else {
            assert_eq!(default, log::LevelFilter::Warn);
        }
    }
}
