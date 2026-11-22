import { useEffect, useState } from "react";
import type { ListItem } from "../api";

/** Unix seconds, updated every `intervalMs`. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

export interface NextAiring {
  episode: number;
  /** Unix seconds. In the past until the service reports the next one. */
  time: number;
}

/** The next episode and when it airs, as the service last reported it. */
export function nextAiring(item: ListItem): NextAiring | null {
  const anime = item.anime;
  if (!anime?.nextEpisodeTime) return null;
  return { episode: anime.lastAiredEpisode + 1, time: anime.nextEpisodeTime };
}

/** Entries worth showing airing times for. */
export function followsAiring(item: ListItem): boolean {
  const status = item.entry.status;
  return status === "Watching" || status === "PlanToWatch" || status === "OnHold";
}

/** "35 min", "5h 20m", "2d 4h", or "aired". */
export function countdown(time: number, now: number): string {
  const minutes = Math.ceil((time - now) / 60);
  if (minutes <= 0) return "aired";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/** Short badge text: "Ep 7 in 2d 4h" or "Ep 7 aired". */
export function airingBadge(next: NextAiring, now: number): string {
  const left = countdown(next.time, now);
  return left === "aired" ? `Ep ${next.episode} aired` : `Ep ${next.episode} in ${left}`;
}
