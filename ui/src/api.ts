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
  "app.backup.enabled": boolean;
  "app.backup.keep": number;
  "app.checkForUpdates": boolean;
  "app.closeToTray": boolean;
  "app.list.highlightNewEpisodes": boolean;
  "app.list.newEpisodesFirst": boolean;
  "app.minimizeToTray": boolean;
  "app.startWithWindows": boolean;
  "app.colorScheme": "system" | "light" | "dark";
  "app.hideNsfw": boolean;
  "app.startInTray": boolean;
  "app.titleLanguage": "romaji" | "english" | "native";
  "library.folders": string[];
  /** MB. */
  "library.minFileSize": number;
  "library.watchFolders": boolean;
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
  /** Copy list changes to the service that isn't `sync.service`. */
  "sync.mirror.enabled": boolean;
  "sync.service": ServiceId;
  "track.detection.disabledPlayers": string[];
  "track.detection.enabled": boolean;
  "track.detection.interval": number;
  "track.detection.streaming.enabled": boolean;
  "track.notify.recognized": boolean;
  "track.notify.notRecognized": boolean;
  "track.notify.airing": boolean;
  "track.update.delay": number;
  "track.update.libraryOnly": boolean;
  "track.update.pauseWhenUnfocused": boolean;
  "track.update.trigger": "afterDelay" | "onPlayerClose";
  /** Percent; 0 always uses the delay. */
  "track.update.watchedAt": number;
  "track.position.mpcPort": number;
  "track.position.vlcPort": number;
  "track.position.vlcPassword": string;
  "track.position.mpvIpc": string;
  "torrents.source": string;
  "torrents.searchUrl": string;
  "torrents.autoCheck.enabled": boolean;
  /** Minutes. */
  "torrents.autoCheck.interval": number;
  "torrents.autoCheck.action": "notify" | "download";
  "torrents.app.mode": "default" | "custom";
  "torrents.app.open": boolean;
  "torrents.app.path": string;
  "torrents.download.fileFolder": string;
  "torrents.download.useAnimeFolder": boolean;
  "torrents.download.fallbackToFolder": boolean;
  "torrents.download.folder": string;
  "torrents.download.createSubfolder": boolean;
  "torrents.download.sortBy": "episodeNumber" | "releaseDate";
  "torrents.download.descending": boolean;
  "torrents.download.useMagnet": boolean;
  "torrents.filter.enabled": boolean;
  "torrents.filter.archiveMaxCount": number;
  "torrents.filters": FeedFilter[];
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
  avatarUrl: string | null;
  /** The user's page on the service. */
  profileUrl: string | null;
  /** The selected service, whose list is shown. */
  current: boolean;
  connected: boolean;
  /** Changes to the main list are copied to this service. */
  mirror: boolean;
  mirrorConnected: boolean;
}

export interface Suggestion {
  anime: Anime;
  /** The anime in the list it's suggested because of. */
  becauseId: number;
  becauseTitle: string;
}

export interface RecommendationGroup {
  becauseId: number;
  becauseTitle: string;
  items: Anime[];
}

export interface BackupInfo {
  /** Local time, `2026-11-22_093000`. */
  name: string;
  /** Unix seconds. */
  time: number;
  /** Bytes. */
  size: number;
}

export interface MirrorStatus {
  enabled: boolean;
  service: ServiceId;
  name: string;
  connected: boolean;
  /** Changes waiting to be copied, including failed ones. */
  pending: number;
  failed: { animeId: number; title: string; error: string }[];
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
  /** Judged by the player's position; `remainingSecs` is playback time left. */
  byPosition: boolean;
  positionPercent: number;
  watchedAtPercent: number;
}

