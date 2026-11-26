import type { Anime, ListEntry, ListItem, ListStatus, Settings } from "../api";

export const LIST_STATUSES: ListStatus[] = [
  "Watching",
  "Completed",
  "OnHold",
  "Dropped",
  "PlanToWatch",
];

export const LIST_STATUS_LABELS: Record<ListStatus, string> = {
  NotInList: "Not in list",
  Watching: "Watching",
  Completed: "Completed",
  OnHold: "On hold",
  Dropped: "Dropped",
  PlanToWatch: "Plan to watch",
};

/**
 * The list tab an entry belongs in. A rewatch in progress counts as watching:
 * MyAnimeList keeps it as Completed with `is_rewatching` set.
 */
export function listStatus(entry: ListEntry): ListStatus {
  return entry.rewatching && entry.status === "Completed" ? "Watching" : entry.status;
}

/** Being watched now, for the first time or again. */
export function isWatching(entry: ListEntry): boolean {
  return listStatus(entry) === "Watching";
}

export const TYPE_LABELS: Record<string, string> = {
  Unknown: "Unknown",
  Tv: "TV",
  Ova: "OVA",
  Movie: "Movie",
  Special: "Special",
  Ona: "ONA",
  Music: "Music",
};

export const AIRING_LABELS: Record<string, string> = {
  Unknown: "Unknown",
  FinishedAiring: "Finished airing",
  Airing: "Airing",
  NotYetAired: "Not yet aired",
};

export const SEASON_NAMES = ["winter", "spring", "summer", "fall"] as const;
export type SeasonName = (typeof SEASON_NAMES)[number];

export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

type TitleLanguage = Settings["app.titleLanguage"];

/** Uses a custom title, then the chosen language, then romaji. */
export function preferredTitle(
  anime: Anime | null | undefined,
  language: TitleLanguage,
  displayTitle?: string | null,
  fallbackId?: number,
): string {
  if (displayTitle) return displayTitle;
  if (!anime) return fallbackId ? `#${fallbackId}` : "Unknown";
  const t = anime.titles;
  if (language === "english" && t.english) return t.english;
  if (language === "native" && t.japanese) return t.japanese;
  return t.romaji || t.english || `#${anime.id}`;
}

export function itemTitle(item: ListItem, language: TitleLanguage): string {
  return preferredTitle(item.anime, language, item.displayTitle, item.entry.animeId);
}

/** Checks whether an anime is marked as adult content. */
export function isNsfw(anime: Anime | null | undefined): boolean {
  if (!anime) return false;
  if (anime.ageRating === "R18") return true;
  return anime.ageRating === "Unknown" && anime.genres.includes("Hentai");
}

// --- Dates -----------------------------------------------------------------

export interface FuzzyParts {
  year: number;
  month: number;
  day: number;
}

export function parseFuzzy(value: string): FuzzyParts {
  const [y, m, d] = value.split("-").map((p) => Number.parseInt(p, 10) || 0);
  return { year: y ?? 0, month: m ?? 0, day: d ?? 0 };
}

export function isEmptyDate(value: string): boolean {
  const { year, month, day } = parseFuzzy(value);
  return !year && !month && !day;
}

export const EMPTY_DATE = "0000-00-00";

export function toFuzzy(parts: FuzzyParts): string {
  const pad = (n: number, w: number) => String(n).padStart(w, "0");
  return `${pad(parts.year, 4)}-${pad(parts.month, 2)}-${pad(parts.day, 2)}`;
}

