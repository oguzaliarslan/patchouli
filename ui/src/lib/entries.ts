import { writeText } from "./clipboard";
import { openUrl } from "@tauri-apps/plugin-opener";
import { newEntry, type Anime, type ListEntry, type ListStatus, type ServiceId } from "../api";
import type { MenuEntry } from "../components/ContextMenu";
import { LIST_STATUSES, LIST_STATUS_LABELS, isEmptyDate, scoreOptions, today } from "./format";

/**
 * Taiga's `entryWithEpisodeWatched`: sets the episode and fills in the start
 * date, completion, status and rewatch count that go with it.
 */
export function withEpisodeWatched(
  anime: Anime | null | undefined,
  entry: ListEntry | null | undefined,
  animeId: number,
  episode: number,
): ListEntry {
  const inList = entry && entry.status !== "NotInList" && !entry.pendingDelete;
  const updated: ListEntry = inList ? { ...entry } : newEntry(animeId, "Watching");
  const total = anime?.episodeCount ?? 0;
  const isFinal = total > 0 && episode === total;
  updated.watchedEpisodes = episode;
  if (episode === 1 && isEmptyDate(updated.dateStarted)) updated.dateStarted = today();
  if (isFinal && isEmptyDate(updated.dateCompleted)) updated.dateCompleted = today();
  if (updated.rewatching && isFinal) {
    updated.rewatching = false;
    updated.rewatchedTimes += 1;
  }
  if (isFinal) updated.status = "Completed";
  else if (!updated.rewatching) updated.status = "Watching";
  return updated;
}

export function pageUrl(service: ServiceId | undefined, id: number): string | null {
  switch (service) {
    case "anilist":
      return `https://anilist.co/anime/${id}`;
    case "myanimelist":
      return `https://myanimelist.net/anime/${id}/`;
    default:
      return null;
  }
}

export interface MenuTarget {
  animeId: number;
  anime: Anime | null | undefined;
  entry: ListEntry | null | undefined;
  title: string;
}

export interface MenuDeps {
  service: ServiceId | undefined;
  scoreFormat: string | null;
  openAnime: (id: number, tab?: "details" | "entry" | "settings") => void;
  save: (entry: ListEntry) => void;
  remove: (animeId: number, title: string) => void;
  notify: (message: string) => void;
  play: (animeId: number, title: string, options: { episode?: number; random?: boolean }) => void;
  openFolder: (animeId: number) => void;
}

/** Taiga's `nextEpisodeNumber`: after the last watched, or 1 once completed. */
export function nextEpisode(anime: Anime | null | undefined, entry: ListEntry | null | undefined): number {
  const next = (entry?.watchedEpisodes ?? 0) + 1;
  const total = anime?.episodeCount ?? 0;
  return total > 0 && next > total ? 1 : next;
}

/** The right-click menu for an anime, as in Taiga's `MediaMenu`. */
export function animeMenu(target: MenuTarget, deps: MenuDeps): MenuEntry[] {
  const { animeId, anime, entry, title } = target;
  const inList = !!entry && entry.status !== "NotInList" && !entry.pendingDelete;
  const total = anime?.episodeCount ?? 0;
  const url = pageUrl(deps.service, animeId);
  const items: MenuEntry[] = [{ label: "Details", onSelect: () => deps.openAnime(animeId) }];

  if (inList && entry) {
    const watched = entry.watchedEpisodes;
    items.push(
      { label: "Edit…", onSelect: () => deps.openAnime(animeId, "entry") },
      "separator",
      {
        label: total > 0 ? `Watched episode ${watched + 1}/${total}` : `Watched episode ${watched + 1}`,
        disabled: total > 0 && watched >= total,
        onSelect: () => deps.save(withEpisodeWatched(anime, entry, animeId, watched + 1)),
      },
      {
        label: "Undo last episode",
        disabled: watched <= 0,
        onSelect: () => deps.save({ ...entry, watchedEpisodes: watched - 1 }),
      },
      {
        label: "Status",
        items: LIST_STATUSES.map((status) => ({
          label: LIST_STATUS_LABELS[status],
          checked: entry.status === status,
          onSelect: () => deps.save(withStatus(entry, status, total)),
        })),
      },
      {
        label: "Score",
        items: scoreOptions(deps.scoreFormat).map((o) => ({
          label: o.label,
          checked: entry.score === o.value,
          onSelect: () => deps.save({ ...entry, score: o.value }),
        })),
      },
    );
  } else {
    items.push({
      label: "Add to list",
      items: LIST_STATUSES.map((status) => ({
        label: LIST_STATUS_LABELS[status],
        onSelect: () => deps.save(withStatus(newEntry(animeId, status), status, total)),
      })),
    });
  }

  // Playing and folders use the library folders (Taiga's Play / Open folder).
  const next = nextEpisode(anime, entry);
  items.push(
    "separator",
    {
      label: total > 0 && next > total ? "Play" : `Play episode ${next}`,
      onSelect: () => deps.play(animeId, title, {}),
    },
    { label: "Play random episode", onSelect: () => deps.play(animeId, title, { random: true }) },
    { label: "Open folder", onSelect: () => deps.openFolder(animeId) },
    "separator",
  );
  if (url) {
    items.push({ label: "Open page in browser", onSelect: () => void openUrl(url) });
  }
  items.push({
    label: "Copy title",
    onSelect: () => void writeText(title).then(() => deps.notify("Title copied.")),
  });
  if (url) {
    items.push({
      label: "Copy link",
      onSelect: () => void writeText(url).then(() => deps.notify("Link copied.")),
    });
  }
  if (inList) {
    items.push("separator", {
      label: "Remove from list…",
      danger: true,
      onSelect: () => deps.remove(animeId, title),
    });
  }
  return items;
}

/** Changing the status to Completed also completes the episodes and date. */
export function withStatus(entry: ListEntry, status: ListStatus, total: number): ListEntry {
  const updated = { ...entry, status };
  if (status === "Completed") {
    if (total > 0) updated.watchedEpisodes = total;
    if (isEmptyDate(updated.dateCompleted)) updated.dateCompleted = today();
    updated.rewatching = false;
  }
  return updated;
}
