// Sample backend for running the UI in a plain browser during development
// (`npm run ui:dev`, then open http://localhost:1420). Never used in the app.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AccountStatus,
  Anime,
  Feed,
  FeedItem,
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
    // Spread over the coming week; a few have just aired.
    nextEpisodeTime: airing ? Math.floor(Date.now() / 1000) + Math.floor((r() * 7.2 - 0.2) * 86400) : 0,
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
let taigaDone = false;
const TAIGA_PATH = String.raw`C:\Users\demo\AppData\Roaming\erengy\taiga\data`;
for (const e of [...entries.values()].filter((e) => e.status === "Watching").slice(0, 6)) {
  for (let ep = Math.max(1, e.watchedEpisodes - 2); ep <= e.watchedEpisodes; ep++) {
    history.push({ id: historyId++, animeId: e.animeId, episode: ep, time: e.lastUpdated - (e.watchedEpisodes - ep) * 3600 });
  }
}
history.sort((a, b) => a.time - b.time);

let settings: Settings = {
  "app.checkForUpdates": true,
  "app.closeToTray": true,
  "app.list.highlightNewEpisodes": true,
  "app.list.newEpisodesFirst": false,
  "app.minimizeToTray": false,
  "app.startWithWindows": false,
  "app.colorScheme": "system",
  "app.hideNsfw": true,
  "app.startInTray": false,
  "app.titleLanguage": "romaji",
  "library.folders": ["D:\\Anime"],
  "library.minFileSize": 10,
  "library.watchFolders": true,
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
  "sync.mirror.enabled": false,
  "sync.service": "anilist",
  "track.detection.disabledPlayers": [],
  "track.detection.enabled": true,
  "track.detection.interval": 3000,
  "track.detection.streaming.enabled": false,
  "track.notify.recognized": true,
  "track.notify.notRecognized": true,
  "track.notify.airing": true,
  "track.update.delay": 120,
  "track.update.libraryOnly": false,
  "track.update.pauseWhenUnfocused": false,
  "track.update.trigger": "afterDelay",
  "torrents.source": "https://www.tokyotosho.info/rss.php?filter=1,11&zwnj=0",
  "torrents.searchUrl": "https://nyaa.si/?page=rss&c=1_2&f=0&q=%title%",
  "torrents.autoCheck.enabled": false,
  "torrents.autoCheck.interval": 60,
  "torrents.autoCheck.action": "notify",
  "torrents.app.mode": "default",
  "torrents.app.open": true,
  "torrents.app.path": "",
  "torrents.download.fileFolder": "",
  "torrents.download.useAnimeFolder": true,
  "torrents.download.fallbackToFolder": false,
  "torrents.download.folder": "",
  "torrents.download.createSubfolder": false,
  "torrents.download.sortBy": "episodeNumber",
  "torrents.download.descending": false,
  "torrents.download.useMagnet": false,
  "torrents.filter.enabled": true,
  "torrents.filter.archiveMaxCount": 1000,
  "torrents.filters": [
    {
      name: "Select currently watching",
      enabled: true,
      action: "select",
      match: "any",
      option: "default",
      animeIds: [],
      conditions: [{ element: "user_status", operator: "equals", value: "1" }],
    },
    {
      name: "Discard watched and available episodes",
      enabled: true,
      action: "discard",
      match: "any",
      option: "default",
      animeIds: [],
      conditions: [
        { element: "episode_number", operator: "le", value: "%watched%" },
        { element: "local_episode_available", operator: "equals", value: "True" },
      ],
    },
    {
      name: "Prefer high-resolution files",
      enabled: true,
      action: "prefer",
      match: "any",
      option: "default",
      animeIds: [],
      conditions: [{ element: "episode_video_resolution", operator: "equals", value: "1080p" }],
    },
  ],
};

