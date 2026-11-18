import { useQuery } from "@tanstack/react-query";
import { api, type ListItem } from "../api";
import { keys } from "./queries";

/** Episodes in the library folders, per anime ID. Kept current by events. */
export const useAvailable = () =>
  useQuery({ queryKey: keys.available, queryFn: api.libraryAvailable });

export interface Availability {
  /** The next episode to watch, if it's in the library. */
  next: number | null;
  /** The highest episode in the library (within the episode count). */
  max: number;
}

/** What the library has for a list entry (Taiga v1's "available episodes"). */
export function availability(item: ListItem, available: Record<number, number[]> | undefined): Availability {
  const episodes = available?.[item.entry.animeId];
  if (!episodes?.length) return { next: null, max: 0 };
  const total = item.anime?.episodeCount ?? 0;
  const inRange = total > 0 ? episodes.filter((e) => e <= total) : episodes;
  const next = item.entry.watchedEpisodes + 1;
  const finished = item.entry.status === "Completed" && !item.entry.rewatching;
  return {
    next: !finished && inRange.includes(next) ? next : null,
    max: inRange.length ? Math.max(...inRange) : 0,
  };
}