export interface SharingStatus {
  /** A Discord application ID is set (TRACKER_DISCORD_APP_ID). */
  discordConfigured: boolean;
  discord: "disabled" | "connecting" | "connected" | "invalidClientId";
  webhookError: string | null;
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

export interface TaigaFound {
  version: "V1" | "V2";
  path: string;
}

export interface TaigaDetection {
  found: TaigaFound[];
  /** Offer to import on start. */
  prompt: boolean;
}

export interface TaigaSummary {
  version: "V1" | "V2";
  path: string;
  service: ServiceId | null;
  serviceName: string | null;
  username: string | null;
  anime: number;
  entries: number;
  history: number;
  queue: number;
  settings: number;
  warnings: string[];
}

export type FilterElement =
  | "meta_id"
  | "meta_status"
  | "meta_type"
  | "meta_episodes"
  | "meta_date_start"
  | "meta_date_end"
  | "user_tags"
  | "user_status"
  | "local_episode_available"
  | "episode_title"
  | "episode_number"
  | "episode_version"
  | "episode_group"
  | "episode_video_resolution"
  | "episode_video_type"
  | "file_title"
  | "file_category"
  | "file_description"
  | "file_link"
  | "file_size";

export type FilterOperator =
  | "equals"
  | "notequals"
  | "gt"
  | "ge"
  | "lt"
  | "le"
  | "beginswith"
  | "endswith"
  | "contains"
  | "notcontains";

export interface FilterCondition {
  element: FilterElement;
  operator: FilterOperator;
  value: string;
}

/** A torrent filter, in Taiga's shortcodes. */
export interface FeedFilter {
  name: string;
  enabled: boolean;
  action: "discard" | "select" | "prefer";
  match: "all" | "any";
  option: "default" | "deactivate" | "hide";
  /** Only these anime; all if empty. */
  animeIds: number[];
  conditions: FilterCondition[];
}

export interface FilterPreset {
  description: string;
  isDefault: boolean;
  filter: FeedFilter;
}

export type FeedItemState = "blank" | "discardedNormal" | "discardedInactive" | "discardedHidden" | "selected";

export interface FeedItem {
  title: string;
  link: string;
  description: string;
  category: "Anime" | "Batch" | "Other";
  sourceCategory: string;
  guid: string;
  pubDate: string;
  pubTime: number;
  infoLink: string;
  magnetLink: string;
  fileSize: number;
  seeders: number | null;
  leechers: number | null;
  downloads: number | null;
  state: FeedItemState;
  animeId: number | null;
  newEpisode: boolean;
  episode: {
    title: string;
    number: string;
    version: number;
    group: string;
    resolution: string;
    video: string;
  };
}

export interface Feed {
  url: string;
  title: string;
  items: FeedItem[];
}

export interface DownloadResult {
  downloaded: string[];
  errors: string[];
}

export interface Stats {
  animeCount: number;
  /** Counting rewatches. */
  episodeCount: number;
  secondsWatched: number;
  secondsPlanned: number;
  /** 0–100. */
  meanScore: number;
  scoreDeviation: number;
  /** Index n: scores from n×10 to n×10+9 (index 10: 100). */
  scoreCounts: number[];
  statusCounts: Partial<Record<ListStatus, number>>;
  typeCounts: Partial<Record<AnimeType, number>>;
  topGenres: [string, number][];
  historyCount: number;
  /** `YYYY-MM` and episodes watched, oldest first. */
  historyByMonth: [string, number][];
}

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

export interface AvailableUpdate {
  version: string;
  currentVersion: string;
  notes: string | null;
  date: string | null;
}

export const api = {
  updateStatus: () => invoke<{ configured: boolean; available: AvailableUpdate | null }>("update_status"),
  checkUpdate: () => invoke<AvailableUpdate | null>("update_check"),
  installUpdate: () => invoke<void>("update_install"),
  onUpdateAvailable: (handler: (update: AvailableUpdate) => void) =>
    listen<AvailableUpdate>("update-available", (event) => handler(event.payload)),
  onUpdateProgress: (handler: (progress: { downloaded: number; total: number | null }) => void) =>
    listen<{ downloaded: number; total: number | null }>("update-progress", (event) => handler(event.payload)),
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("settings_get"),
  /** Returns the keys that changed. */
  setSettings: (settings: Settings) => invoke<string[]>("settings_set", { settings }),
  onSettingsChanged: (handler: (keys: string[]) => void): Promise<UnlistenFn> =>
    listen<string[]>("settings-changed", (event) => handler(event.payload)),

  accounts: () => invoke<AccountStatus[]>("accounts_get"),
  /** Opens the browser and waits for the login to finish (up to 5 minutes). */
  login: (service: ServiceId, replaceLocal = false, mirror = false) =>
    invoke<AccountStatus>("account_login", { service, replaceLocal, mirror }),
  mirrorStatus: () => invoke<MirrorStatus>("mirror_status"),
  backups: () => invoke<BackupInfo[]>("backups_get"),
  backupNow: () => invoke<BackupInfo>("backup_now"),
  openBackupFolder: () => invoke<void>("backup_open_folder"),
  /** Backs up the current data, then restarts with backup `name`. */
  restoreBackup: (name: string) => invoke<void>("backup_restore", { name }),
  /** Sequels of completed anime that aren't in the list. */
  sequels: () => invoke<Suggestion[]>("sequels_get"),
  recommendations: () => invoke<RecommendationGroup[]>("recommendations_get"),
  dismissSuggestion: (animeId: number) => invoke<void>("suggestion_dismiss", { animeId }),
  /** Checks sequels and recommendations again; returns how many anime were checked. */
  discoverRefresh: () => invoke<number>("discover_refresh"),
  onDiscoverChanged: (handler: () => void): Promise<UnlistenFn> => listen("discover-changed", () => handler()),
  /** Queues the whole list to be copied; returns how many entries. */
  mirrorCopyAll: () => invoke<number>("mirror_copy_all"),
  mirrorDismissFailed: () => invoke<number>("mirror_dismiss_failed"),
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

  sharingStatus: () => invoke<SharingStatus>("sharing_status"),
  /** Writes the list to `path`; returns the number of entries. */
  exportList: (format: "markdown" | "myAnimeListXml", path: string) =>
    invoke<number>("list_export", { format, path }),

  taigaDetect: () => invoke<TaigaDetection>("taiga_detect"),
  /** `path`: a Taiga data folder or portable Taiga folder; default location if null. */
  taigaPreview: (path: string | null) => invoke<TaigaSummary>("taiga_preview", { path }),
  taigaImport: (path: string | null, replaceLocal = false) =>
    invoke<TaigaSummary>("taiga_import", { path, replaceLocal }),
  taigaDismiss: () => invoke<void>("taiga_dismiss"),

  season: (season: string, year: number) => invoke<Anime[]>("season_get", { season, year }),
  stats: () => invoke<Stats>("stats_get"),
  /** Fetches the account again (name, picture). */
  refreshProfile: () => invoke<AccountStatus>("profile_refresh"),

  /** Episodes in the library folders, per anime ID. */
  libraryAvailable: () => invoke<Record<number, number[]>>("library_available"),
  onLibraryAvailable: (handler: (available: Record<number, number[]>) => void): Promise<UnlistenFn> =>
    listen<Record<number, number[]>>("library-available", (event) => handler(event.payload)),

  torrents: () => invoke<Feed | null>("torrents_get"),
  /** Checks `url`, or the feed in settings. */
  checkTorrents: (url: string | null = null) => invoke<Feed>("torrents_check", { url }),
  searchTorrents: (text: string) => invoke<Feed>("torrents_search", { text }),
  setTorrentState: (link: string, selected: boolean) =>
    invoke<Feed | null>("torrents_set_state", { link, selected }),
  /** Downloads the given links, or every selected item if empty. */
  downloadTorrents: (links: string[] = []) => invoke<DownloadResult>("torrents_download", { links }),
  filterPresets: () => invoke<FilterPreset[]>("torrents_presets"),
  torrentArchive: () => invoke<{ count: number }>("torrents_archive_info"),
  clearTorrentArchive: () => invoke<void>("torrents_archive_clear"),
  onTorrents: (handler: (feed: Feed) => void): Promise<UnlistenFn> =>
    listen<Feed>("torrents", (event) => handler(event.payload)),

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