/** A sample feed: the next episodes of anime being watched, in two resolutions. */
let torrentFeed: Feed | null = null;
let archived = 0;
function sampleFeed(url: string): Feed {
  const items: FeedItem[] = [];
  const watching = [...entries.values()].filter((e) => e.status === "Watching").slice(0, 8);
  const groups = ["SubsPlease", "Erai-raws"];
  watching.forEach((e, n) => {
    const anime = catalog.get(e.animeId)!;
    const group = groups[n % 2];
    for (const [resolution, size] of [["1080p", 1_400_000_000], ["720p", 700_000_000]] as const) {
      const episode = e.watchedEpisodes + 1;
      const title = `[${group}] ${anime.titles.romaji} - ${String(episode).padStart(2, "0")} (${resolution}).mkv`;
      items.push({
        title,
        link: `https://example.com/${e.animeId}-${resolution}.torrent`,
        description: `Size: ${(size / 1e9).toFixed(1)} GB`,
        category: "Anime",
        sourceCategory: "Anime - English-translated",
        guid: `https://example.com/view/${e.animeId}`,
        pubDate: "",
        pubTime: Math.floor(Date.now() / 1000) - n * 5400,
        infoLink: `https://example.com/view/${e.animeId}`,
        magnetLink: "",
        fileSize: size,
        seeders: 400 - n * 30,
        leechers: 12,
        downloads: 2000,
        state: resolution === "1080p" ? "selected" : "discardedNormal",
        animeId: e.animeId,
        newEpisode: true,
        episode: { title: anime.titles.romaji, number: String(episode), version: 1, group, resolution, video: "" },
      });
    }
  });
  items.push({
    ...items[0],
    title: "[Other] Unknown Show - 01 (1080p).mkv",
    link: "https://example.com/unknown.torrent",
    state: "discardedInactive",
    animeId: null,
    newEpisode: false,
    episode: { ...items[0].episode, title: "Unknown Show", number: "1", group: "Other" },
  });
  items.sort((a, b) => Number(b.state === "selected") - Number(a.state === "selected"));
  return { url, title: "Tokyo Toshokan", items };
}

const accounts: Record<"anilist" | "myanimelist", string | null> = { anilist: "sample-user", myanimelist: null };

function accountStatus(): AccountStatus[] {
  return (["anilist", "myanimelist"] as const).map((service) => ({
    service,
    name: service === "anilist" ? "AniList" : "MyAnimeList",
    configured: true,
    scoreFormat: service === "anilist" ? "POINT_10_DECIMAL" : "POINT_10",
    username: accounts[service],
    avatarUrl: null,
    profileUrl: accounts[service] ? `https://anilist.co/user/${accounts[service]}/` : null,
    current: settings["sync.service"] === service,
    connected: settings["sync.service"] === service && !!accounts[service],
    mirror: settings["sync.mirror.enabled"] && settings["sync.service"] !== service,
    mirrorConnected: settings["sync.mirror.enabled"] && settings["sync.service"] !== service && !!accounts[service],
  }));
}

/** Sample changes waiting to be copied to the second service. */
let mirrorFailed = [
  { animeId: 7, title: "Sample Anime With No Match", error: "Not found on MyAnimeList." },
];
let mirrorPending = 0;

