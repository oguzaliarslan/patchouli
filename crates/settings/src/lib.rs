//! Application settings.
//!
//! [`Settings`] mirrors Taiga v2's `settings.json`: a flat JSON object with
//! dotted keys (`"track.update.delay": 120`) and the same defaults, so Taiga's
//! file can be read as-is. Keys this app doesn't know (e.g. Taiga's `app.style`)
//! are kept when the file is saved.
//!
//! [`SettingsStore`] loads and saves the file and reports which keys changed.

mod store;

pub use store::{Error, Listener, SettingsStore};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracker_core::{ServiceId, TitleLanguage};

/// File name Taiga uses for its settings.
pub const FILE_NAME: &str = "settings.json";

/// Bounds Taiga enforces on the update delay, in seconds.
pub const UPDATE_DELAY_MIN: u32 = 10;
pub const UPDATE_DELAY_MAX: u32 = 3600;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    #[default]
    Http,
    Socks5,
}

/// When a detected episode updates the list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateTrigger {
    /// After the episode has been playing for `track.update.delay` seconds.
    #[default]
    AfterDelay,
    /// When the player is closed.
    OnPlayerClose,
}

/// All settings, keyed as in Taiga's `taiga/settings.hpp`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "app.closeToTray")]
    pub app_close_to_tray: bool,
    #[serde(rename = "app.colorScheme")]
    pub app_color_scheme: ColorScheme,
    #[serde(rename = "app.hideNsfw")]
    pub app_hide_nsfw: bool,
    #[serde(rename = "app.startInTray")]
    pub app_start_in_tray: bool,
    #[serde(rename = "app.titleLanguage")]
    pub app_title_language: TitleLanguage,

    #[serde(rename = "library.folders")]
    pub library_folders: Vec<String>,

    /// App version that last wrote the file.
    #[serde(rename = "meta.version")]
    pub meta_version: String,

    #[serde(rename = "network.proxy.type")]
    pub proxy_type: ProxyType,
    #[serde(rename = "network.proxy.host")]
    pub proxy_host: String,
    /// -1 if unset.
    #[serde(rename = "network.proxy.port")]
    pub proxy_port: i32,
    #[serde(rename = "network.proxy.username")]
    pub proxy_username: String,
    /// TODO(phase 2): move to the OS keychain with the account tokens.
    #[serde(rename = "network.proxy.password")]
    pub proxy_password: String,

    #[serde(rename = "sharing.enabled")]
    pub sharing_enabled: bool,
    #[serde(rename = "sharing.discord.enabled")]
    pub discord_enabled: bool,
    #[serde(rename = "sharing.webhook.enabled")]
    pub webhook_enabled: bool,
    /// `Name: value` lines.
    #[serde(rename = "sharing.webhook.headers")]
    pub webhook_headers: Vec<String>,
    #[serde(rename = "sharing.webhook.url")]
    pub webhook_url: String,

    #[serde(rename = "sync.enabled")]
    pub sync_enabled: bool,
    #[serde(rename = "sync.service")]
    pub sync_service: ServiceId,

    /// Player names that are not detected.
    #[serde(rename = "track.detection.disabledPlayers")]
    pub disabled_players: Vec<String>,
    #[serde(rename = "track.detection.enabled")]
    pub detection_enabled: bool,
    /// Milliseconds between player polls.
    #[serde(rename = "track.detection.interval")]
    pub detection_interval_ms: u32,
    #[serde(rename = "track.detection.streaming.enabled")]
    pub streaming_detection_enabled: bool,

    /// Seconds, between [`UPDATE_DELAY_MIN`] and [`UPDATE_DELAY_MAX`].
    #[serde(rename = "track.update.delay")]
    pub update_delay_secs: u32,
    #[serde(rename = "track.update.libraryOnly")]
    pub update_library_only: bool,
    #[serde(rename = "track.update.pauseWhenUnfocused")]
    pub update_pause_when_unfocused: bool,
    #[serde(rename = "track.update.trigger")]
    pub update_trigger: UpdateTrigger,

    /// Keys this app doesn't use, kept so they survive a save.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    /// Taiga's defaults.
    fn default() -> Self {
        Self {
            app_close_to_tray: true,
            app_color_scheme: ColorScheme::System,
            app_hide_nsfw: true,
            app_start_in_tray: false,
            app_title_language: TitleLanguage::Romaji,
            library_folders: Vec::new(),
            meta_version: String::new(),
            proxy_type: ProxyType::Http,
            proxy_host: String::new(),
            proxy_port: -1,
            proxy_username: String::new(),
            proxy_password: String::new(),
            sharing_enabled: true,
            discord_enabled: false,
            webhook_enabled: false,
            webhook_headers: Vec::new(),
            webhook_url: String::new(),
            sync_enabled: true,
            sync_service: ServiceId::AniList,
            disabled_players: Vec::new(),
            detection_enabled: true,
            detection_interval_ms: 3000,
            streaming_detection_enabled: false,
            update_delay_secs: 120,
            update_library_only: false,
            update_pause_when_unfocused: false,
            update_trigger: UpdateTrigger::AfterDelay,
            extra: Map::new(),
        }
    }
}

impl Settings {
    /// Clamps values to the ranges Taiga allows.
    pub fn normalize(&mut self) {
        self.update_delay_secs = self
            .update_delay_secs
            .clamp(UPDATE_DELAY_MIN, UPDATE_DELAY_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_taiga_settings_file() {
        let json = r#"{
            "app.style": "fusion",
            "app.titleLanguage": "english",
            "library.folders": ["D:/Anime"],
            "meta.version": "2.0.0",
            "sync.service": "myanimelist",
            "track.update.delay": 90,
            "track.update.trigger": "onPlayerClose"
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.app_title_language, TitleLanguage::English);
        assert_eq!(settings.library_folders, ["D:/Anime"]);
        assert_eq!(settings.sync_service, ServiceId::MyAnimeList);
        assert_eq!(settings.update_delay_secs, 90);
        assert_eq!(settings.update_trigger, UpdateTrigger::OnPlayerClose);
        // Unspecified keys take Taiga's defaults.
        assert!(settings.app_close_to_tray);
        assert_eq!(settings.detection_interval_ms, 3000);
        // Unknown keys are kept.
        assert_eq!(settings.extra["app.style"], "fusion");

        let saved = serde_json::to_value(&settings).unwrap();
        assert_eq!(saved["app.style"], "fusion");
        assert_eq!(saved["track.update.trigger"], "onPlayerClose");
        assert_eq!(saved["app.colorScheme"], "system");
    }

    #[test]
    fn normalize_clamps_update_delay() {
        let mut settings = Settings {
            update_delay_secs: 1,
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.update_delay_secs, UPDATE_DELAY_MIN);
        settings.update_delay_secs = 99_999;
        settings.normalize();
        assert_eq!(settings.update_delay_secs, UPDATE_DELAY_MAX);
    }
}
