// Sample backend for running the UI in a plain browser during development
// (`npm run ui:dev`, then open http://localhost:1420). Never used in the app.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AccountStatus,
  Anime,
  AnimeSettings,
  HistoryItem,
  ListEntry,
  ListStatus,
  QueueItem,
  SearchParams,
  Settings,
} from "./api";

const FIRST = ["Starlit", "Crimson", "Silent", "Clockwork", "Paper", "Endless", "Hollow", "Azure", "Iron", "Little", "Midnight", "Wandering"];
const SECOND = ["Garden", "Railway", "Witch", "Academy", "Harbor", "Lantern", "Kingdom", "Orchestra", "Satellite", "Detective", "Bakery", "Festival"];
const SUFFIX = ["", "", "", " 2nd Season", " Movie", ": Final Chapter", " OVA"];
const TYPES: Anime["type"][] = ["Tv", "Tv", "Tv", "Movie", "Ova", "Ona", "Special"];
const GENRES = ["Action", "Comedy", "Drama", "Fantasy", "Mystery", "Romance", "Sci-Fi", "Slice of Life"];
const STATUSES: ListStatus[] = ["Watching", "Completed", "Completed", "OnHold", "Dropped", "PlanToWatch"];

/** Deterministic pseudo-random numbers (mulberry32), so the sample data is stable. */
function random(seed: number) {
  let s = (seed * 2654435761) >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pad = (n: number) => String(n).padStart(2, "0");

function makeAnime(id: number): Anime {
  const r = random(id);
  const pick = <T,>(list: T[]) => list[Math.floor(r() * list.length)];
  const romaji = `${pick(FIRST)} ${pick(SECOND)}${pick(SUFFIX)}`;
  const type = pick(TYPES);
  const year = 2000 + Math.floor(r() * 26);
  const month = 1 + Math.floor(r() * 12);
  const episodes = type === "Movie" ? 1 : r() < 0.15 ? 0 : [12, 13, 24, 25, 26][Math.floor(r() * 5)];
  const airing = year >= 2025 && r() < 0.5;
  return {
    id,
    lastModified: 0,
    episodeCount: episodes,
    episodeLength: type === "Movie" ? 110 : 24,
    ageRating: "Pg13",
    status: airing ? "Airing" : "FinishedAiring",
    type,
    dateStarted: `${year}-${pad(month)}-${pad(1 + Math.floor(r() * 28))}`,
    dateFinished: airing ? "0000-00-00" : `${year}-${pad(Math.min(12, month + 3))}-00`,
    score: Math.round((5 + r() * 4.5) * 100) / 100,
    popularityRank: Math.floor(r() * 5000) + 1,
    imageUrl: "",
    synopsis:
      "A sample synopsis for this made-up series.<br><br>It exists only so the interface can be tried out without logging in.",
    trailerId: "",
    titles: { romaji, english: r() < 0.5 ? `The ${romaji}` : "", japanese: "", synonyms: [] },
    genres: [pick(GENRES), pick(GENRES)].filter((g, i, a) => a.indexOf(g) === i),
    producers: [],
    studios: ["Studio Sample"],
    tags: [],
    lastAiredEpisode: airing ? Math.floor(episodes / 2) : 0,
    nextEpisodeTime: airing ? Math.floor(Date.now() / 1000) + 86400 * 3 : 0,
  };
}

const catalog = new Map<number, Anime>();
for (let id = 1; id <= 400; id++) catalog.set(id, makeAnime(id));

const now = () => Math.floor(Date.now() / 1000);
const entries = new Map<number, ListEntry>();
for (let id = 1; id <= 180; id++) {
  const r = random(id * 7);
  const anime = catalog.get(id)!;
  const status = STATUSES[Math.floor(r() * STATUSES.length)];
  const total = anime.episodeCount;
  const watched =
    status === "Completed" ? total : status === "PlanToWatch" ? 0 : Math.floor(r() * (total || 30));
  entries.set(id, {
    id: 1000 + id,
    animeId: id,
    watchedEpisodes: watched,
    score: status === "Completed" || r() < 0.3 ? Math.round(r() * 10) * 10 : 0,
    status,
    isPrivate: false,
    rewatchedTimes: 0,
    rewatching: false,
    rewatchingEp: 0,
    dateStarted: status === "PlanToWatch" ? "0000-00-00" : anime.dateStarted,
    dateCompleted: status === "Completed" ? anime.dateFinished : "0000-00-00",
    lastUpdated: now() - Math.floor(r() * 86400 * 200),
    notes: "",
    pendingDelete: false,
  });
}

const queue = new Map<number, QueueItem>();
const animeSettings = new Map<number, AnimeSettings>();
let history: HistoryItem[] = [];
let historyId = 1;
for (const e of [...entries.values()].filter((e) => e.status === "Watching").slice(0, 6)) {
  for (let ep = Math.max(1, e.watchedEpisodes - 2); ep <= e.watchedEpisodes; ep++) {
    history.push({ id: historyId++, animeId: e.animeId, episode: ep, time: e.lastUpdated - (e.watchedEpisodes - ep) * 3600 });
  }
}
history.sort((a, b) => a.time - b.time);

let settings: Settings = {
  "app.closeToTray": true,
  "app.colorScheme": "system",
  "app.hideNsfw": true,
  "app.startInTray": false,
  "app.titleLanguage": "romaji",
  "library.folders": ["D:\\Anime"],
  "meta.version": "0.1.0",
  "network.proxy.type": "http",
  "network.proxy.host": "",
  "network.proxy.port": -1,
  "network.proxy.username": "",
  "network.proxy.password": "",
  "sharing.enabled": true,
  "sharing.discord.enabled": false,
  "sharing.webhook.enabled": false,
  "sharing.webhook.headers": [],
  "sharing.webhook.url": "",
  "sync.enabled": true,
  "sync.service": "anilist",
  "track.detection.disabledPlayers": [],
  "track.detection.enabled": true,
  "track.detection.interval": 3000,
  "track.detection.streaming.enabled": false,
  "track.update.delay": 120,
  "track.update.libraryOnly": false,
  "track.update.pauseWhenUnfocused": false,
  "track.update.trigger": "afterDelay",
};

const accounts: Record<"anilist" | "myanimelist", string | null> = { anilist: "sample-user", myanimelist: null };

function accountStatus(): AccountStatus[] {
  return (["anilist", "myanimelist"] as const).map((service) => ({
    service,
    name: service === "anilist" ? "AniList" : "MyAnimeList",
    configured: true,
    scoreFormat: service === "anilist" ? "POINT_10_DECIMAL" : "POINT_10",
    username: accounts[service],
    current: settings["sync.service"] === service,
    connected: settings["sync.service"] === service && !!accounts[service],
  }));
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function listItems() {
  return [...entries.values()]
    .filter((e) => !e.pendingDelete)
    .map((entry) => ({
      entry,
      anime: catalog.get(entry.animeId) ?? null,
      displayTitle: animeSettings.get(entry.animeId)?.displayTitle || null,
      queued: queue.has(entry.animeId),
    }));
}

function details(id: number) {
  return {
    anime: catalog.get(id) ?? null,
    entry: entries.get(id) ?? null,
    settings: animeSettings.get(id) ?? null,
    queued: queue.get(id) ?? null,
    pageUrl: `https://anilist.co/anime/${id}`,
  };
}

function saveEntry(entry: ListEntry): boolean {
  const previous = entries.get(entry.animeId);
  if (previous && JSON.stringify(previous) === JSON.stringify(entry)) return false;
  const saved = { ...entry, id: previous?.id ?? 0, lastUpdated: now(), pendingDelete: false };
  entries.set(entry.animeId, saved);
  if (saved.watchedEpisodes > 0 && saved.watchedEpisodes !== previous?.watchedEpisodes) {
    history.push({ id: historyId++, animeId: entry.animeId, episode: saved.watchedEpisodes, time: now() });
  }
  queue.set(entry.animeId, { animeId: entry.animeId, dirty: "EPISODE", time: now(), retryCount: 0, lastError: null });
  void emit("sync", { type: "queueChanged" });
  return true;
}

const PLAYERS = ["MPC-HC", "mpv", "VLC media player", "PotPlayer", "MPC-BE", "Kodi", "SMPlayer", "Zoom Player"];

export function installMocks() {
  mockWindows("main");
  mockIPC(
    async (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "app_info":
          return {
            name: "tracker-app",
            version: "0.1.0 (browser preview)",
            dataDir: "C:\\Users\\Sample\\AppData\\Roaming\\app.tracker.desktop",
            portable: false,
            schemaVersion: 2,
            animeCount: catalog.size,
            listCount: entries.size,
            historyCount: history.length,
            queueCount: queue.size,
          };
        case "settings_get":
          return settings;
        case "settings_set": {
          const next = a.settings as Settings;
          const changed = Object.keys(next).filter(
            (k) => JSON.stringify(next[k]) !== JSON.stringify(settings[k]),
          );
          settings = next;
          void emit("settings-changed", changed);
          return changed;
        }
        case "accounts_get":
          return accountStatus();
        case "account_login": {
          await sleep(1200);
          const service = a.service as "anilist" | "myanimelist";
          accounts[service] = "sample-user";
          settings = { ...settings, "sync.service": service };
          return accountStatus().find((s) => s.service === service);
        }
        case "account_logout":
          accounts[a.service as "anilist" | "myanimelist"] = null;
          return accountStatus().find((s) => s.service === a.service);
        case "sync_now":
          await sleep(1500);
          queue.clear();
          void emit("sync", { type: "listChanged" });
          return null;
        case "list_get":
          return listItems();
        case "list_save_entry":
          return saveEntry(a.entry as ListEntry);
        case "list_remove_entry": {
          const id = a.animeId as number;
          const entry = entries.get(id);
          if (!entry) return false;
          entries.delete(id);
          queue.set(id, { animeId: id, dirty: "", time: now(), retryCount: 0, lastError: null });
          return true;
        }
        case "queue_get":
          return [...queue.values()];
        case "anime_get":
          return details(a.id as number);
        case "anime_refresh":
          await sleep(600);
          return details(a.id as number);
        case "anime_settings_save": {
          const s = a.settings as AnimeSettings;
          animeSettings.set(s.id, s);
          return null;
        }
        case "anime_search": {
          await sleep(500);
          const p = a.params as SearchParams;
          const text = (p.text ?? "").toLowerCase();
          return [...catalog.values()]
            .filter((x) => !text || x.titles.romaji.toLowerCase().includes(text))
            .filter((x) => !p.type || x.type === p.type)
            .filter((x) => !p.year || x.dateStarted.startsWith(String(p.year)))
            .slice(0, 50);
        }
        case "history_get":
          return {
            items: [...history].reverse(),
            anime: Object.fromEntries(history.map((h) => [h.animeId, catalog.get(h.animeId)])),
            displayTitles: Object.fromEntries(
              [...animeSettings.values()].filter((s) => s.displayTitle).map((s) => [s.id, s.displayTitle]),
            ),
          };
        case "history_remove":
          history = history.filter((h) => h.id !== a.id);
          return true;
        case "history_clear":
          history = [];
          return null;
        case "players_get":
          return PLAYERS;
        case "tracking_get": {
          const watching = [...entries.values()].find((e) => e.status === "Watching")!;
          const next = watching.watchedEpisodes + 3; // skips ahead, to show the prompt
          return {
            nowPlaying: {
              player: "mpv",
              browser: false,
              file: `D:\\Anime\\${catalog.get(watching.animeId)!.titles.romaji} - ${pad(next)}.mkv`,
              title: catalog.get(watching.animeId)!.titles.romaji,
              episodes: [pad(next)],
              animeId: watching.animeId,
              elements: [],
            },
            update: {
              phase: "confirming",
              reason: "skipsAhead",
              remainingSecs: 0,
              animeId: watching.animeId,
              episode: next,
              previousEpisode: watching.watchedEpisodes,
              paused: false,
            },
          };
        }
        case "update_accept":
        case "update_cancel": {
          const watching = [...entries.values()].find((e) => e.status === "Watching")!;
          return {
            phase: cmd === "update_accept" ? "committed" : "cancelled",
            reason: "none",
            remainingSecs: 0,
            animeId: watching.animeId,
            episode: watching.watchedEpisodes + 3,
            previousEpisode: watching.watchedEpisodes,
            paused: false,
          };
        }
        case "recognition_test": {
          // Rough stand-in for the real parser: `[Group] Title - 01 [Info].ext`.
          const input = String(a.input).split(/[\\/]/).pop() ?? "";
          const title = input.replace(/\[[^\]]*\]|\([^)]*\)/g, "").replace(/\.\w+$/, "").split(" - ")[0].trim();
          const number = /- (\d{1,4})/.exec(input)?.[1];
          const match = [...catalog.values()].find((x) => x.titles.romaji.toLowerCase() === title.toLowerCase());
          const elements = [
            { kind: "title", value: title, position: 0 },
            ...(number ? [{ kind: "episode", value: number, position: 0 }] : []),
          ];
          return {
            elements,
            titleFromPath: false,
            identification: {
              animeId: match?.id ?? null,
              normalizedTitle: title.toLowerCase().replace(/[^\p{L}\p{N}]/gu, ""),
              validType: true,
              candidates: match ? [{ id: match.id, weight: 1, known: true, validNumber: true, redirection: null }] : [],
              episode: { elements },
            },
            titles: match ? { [match.id]: match.titles.romaji } : {},
          };
        }
        case "anime_play":
          await sleep(400);
          throw { code: null, message: "Playing files isn't available in the browser preview." };
        case "anime_open_folder":
          throw { code: null, message: "Opening folders isn't available in the browser preview." };
        case "library_scan": {
          await sleep(800);
          const found = [...entries.values()].filter((e) => e.status === "Watching").slice(0, 8);
          return {
            scan: {
              anime: found.map((e) => ({
                animeId: e.animeId,
                files: Array.from({ length: Math.max(1, e.watchedEpisodes + 1) }, (_, i) => ({
                  path: `D:\\Anime\\${catalog.get(e.animeId)!.titles.romaji}\\${pad(i + 1)}.mkv`,
                  episodes: [i + 1],
                })),
              })),
              unrecognized: ["D:\\Anime\\Misc\\clip.mp4"],
              filesScanned: 42,
            },
            anime: Object.fromEntries(found.map((e) => [e.animeId, catalog.get(e.animeId)])),
          };
        }
        case "plugin:dialog|confirm":
          return window.confirm(String(a.message ?? ""));
        case "plugin:dialog|open":
          return window.prompt("Folder path") || null;
        case "plugin:opener|open_url":
          window.open(String(a.url), "_blank", "noopener");
          return null;
        default:
          // Window and other plugin calls have no effect here.
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
