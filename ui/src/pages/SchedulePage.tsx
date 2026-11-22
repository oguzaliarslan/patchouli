import { useMemo } from "react";
import type { ListItem } from "../api";
import { Poster } from "../components/Poster";
import { countdown, followsAiring, nextAiring, useNow, type NextAiring } from "../lib/airing";
import { useApp } from "../lib/context";
import { isNsfw, itemTitle } from "../lib/format";
import { usePref } from "../lib/prefs";
import { useList, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";

type Scope = "watching" | "list";

interface Row {
  item: ListItem;
  next: NextAiring;
}

function dayLabel(seconds: number, now: number): string {
  const date = new Date(seconds * 1000);
  const today = new Date(now * 1000);
  const tomorrow = new Date(today);
  tomorrow.setDate(today.getDate() + 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === tomorrow.toDateString()) return "Tomorrow";
  return date.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
}

export function SchedulePage() {
  const app = useApp();
  const now = useNow();
  const { data: items = [], isLoading } = useList();
  const { data: settings } = useSettings();
  const openMenu = useAnimeMenu();
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;
  const [scope, setScope] = usePref<Scope>("schedule.scope", "watching");

  const groups = useMemo(() => {
    const rows: Row[] = [];
    for (const item of items) {
      if (scope === "watching" ? item.entry.status !== "Watching" : !followsAiring(item)) continue;
      if (hideNsfw && isNsfw(item.anime)) continue;
      const next = nextAiring(item);
      if (next) rows.push({ item, next });
    }
    rows.sort((a, b) => a.next.time - b.next.time);
    const result: { key: string; label: string; rows: Row[] }[] = [];
    for (const row of rows) {
      // Aired, but the service hasn't reported the next episode yet: today.
      const time = Math.max(row.next.time, now);
      const key = new Date(time * 1000).toDateString();
      const last = result.at(-1);
      if (last?.key === key) last.rows.push(row);
      else result.push({ key, label: dayLabel(time, now), rows: [row] });
    }
    return result;
  }, [items, scope, hideNsfw, now]);

  return (
    <div className="page">
      <header className="page-header">
        <h1>Schedule</h1>
        <div className="page-actions">
          <div className="segmented" role="group" aria-label="Show">
            <button aria-pressed={scope === "watching"} onClick={() => setScope("watching")}>
              Watching
            </button>
            <button aria-pressed={scope === "list"} onClick={() => setScope("list")}>
              Also planned and on hold
            </button>
          </div>
        </div>
      </header>

      {isLoading ? (
        <p className="empty">Loading…</p>
      ) : groups.length === 0 ? (
        <p className="empty">
          Nothing here has an upcoming episode. Airing times come from your service and update when you sync.
        </p>
      ) : (
        <div className="history">
          {groups.map((group) => (
            <section key={group.key}>
              <h2 className="section-title">{group.label}</h2>
              <ul className="history-list schedule-list">
                {group.rows.map(({ item, next }) => {
                  const id = item.entry.animeId;
                  const title = itemTitle(item, language);
                  const total = item.anime?.episodeCount ?? 0;
                  const behind = next.episode - 1 - item.entry.watchedEpisodes;
                  const left = countdown(next.time, now);
                  const at = new Date(next.time * 1000);
                  return (
                    <li
                      key={id}
                      onDoubleClick={() => app.openAnime(id)}
                      onContextMenu={(e) => openMenu(e, { animeId: id, anime: item.anime, entry: item.entry, title })}
                    >
                      <span className="history-time muted small" title={at.toLocaleString()}>
                        {at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
                      </span>
                      <Poster anime={item.anime} title={title} className="poster tiny" />
                      <span className="schedule-title">
                        <button className="link history-title" onClick={() => app.openAnime(id)}>
                          {title}
                        </button>
                        <span className="muted small">
                          Episode {next.episode}
                          {total > 0 && ` of ${total}`}
                          {behind > 0 && ` · ${behind} aired episode${behind === 1 ? "" : "s"} to catch up on`}
                        </span>
                      </span>
                      <span className={left === "aired" ? "badge ok" : "badge"}>
                        {left === "aired" ? "Aired" : `in ${left}`}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
