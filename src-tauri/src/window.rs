//! Tray icon, close-to-tray and start-in-tray.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::AppState;

pub const MAIN: &str = "main";

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Patchouli", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &sync, &separator, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Patchouli")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "sync" => {
                let state = app.state::<AppState>();
                if state.sync.is_logged_in() {
                    crate::sync::spawn_synchronize(app, state.sync.clone());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Hides the main window instead of closing it when "close to tray" is on.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        let close_to_tray = window
            .try_state::<AppState>()
            .is_some_and(|state| state.settings.get().app_close_to_tray);
        if close_to_tray {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

/// The window starts hidden (see `tauri.conf.json`) to avoid a flash, and is
/// shown here unless "start in tray" is on.
pub fn show_on_start(app: &AppHandle, state: &AppState) {
    if !state.settings.get().app_start_in_tray {
        show_main(app);
    }
}