function mirrorStatus() {
  const service = settings["sync.service"] === "anilist" ? "myanimelist" : "anilist";
  return {
    enabled: settings["sync.mirror.enabled"],
    service,
    name: service === "anilist" ? "AniList" : "MyAnimeList",
    connected: !!accounts[service],
    pending: mirrorPending + mirrorFailed.length,
    failed: mirrorFailed,
  };
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
            name: "patchouli",
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
          settings = a.mirror
            ? { ...settings, "sync.mirror.enabled": true }
            : { ...settings, "sync.service": service };
          return accountStatus().find((s) => s.service === service);
        }
        case "mirror_status":
          return mirrorStatus();
        case "mirror_copy_all":
          mirrorPending = listItems().length;
          setTimeout(() => {
            mirrorPending = 0;
          }, 5000);
          return mirrorPending;
        case "mirror_dismiss_failed": {
          const count = mirrorFailed.length;
          mirrorFailed = [];
          return count;
        }
        case "account_logout":
          accounts[a.service as "anilist" | "myanimelist"] = null;
          if (a.service !== settings["sync.service"]) settings = { ...settings, "sync.mirror.enabled": false };
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
        case "sharing_status":
          return {
            discordConfigured: true,
            discord: settings["sharing.discord.enabled"] ? "connecting" : "disabled",
            webhookError: null,
          };
        case "plugin:dialog|save":
          return window.prompt("Save as", "anime-list.md") || null;
        case "list_export":
          return entries.size;
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
        case "season_get": {
          await sleep(300);
          const wanted = `${a.season}`;
          return [...catalog.values()]
            .filter((x) => {
              const m = Number(x.dateStarted.slice(5, 7));
              const y = Number(x.dateStarted.slice(0, 4));
              const name = ["winter", "winter", "spring", "spring", "spring", "summer", "summer", "summer", "fall", "fall", "fall", "winter"][m - 1];
              return name === wanted && (m === 12 ? y + 1 : y) === a.year;
            })
            .sort((x, y) => x.popularityRank - y.popularityRank);
        }
        case "stats_get": {
          const items = [...entries.values()];
          const scoreCounts = Array(11).fill(0);
          const statusCounts: Record<string, number> = {};
          const typeCounts: Record<string, number> = {};
          const genres: Record<string, number> = {};
          let episodes = 0;
          for (const e of items) {
            const anime = catalog.get(e.animeId)!;
            episodes += e.watchedEpisodes;
            if (e.score) scoreCounts[Math.floor(e.score / 10)]++;
            statusCounts[e.status] = (statusCounts[e.status] ?? 0) + 1;
            typeCounts[anime.type] = (typeCounts[anime.type] ?? 0) + 1;
            for (const g of anime.genres) genres[g] = (genres[g] ?? 0) + 1;
          }
          const scores = items.filter((e) => e.score).map((e) => e.score);
          const mean = scores.reduce((x, y) => x + y, 0) / (scores.length || 1);
          const months: Record<string, number> = {};
          for (const h of history) {
            const d = new Date(h.time * 1000);
            const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
            months[key] = (months[key] ?? 0) + 1;
          }
          return {
            animeCount: items.length,
            episodeCount: episodes,
            secondsWatched: episodes * 24 * 60,
            secondsPlanned: 123 * 24 * 60,
            meanScore: mean,
            scoreDeviation: Math.sqrt(scores.reduce((x, s) => x + (s - mean) ** 2, 0) / (scores.length || 1)),
            scoreCounts,
            statusCounts,
            typeCounts,
            topGenres: Object.entries(genres).sort((x, y) => y[1] - x[1]).slice(0, 10),
            historyCount: history.length,
            historyByMonth: Object.entries(months).sort(),
          };
        }
        case "profile_refresh":
          return accountStatus().find((x) => x.current);
        case "library_available": {
          // Every other anime being watched has its next episode on disk.
          const map: Record<number, number[]> = {};
          [...entries.values()]
            .filter((e) => e.status === "Watching")
            .forEach((e, i) => {
              if (i % 2 === 0) map[e.animeId] = Array.from({ length: e.watchedEpisodes + 1 }, (_, n) => n + 1);
            });
          return map;
        }
        case "torrents_get":
          return torrentFeed;
        case "torrents_check":
        case "torrents_search":
          torrentFeed = sampleFeed(String(a.url ?? settings["torrents.source"]));
          return torrentFeed;
        case "torrents_set_state": {
          for (const item of torrentFeed?.items ?? []) {
            if (item.link === a.link) item.state = a.selected ? "selected" : "discardedNormal";
          }
          torrentFeed = torrentFeed && { ...torrentFeed, items: [...torrentFeed.items] };
          return torrentFeed;
        }
        case "torrents_download": {
          const links = (a.links as string[]).length
            ? (a.links as string[])
            : (torrentFeed?.items.filter((i) => i.state === "selected").map((i) => i.link) ?? []);
          const done = torrentFeed?.items.filter((i) => links.includes(i.link)) ?? [];
          for (const item of done) item.state = "discardedNormal";
          archived += done.length;
          torrentFeed = torrentFeed && { ...torrentFeed, items: [...torrentFeed.items] };
          if (torrentFeed) void emit("torrents", torrentFeed);
          return { downloaded: done.map((i) => i.title), errors: [] };
        }
        case "torrents_presets":
          return settings["torrents.filters"].map((filter) => ({ description: filter.name, isDefault: true, filter }));
        case "torrents_archive_info":
          return { count: archived };
        case "torrents_archive_clear":
          archived = 0;
          return null;
        case "update_status":
          return { configured: true, available: null };
        case "update_check":
          return null;
        case "taiga_detect":
          return {
            found: [{ version: "V2", path: TAIGA_PATH }],
            prompt: !taigaDone,
          };
        case "taiga_preview":
        case "taiga_import": {
          const summary = {
            version: "V2",
            path: String(a.path ?? TAIGA_PATH),
            service: "anilist",
            serviceName: "AniList",
            username: "demo",
            anime: catalog.size,
            entries: entries.size,
            history: 0,
            queue: 0,
            settings: 12,
            warnings: [],
          };
          if (cmd === "taiga_import") taigaDone = true;
          return summary;
        }
        case "taiga_dismiss":
          taigaDone = true;
          return null;
        case "plugin:dialog|confirm":
          return window.confirm(String(a.message ?? ""));
        // `confirm` with custom labels; answers with the chosen label.
        case "plugin:dialog|message": {
          const custom = (a.buttons as { OkCancelCustom?: [string, string] } | undefined)?.OkCancelCustom;
          const [ok, cancel] = custom ?? ["Ok", "Cancel"];
          return window.confirm(String(a.message ?? "")) ? ok : cancel;
        }
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
