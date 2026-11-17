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
  "sync.service": ServiceId;
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
  /** `POINT_100`, `POINT_10_DECIMAL`, `POINT_10`, `POINT_5` or `POINT_3`. */
  scoreFormat: string | null;
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
  /** Unix seconds */
  lastUpdated: number;
  notes: string;
  pendingDelete: boolean;
}

export type AnimeType = "Unknown" | "Tv" | "Ova" | "Movie" | "Special" | "Ona" | "Music";
export type AiringStatus = "Unknown" | "FinishedAiring" | "Airing" | "NotYetAired";

export interface Anime {
  id: number;
  lastModified: number;
  /** -1 or 0 if unknown */
  episodeCount: number;
  /** Minutes */
  episodeLength: number;
  ageRating: "Unknown" | "G" | "Pg" | "Pg13" | "R17" | "R18";
  status: AiringStatus;
  type: AnimeType;
  dateStarted: string;
  dateFinished: string;
  /** 0–10 */
  score: number;
  popularityRank: number;
  imageUrl: string;
  synopsis: string;
  trailerId: string;
  titles: { romaji: string; english: string; japanese: string; synonyms: string[] };
  genres: string[];
  producers: string[];
  studios: string[];
  tags: string[];
  lastAiredEpisode: number;
  nextEpisodeTime: number;
}

export interface AnimeSettings {
  id: number;
  displayTitle: string;
  folder: string;
  synonyms: string[];
}

export interface ListItem {
  entry: ListEntry;
  anime: Anime | null;
  displayTitle: string | null;
  queued: boolean;
}

export interface QueueItem {
  animeId: number;
  /** e.g. "EPISODE | STATUS" */
  dirty: string;
  time: number;
  retryCount: number;
  lastError: string | null;
}

export interface AnimeDetails {
  anime: Anime | null;
  entry: ListEntry | null;
  settings: AnimeSettings | null;
  queued: QueueItem | null;
  pageUrl: string | null;
}

export interface HistoryItem {
  id: number;
  animeId: number;
  episode: number;
  time: number;
}

export interface HistoryView {
  items: HistoryItem[];
  anime: Record<string, Anime>;
  displayTitles: Record<string, string>;
}

export interface SearchParams {
  text?: string;
  year?: number | null;
  season?: "winter" | "spring" | "summer" | "fall" | null;
  type?: AnimeType | null;
  status?: AiringStatus | null;
  sort?: "title" | "duration" | "score" | "type" | "startDate" | null;
  descending?: boolean;
}

export interface AnitomyElement {
  kind: string;
  value: string;
  position: number;
}

export interface RecognitionCandidate {
  id: number;
  weight: number;
  known: boolean;
  validNumber: boolean;
  redirection: { id: number; episodes: [number, number] } | null;
}

export interface RecognitionTest {
  elements: AnitomyElement[];
  titleFromPath: boolean;
  identification: {
    animeId: number | null;
    normalizedTitle: string;
    validType: boolean;
    candidates: RecognitionCandidate[];
    episode: { elements: AnitomyElement[] };
  };
  titles: Record<string, string>;
}

export interface LibraryView {
  scan: {
    anime: { animeId: number; files: { path: string; episodes: number[] }[] }[];
    unrecognized: string[];
    filesScanned: number;
  };
  anime: Record<string, Anime>;
}

export interface NowPlaying {
  player: string;
  browser: boolean;
  file: string | null;
  title: string;
  /** Episode numbers after redirection. */
  episodes: string[];
  animeId: number | null;
  elements: AnitomyElement[];
}

export type UpdatePhase =
  | "idle"
  | "denied"
  | "countdown"
  | "waitingForClose"
  | "confirming"
  | "committed"
  | "cancelled";

export type UpdateReason = "none" | "invalidEpisode" | "outsideLibrary" | "alreadyWatched" | "skipsAhead";

