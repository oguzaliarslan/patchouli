// Typed wrappers around Tauri commands. Replace with tauri-specta generated
// bindings once the command surface grows (see MIGRATION_PLAN.md §4).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppInfo {
  name: string;
  version: string;
  dataDir: string;
  portable: boolean;
  schemaVersion: number;
  animeCount: number;
  listCount: number;
  historyCount: number;
  queueCount: number;
}

/**
 * Settings as stored in settings.json: a flat object with Taiga's dotted keys.
 * Unknown keys are kept, so always send back the whole object.
 */
export interface Settings {
  "app.closeToTray": boolean;
  "app.colorScheme": "system" | "light" | "dark";
  "app.hideNsfw": boolean;
  "app.startInTray": boolean;
  "app.titleLanguage": "romaji" | "english" | "native";
  "library.folders": string[];
  "meta.version": string;
  "network.proxy.type": "http" | "socks5";
  "network.proxy.host": string;
  "network.proxy.port": number;
  "network.proxy.username": string;
  "network.proxy.password": string;
  "sharing.enabled": boolean;
  "sharing.discord.enabled": boolean;
  "sharing.webhook.enabled": boolean;
  "sharing.webhook.headers": string[];
  "sharing.webhook.url": string;
  "sync.enabled": boolean;
  "sync.service": "anilist" | "kitsu" | "myanimelist";
  "track.detection.disabledPlayers": string[];
  "track.detection.enabled": boolean;
  "track.detection.interval": number;
  "track.detection.streaming.enabled": boolean;
  "track.update.delay": number;
  "track.update.libraryOnly": boolean;
  "track.update.pauseWhenUnfocused": boolean;
  "track.update.trigger": "afterDelay" | "onPlayerClose";
  [key: string]: unknown;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("settings_get"),
  /** Returns the keys that changed. */
  setSettings: (settings: Settings) => invoke<string[]>("settings_set", { settings }),
  onSettingsChanged: (handler: (keys: string[]) => void): Promise<UnlistenFn> =>
    listen<string[]>("settings-changed", (event) => handler(event.payload)),
};
