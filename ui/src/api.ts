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

export type ServiceId = "anilist" | "myanimelist" | "kitsu";

export interface AccountStatus {
  service: ServiceId;
  name: string;
  /** A client ID is configured for this service. */
  configured: boolean;
  username: string | null;
  /** The selected service, whose list is shown. */
  current: boolean;
  connected: boolean;
}

export type ListStatus =
  | "NotInList"
  | "Watching"
  | "Completed"
  | "OnHold"
  | "Dropped"
  | "PlanToWatch";

export interface ListEntry {
  id: number;
  animeId: number;
  watchedEpisodes: number;
  /** 0–100 */
  score: number;
  status: ListStatus;
  isPrivate: boolean;
  rewatchedTimes: number;
  rewatching: boolean;
  rewatchingEp: number;
  /** YYYY-MM-DD, 00 for unknown parts */
  dateStarted: string;
  dateCompleted: string;
  lastUpdated: number;
  notes: string;
  pendingDelete: boolean;
}

export interface Anime {
  id: number;
  episodeCount: number;
  type: string;
  status: string;
  imageUrl: string;
  score: number;
  titles: { romaji: string; english: string; japanese: string; synonyms: string[] };
  [key: string]: unknown;
}

export interface ListItem {
  entry: ListEntry;
  anime: Anime | null;
}

export interface QueueItem {
  animeId: number;
  dirty: string;
  time: number;
  retryCount: number;
  lastError: string | null;
}

export type SyncEvent =
  | { type: "queueChanged" }
  | { type: "listChanged" }
  | { type: "busy"; busy: boolean }
  | { type: "error"; message: string; unauthorized: boolean };

/** What commands reject with. */
export interface CommandError {
  code: string | null;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String(e.message);
  return String(e);
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  accounts: () => invoke<AccountStatus[]>("accounts_get"),
  /** Opens the browser and waits for the login to finish (up to 5 minutes). */
  login: (service: ServiceId, replaceLocal = false) =>
    invoke<AccountStatus>("account_login", { service, replaceLocal }),
  logout: (service: ServiceId) => invoke<AccountStatus>("account_logout", { service }),
  syncNow: () => invoke<void>("sync_now"),
  list: () => invoke<ListItem[]>("list_get"),
  saveEntry: (entry: ListEntry) => invoke<boolean>("list_save_entry", { entry }),
  removeEntry: (animeId: number) => invoke<boolean>("list_remove_entry", { animeId }),
  queue: () => invoke<QueueItem[]>("queue_get"),
  onSync: (handler: (event: SyncEvent) => void): Promise<UnlistenFn> =>
    listen<SyncEvent>("sync", (event) => handler(event.payload)),
  getSettings: () => invoke<Settings>("settings_get"),
  /** Returns the keys that changed. */
  setSettings: (settings: Settings) => invoke<string[]>("settings_set", { settings }),
  onSettingsChanged: (handler: (keys: string[]) => void): Promise<UnlistenFn> =>
    listen<string[]>("settings-changed", (event) => handler(event.payload)),
};