export function today(): string {
  const d = new Date();
  return toFuzzy({ year: d.getFullYear(), month: d.getMonth() + 1, day: d.getDate() });
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `2024-04-07` → "Apr 7, 2024"; partial dates show what's known. */
export function formatDate(value: string): string {
  const { year, month, day } = parseFuzzy(value);
  if (!year) return "—";
  if (!month) return String(year);
  if (!day) return `${MONTHS[month - 1]} ${year}`;
  return `${MONTHS[month - 1]} ${day}, ${year}`;
}

/** Season of a start date; December belongs to the next year's winter. */
export function seasonOf(value: string): { name: SeasonName; year: number } | null {
  const { year, month } = parseFuzzy(value);
  if (!year || !month) return null;
  const index = Math.floor((month % 12) / 3);
  return { name: SEASON_NAMES[index], year: month === 12 ? year + 1 : year };
}

export function formatSeason(value: string): string {
  const season = seasonOf(value);
  return season ? `${capitalize(season.name)} ${season.year}` : "—";
}

export function formatTime(seconds: number): string {
  if (!seconds) return "—";
  return new Date(seconds * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function relativeTime(seconds: number, now = Date.now()): string {
  if (!seconds) return "—";
  const diff = Math.round((now / 1000 - seconds) / 60);
  if (diff < 1) return "just now";
  if (diff < 60) return `${diff} min ago`;
  const hours = Math.round(diff / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

// --- Scores ----------------------------------------------------------------

export type ScoreFormat = "POINT_100" | "POINT_10_DECIMAL" | "POINT_10" | "POINT_5" | "POINT_3";

export interface ScoreOption {
  value: number;
  label: string;
}

const range = (from: number, to: number, step: number) =>
  Array.from({ length: Math.floor((to - from) / step) + 1 }, (_, i) => from + i * step);

/** Choices for a score on a 0–100 scale. */
export function scoreOptions(format: string | null | undefined): ScoreOption[] {
  switch (format as ScoreFormat) {
    case "POINT_100":
      return [0, ...range(10, 100, 5)].map((v) => ({ value: v, label: v ? String(v) : "—" }));
    case "POINT_10_DECIMAL":
      return [0, ...range(10, 100, 5)].map((v) => ({
        value: v,
        label: v ? (v / 10).toFixed(v === 100 ? 0 : 1) : "—",
      }));
    case "POINT_5":
      return [0, 10, 30, 50, 70, 90].map((v, i) => ({
        value: v,
        label: v ? "★".repeat(i) + "☆".repeat(5 - i) : "—",
      }));
    case "POINT_3":
      return [
        { value: 0, label: "—" },
        { value: 35, label: ":(" },
        { value: 60, label: ":|" },
        { value: 85, label: ":)" },
      ];
    default:
      return range(0, 100, 10).map((v) => ({ value: v, label: v ? String(v / 10) : "—" }));
  }
}

/** Formats a stored score using the account's rating scale. */
export function formatScore(value: number, format: string | null | undefined): string {
  if (!value) return "—";
  switch (format as ScoreFormat) {
    case "POINT_100":
      return String(value);
    case "POINT_10_DECIMAL":
      return (value / 10).toFixed(1);
    case "POINT_5": {
      const stars = value < 30 ? 1 : value < 50 ? 2 : value < 70 ? 3 : value < 90 ? 4 : 5;
      return "★".repeat(stars) + "☆".repeat(5 - stars);
    }
    case "POINT_3":
      return value < 36 ? ":(" : value < 61 ? ":|" : ":)";
    default:
      return String(Math.floor(value / 10));
  }
}

/** The closest choice to `value`, so odd scores still select something. */
export function nearestOption(options: ScoreOption[], value: number): number {
  return options.reduce((best, o) =>
    Math.abs(o.value - value) < Math.abs(best.value - value) ? o : best,
  ).value;
}

// --- Text ------------------------------------------------------------------

/** AniList descriptions contain HTML; keep the text with line breaks. */
export function plainText(html: string): string {
  if (!html) return "";
  const withBreaks = html.replace(/<br\s*\/?>/gi, "\n");
  const doc = new DOMParser().parseFromString(withBreaks, "text/html");
  return (doc.body.textContent ?? "").replace(/\n{3,}/g, "\n\n").trim();
}

export function progressRatio(item: ListItem): number {
  const total = item.anime?.episodeCount ?? 0;
  if (!total) return item.entry.watchedEpisodes ? 0.8 : 0;
  return Math.min(item.entry.watchedEpisodes / total, 1);
}

export function episodesText(watched: number, total: number | undefined): string {
  return `${watched}/${total && total > 0 ? total : "?"}`;
}
