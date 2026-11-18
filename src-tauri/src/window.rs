//! Tray icon, close-to-tray, minimise-to-tray, start-in-tray and starting
//! with Windows.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::AppState;

pub const MAIN: &str = "main";

/// Passed when Windows starts the app at sign-in.
pub const AUTOSTART_ARG: &str = "--autostart";

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
    let settings = window
        .try_state::<AppState>()
        .map(|state| state.settings.get());
    match event {
        WindowEvent::CloseRequested { api, .. }
            if settings.as_ref().is_some_and(|s| s.app_close_to_tray) =>
        {
            api.prevent_close();
            let _ = window.hide();
        }
        // Minimising resizes the window; hide it to the tray instead.
        WindowEvent::Resized(_)
            if settings.as_ref().is_some_and(|s| s.app_minimize_to_tray)
                && window.is_minimized().unwrap_or(false) =>
        {
            let _ = window.hide();
        }
        _ => {}
    }
}

/// Registers or removes the app from the programs Windows starts at sign-in.
pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    let autostart = app.autolaunch();
    if autostart.is_enabled().unwrap_or(false) == enabled {
        return;
    }
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    match result {
        Ok(()) => log::info!("Start with Windows: {}", if enabled { "on" } else { "off" }),
        Err(e) => log::warn!("Couldn't change starting with Windows: {e}"),
    }
}

/// The window starts hidden (see `tauri.conf.json`) to avoid a flash, and is
/// shown here unless "start in tray" is on or Windows started the app.
pub fn show_on_start(app: &AppHandle, state: &AppState) {
    let autostarted = std::env::args().any(|a| a == AUTOSTART_ARG);
    if !state.settings.get().app_start_in_tray && !autostarted {
        show_main(app);
    }
}