export interface UpdateState {
  phase: UpdatePhase;
  reason: UpdateReason;
  remainingSecs: number;
  animeId: number;
  episode: number;
  previousEpisode: number;
  paused: boolean;
}

export interface TrackingState {
  nowPlaying: NowPlaying | null;
  update: UpdateState;
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

export function errorCode(e: unknown): string | null {
  if (e && typeof e === "object" && "code" in e) return (e as CommandError).code;
  return null;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("settings_get"),
  /** Returns the keys that changed. */
  setSettings: (settings: Settings) => invoke<string[]>("settings_set", { settings }),
  onSettingsChanged: (handler: (keys: string[]) => void): Promise<UnlistenFn> =>
    listen<string[]>("settings-changed", (event) => handler(event.payload)),

  accounts: () => invoke<AccountStatus[]>("accounts_get"),
  /** Opens the browser and waits for the login to finish (up to 5 minutes). */
  login: (service: ServiceId, replaceLocal = false) =>
    invoke<AccountStatus>("account_login", { service, replaceLocal }),
  logout: (service: ServiceId) => invoke<AccountStatus>("account_logout", { service }),
  syncNow: () => invoke<void>("sync_now"),
  onSync: (handler: (event: SyncEvent) => void): Promise<UnlistenFn> =>
    listen<SyncEvent>("sync", (event) => handler(event.payload)),

  list: () => invoke<ListItem[]>("list_get"),
  saveEntry: (entry: ListEntry) => invoke<boolean>("list_save_entry", { entry }),
  removeEntry: (animeId: number) => invoke<boolean>("list_remove_entry", { animeId }),
  queue: () => invoke<QueueItem[]>("queue_get"),

  anime: (id: number) => invoke<AnimeDetails>("anime_get", { id }),
  refreshAnime: (id: number) => invoke<AnimeDetails>("anime_refresh", { id }),
  saveAnimeSettings: (settings: AnimeSettings) =>
    invoke<void>("anime_settings_save", { settings }),
  search: (params: SearchParams) => invoke<Anime[]>("anime_search", { params }),

  history: () => invoke<HistoryView>("history_get"),
  removeHistory: (id: number) => invoke<boolean>("history_remove", { id }),
  clearHistory: () => invoke<void>("history_clear"),

  players: () => invoke<string[]>("players_get"),

  testRecognition: (input: string) => invoke<RecognitionTest>("recognition_test", { input }),
  /** Opens an episode from the library folders: `episode`, a random one, or the next one. */
  play: (animeId: number, options: { episode?: number; random?: boolean } = {}) =>
    invoke<{ episode: number; path: string }>("anime_play", {
      animeId,
      episode: options.episode ?? null,
      random: options.random ?? false,
    }),
  openFolder: (animeId: number) => invoke<string>("anime_open_folder", { animeId }),
  scanLibrary: () => invoke<LibraryView>("library_scan"),

  tracking: () => invoke<TrackingState>("tracking_get"),
  acceptUpdate: () => invoke<UpdateState>("update_accept"),
  cancelUpdate: () => invoke<UpdateState>("update_cancel"),
  onNowPlaying: (handler: (nowPlaying: NowPlaying | null) => void): Promise<UnlistenFn> =>
    listen<NowPlaying | null>("now-playing", (event) => handler(event.payload)),
  onUpdateState: (handler: (state: UpdateState) => void): Promise<UnlistenFn> =>
    listen<UpdateState>("update-state", (event) => handler(event.payload)),
};

/** A new list entry for an anime that isn't in the list. */
export function newEntry(animeId: number, status: ListStatus = "PlanToWatch"): ListEntry {
  return {
    id: 0,
    animeId,
    watchedEpisodes: 0,
    score: 0,
    status,
    isPrivate: false,
    rewatchedTimes: 0,
    rewatching: false,
    rewatchingEp: 0,
    dateStarted: "0000-00-00",
    dateCompleted: "0000-00-00",
    lastUpdated: 0,
    notes: "",
    pendingDelete: false,
  };
}
