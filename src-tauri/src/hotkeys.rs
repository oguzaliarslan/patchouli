//! System-wide keyboard shortcuts (`app.hotkeys.*` settings).
//!
//! Accelerators are strings like `Ctrl+Alt+Shift+Up`; an empty one is off.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use patchouli_settings::Settings;
use serde::Serialize;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Record the next episode of what's playing as watched.
    Increment,
    /// Turn player detection off or on.
    ToggleDetection,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    action: Action,
    accelerator: String,
    /// Why it isn't registered, if it isn't.
    error: Option<String>,
}

/// Registered shortcut IDs and what they do, and the last registration.
#[derive(Default)]
struct Registered {
    actions: HashMap<u32, Action>,
    status: Vec<HotkeyStatus>,
}

fn registered() -> &'static Mutex<Registered> {
    static REGISTERED: OnceLock<Mutex<Registered>> = OnceLock::new();
    REGISTERED.get_or_init(Mutex::default)
}

fn configured(settings: &Settings) -> [(Action, &str); 2] {
    [
        (Action::Increment, settings.hotkey_increment.trim()),
        (
            Action::ToggleDetection,
            settings.hotkey_toggle_detection.trim(),
        ),
    ]
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let action = registered()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .actions
                .get(&shortcut.id())
                .copied();
            if let Some(action) = action {
                run(app, action);
            }
        })
        .build()
}

/// Registers the shortcuts in `settings`, replacing any registered before.
pub fn apply(app: &AppHandle, settings: &Settings) {
    let shortcuts = app.global_shortcut();
    if let Err(e) = shortcuts.unregister_all() {
        log::warn!("Unregistering shortcuts: {e}");
    }
    let mut actions = HashMap::new();
    let mut status = Vec::new();
    for (action, accelerator) in configured(settings) {
        let error = if accelerator.is_empty() {
            None
        } else {
            match accelerator.parse::<Shortcut>() {
                Err(e) => Some(format!("Not a valid shortcut: {e}")),
                Ok(shortcut) => match shortcuts.register(shortcut) {
                    Ok(()) => {
                        actions.insert(shortcut.id(), action);
                        None
                    }
                    Err(e) => {
                        log::warn!("Registering {accelerator}: {e}");
                        Some("Another app is using this shortcut.".into())
                    }
                },
            }
        };
        status.push(HotkeyStatus {
            action,
            accelerator: accelerator.to_owned(),
            error,
        });
    }
    *registered().lock().unwrap_or_else(|e| e.into_inner()) = Registered { actions, status };
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let mut builder = app.notification().builder().title(title);
    if !body.is_empty() {
        builder = builder.body(body);
    }
    let _ = builder.show();
}

fn run(app: &AppHandle, action: Action) {
    let state = app.state::<AppState>();
    match action {
        Action::Increment => {
            let Some(anime_id) = state.tracker.now_playing().and_then(|n| n.anime_id) else {
                notify(app, "Nothing recognised is playing", "");
                return;
            };
            match crate::tracking::increment(app, &state, anime_id) {
                Some((title, episode)) => {
                    notify(app, &format!("Episode {episode} watched"), &title)
                }
                None => notify(app, "Couldn't update the list", ""),
            }
        }
        Action::ToggleDetection => {
            let enabled = !state.settings.get().detection_enabled;
            match state.settings.update(|s| s.detection_enabled = enabled) {
                Ok(_) if enabled => notify(app, "Detection resumed", ""),
                Ok(_) => notify(
                    app,
                    "Detection paused",
                    "Players aren't checked until you turn it back on.",
                ),
                Err(e) => log::warn!("Toggling detection: {e}"),
            }
        }
    }
}

#[tauri::command]
pub fn hotkeys_get(_state: State<'_, AppState>) -> Vec<HotkeyStatus> {
    registered()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .status
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_default_accelerators() {
        let settings = Settings::default();
        for (_, accelerator) in configured(&settings) {
            assert!(accelerator.parse::<Shortcut>().is_ok(), "{accelerator}");
        }
        assert!("Ctrl+Alt+Nope".parse::<Shortcut>().is_err());
    }
}
