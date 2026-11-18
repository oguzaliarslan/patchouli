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

use patchouli_core::feed::Filter;
use patchouli_core::{ServiceId, TitleLanguage};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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

/// What to do when an automatic feed check selects new items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TorrentNewAction {
    #[default]
    Notify,
    Download,
}

/// Which program opens downloaded torrents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TorrentApp {
    /// The program Windows associates with `.torrent` files and magnet links.
    #[default]
    Default,
    Custom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TorrentSortBy {
    #[default]
    EpisodeNumber,
    ReleaseDate,
}

/// Taiga v1's default feed (`rss/torrent/source/address`).
pub const DEFAULT_TORRENT_SOURCE: &str = "https://www.tokyotosho.info/rss.php?filter=1,11&zwnj=0";
/// Taiga v1's default search (`%title%` is replaced).
pub const DEFAULT_TORRENT_SEARCH: &str = "https://nyaa.si/?page=rss&c=1_2&f=0&q=%title%";

/// All settings, keyed as in Taiga's `taiga/settings.hpp`. Torrent settings
/// (`torrents.*`) are from Taiga v1, which v2 doesn't have yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Check GitHub Releases for a new version on start.
    #[serde(rename = "app.checkForUpdates")]
    pub app_check_for_updates: bool,
    #[serde(rename = "app.closeToTray")]
    pub app_close_to_tray: bool,
    /// Mark list entries whose next episode is in the library.
    #[serde(rename = "app.list.highlightNewEpisodes")]
    pub list_highlight_new_episodes: bool,
    /// …and show them first.
    #[serde(rename = "app.list.newEpisodesFirst")]
    pub list_new_episodes_first: bool,
    #[serde(rename = "app.minimizeToTray")]
    pub app_minimize_to_tray: bool,
    /// Registered to start when the user signs in to Windows.
    #[serde(rename = "app.startWithWindows")]
    pub app_start_with_windows: bool,
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
    /// Video files smaller than this (MB) are ignored.
    #[serde(rename = "library.minFileSize")]
    pub library_min_file_size_mb: u32,
    /// Rescan when files in the library folders change.
    #[serde(rename = "library.watchFolders")]
    pub library_watch_folders: bool,

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
    /// Notify when an episode is recognised.
    #[serde(rename = "track.notify.recognized")]
    pub notify_recognized: bool,
    /// Notify when something plays that isn't recognised.
    #[serde(rename = "track.notify.notRecognized")]
    pub notify_not_recognized: bool,

    #[serde(rename = "track.update.delay")]
    pub update_delay_secs: u32,
    #[serde(rename = "track.update.libraryOnly")]
    pub update_library_only: bool,
    #[serde(rename = "track.update.pauseWhenUnfocused")]
    pub update_pause_when_unfocused: bool,
    #[serde(rename = "track.update.trigger")]
    pub update_trigger: UpdateTrigger,

    #[serde(rename = "torrents.source")]
    pub torrent_source: String,
    #[serde(rename = "torrents.searchUrl")]
    pub torrent_search_url: String,
    #[serde(rename = "torrents.autoCheck.enabled")]
    pub torrent_auto_check: bool,
    /// Minutes between automatic checks.
    #[serde(rename = "torrents.autoCheck.interval")]
    pub torrent_auto_check_interval: u32,
    #[serde(rename = "torrents.autoCheck.action")]
    pub torrent_new_action: TorrentNewAction,
    #[serde(rename = "torrents.app.mode")]
    pub torrent_app: TorrentApp,
    #[serde(rename = "torrents.app.open")]
    pub torrent_app_open: bool,
    #[serde(rename = "torrents.app.path")]
    pub torrent_app_path: String,
    /// Where `.torrent` files are saved; the data folder if empty.
    #[serde(rename = "torrents.download.fileFolder")]
    pub torrent_file_folder: String,
    /// Tell the client to save into the anime's folder.
    #[serde(rename = "torrents.download.useAnimeFolder")]
    pub torrent_use_anime_folder: bool,
    /// Without an anime folder, save into `torrents.download.folder`.
    #[serde(rename = "torrents.download.fallbackToFolder")]
    pub torrent_fallback_to_folder: bool,
    #[serde(rename = "torrents.download.folder")]
    pub torrent_download_folder: String,
    /// …in a subfolder named after the anime.
    #[serde(rename = "torrents.download.createSubfolder")]
    pub torrent_create_subfolder: bool,
    #[serde(rename = "torrents.download.sortBy")]
    pub torrent_sort_by: TorrentSortBy,
    #[serde(rename = "torrents.download.descending")]
    pub torrent_sort_descending: bool,
    #[serde(rename = "torrents.download.useMagnet")]
    pub torrent_use_magnet: bool,
    #[serde(rename = "torrents.filter.enabled")]
    pub torrent_filters_enabled: bool,
    #[serde(rename = "torrents.filter.archiveMaxCount")]
    pub torrent_archive_max_count: u32,
    #[serde(rename = "torrents.filters")]
    pub torrent_filters: Vec<Filter>,

    /// Keys this app doesn't use, kept so they survive a save.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    /// Taiga's defaults.
    fn default() -> Self {
        Self {
            app_check_for_updates: true,
            app_close_to_tray: true,
            list_highlight_new_episodes: true,
            list_new_episodes_first: false,
            app_minimize_to_tray: false,
            app_start_with_windows: false,
            app_color_scheme: ColorScheme::System,
            app_hide_nsfw: true,
            app_start_in_tray: false,
            app_title_language: TitleLanguage::Romaji,
            library_folders: Vec::new(),
            library_min_file_size_mb: 10,
            library_watch_folders: true,
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
            notify_recognized: true,
            notify_not_recognized: true,
            update_delay_secs: 120,
            update_library_only: false,
            update_pause_when_unfocused: false,
            update_trigger: UpdateTrigger::AfterDelay,
            torrent_source: DEFAULT_TORRENT_SOURCE.into(),
            torrent_search_url: DEFAULT_TORRENT_SEARCH.into(),
            // Taiga checks by default; off here until the user sets a feed up.
            torrent_auto_check: false,
            torrent_auto_check_interval: 60,
            torrent_new_action: TorrentNewAction::Notify,
            torrent_app: TorrentApp::Default,
            torrent_app_open: true,
            torrent_app_path: String::new(),
            torrent_file_folder: String::new(),
            torrent_use_anime_folder: true,
            torrent_fallback_to_folder: false,
            torrent_download_folder: String::new(),
            torrent_create_subfolder: false,
            torrent_sort_by: TorrentSortBy::EpisodeNumber,
            torrent_sort_descending: false,
            torrent_use_magnet: false,
            torrent_filters_enabled: true,
            torrent_archive_max_count: 1000,
            torrent_filters: patchouli_core::feed::default_filters(),
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
        // At most every 10 minutes, to go easy on the sites.
        self.torrent_auto_check_interval = self.torrent_auto_check_interval.clamp(10, 24 * 60);
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
