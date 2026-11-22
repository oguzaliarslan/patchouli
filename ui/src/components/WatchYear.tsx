import { useMemo, useState } from "react";
import type { Anime, HistoryView, ListItem } from "../api";
import { useApp } from "../lib/context";
import { parseFuzzy, preferredTitle } from "../lib/format";

/** Minutes for an episode whose length isn't known. */
const DEFAULT_MINUTES = 24;
/** Upper bounds (minutes) of heatmap levels 1–3; above is level 4. */
const LEVELS = [30, 60, 120];
const DAY_MS = 86_400_000;

const DAY_LABEL = new Intl.DateTimeFormat(undefined, { weekday: "short", month: "short", day: "numeric" });
const MONTH_SHORT = new Intl.DateTimeFormat(undefined, { month: "short" });

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

function minutesText(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  return h ? `${h} h${m ? ` ${m} min` : ""}` : `${m} min`;
}

export function level(minutes: number): number {
  if (minutes <= 0) return 0;
  const index = LEVELS.findIndex((max) => minutes <= max);
  return index === -1 ? 4 : index + 1;
}

interface Day {
  episodes: number;
  minutes: number;
}

export interface Recap {
  days: Map<string, Day>;
  episodes: number;
  minutes: number;
  anime: { id: number; episodes: number }[];
  genres: { name: string; episodes: number }[];
  longestStreak: number;
  busiest: { date: Date; day: Day } | null;
}

/** Totals for `year` from the watch history (local time). */
export function recap(history: HistoryView, year: number): Recap {
  const days = new Map<string, Day>();
  const perAnime = new Map<number, number>();
  const perGenre = new Map<string, number>();
  let episodes = 0;
  let minutes = 0;
  let busiest: Recap["busiest"] = null;
  for (const item of history.items) {
    const date = new Date(item.time * 1000);
    if (date.getFullYear() !== year) continue;
    const anime: Anime | undefined = history.anime[item.animeId];
    const length = anime?.episodeLength && anime.episodeLength > 0 ? anime.episodeLength : DEFAULT_MINUTES;
    const key = dayKey(date);
    const day = days.get(key) ?? { episodes: 0, minutes: 0 };
    day.episodes += 1;
    day.minutes += length;
    days.set(key, day);
    if (!busiest || day.minutes > busiest.day.minutes) busiest = { date, day };
    episodes += 1;
    minutes += length;
    perAnime.set(item.animeId, (perAnime.get(item.animeId) ?? 0) + 1);
    for (const genre of anime?.genres ?? []) perGenre.set(genre, (perGenre.get(genre) ?? 0) + 1);
  }

  let longestStreak = 0;
  let streak = 0;
  for (let d = new Date(year, 0, 1); d.getFullYear() === year; d = new Date(d.getTime() + DAY_MS)) {
    // Normalise across daylight-saving changes.
    d.setHours(12);
    streak = days.has(dayKey(d)) ? streak + 1 : 0;
    longestStreak = Math.max(longestStreak, streak);
  }

  return {
    days,
    episodes,
    minutes,
    anime: [...perAnime].map(([id, n]) => ({ id, episodes: n })).sort((a, b) => b.episodes - a.episodes),
    genres: [...perGenre].map(([name, n]) => ({ name, episodes: n })).sort((a, b) => b.episodes - a.episodes),
    longestStreak,
    busiest,
  };
}

/** One column per week (Monday first), one row per weekday. */
function Heatmap({ year, days }: { year: number; days: Map<string, Day> }) {
  const cells: { date: Date; day: Day | undefined; column: number; row: number }[] = [];
  const months: { label: string; column: number }[] = [];
  const first = new Date(year, 0, 1, 12);
  const offset = (first.getDay() + 6) % 7;
  for (let d = first; d.getFullYear() === year; d = new Date(d.getTime() + DAY_MS)) {
    d.setHours(12);
    const index = Math.round((d.getTime() - first.getTime()) / DAY_MS) + offset;
    const column = Math.floor(index / 7) + 1;
    if (d.getDate() === 1) months.push({ label: MONTH_SHORT.format(d), column });
    cells.push({ date: d, day: days.get(dayKey(d)), column, row: (index % 7) + 1 });
  }

  return (
    <figure className="heatmap" aria-label={`Time watched per day in ${year}`}>
      <div className="heatmap-scroll">
        <div className="heatmap-months" aria-hidden="true">
          {months.map((m) => (
            <span key={m.label} style={{ gridColumn: m.column }}>
              {m.label}
            </span>
          ))}
        </div>
        <div className="heatmap-grid" role="list">
          {cells.map(({ date, day, column, row }) => {
            const text = `${DAY_LABEL.format(date)}: ${
              day ? `${day.episodes} episode${day.episodes === 1 ? "" : "s"}, ${minutesText(day.minutes)}` : "nothing watched"
            }`;
            return (
              <span
                key={date.getTime()}
                role="listitem"
                className={`heat-cell level-${level(day?.minutes ?? 0)}`}
                style={{ gridColumn: column, gridRow: row }}
                title={text}
                aria-label={text}
              />
            );
          })}
        </div>
      </div>
      <figcaption className="heatmap-legend muted small" aria-hidden="true">
        Less
        {[0, 1, 2, 3, 4].map((l) => (
          <span key={l} className={`heat-cell level-${l}`} />
        ))}
        More
      </figcaption>
    </figure>
  );
}

/** Heatmap of time watched and a recap of the year, from the watch history. */
export function WatchYear({
  history,
  list,
  language,
}: {
  history: HistoryView;
  list: ListItem[];
  language: "romaji" | "english" | "native";
}) {
  const app = useApp();
  const years = useMemo(() => {
    const set = new Set(history.items.map((i) => new Date(i.time * 1000).getFullYear()));
    set.add(new Date().getFullYear());
    return [...set].sort((a, b) => b - a);
  }, [history]);
  const [year, setYear] = useState(years[0]);
  const data = useMemo(() => recap(history, year), [history, year]);
  const completed = list.filter((i) => i.entry.status === "Completed" && parseFuzzy(i.entry.dateCompleted).year === year);
  const title = (id: number) => preferredTitle(history.anime[id], language, history.displayTitles[id], id);
  const topEpisodes = data.anime[0]?.episodes ?? 1;

  return (
    <section className="watch-year">
      <div className="section-head">
        <h2 className="section-title">Your year in anime</h2>
        {years.length > 1 && (
          <select value={year} aria-label="Year" onChange={(e) => setYear(Number(e.target.value))}>
            {years.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </select>
        )}
      </div>

      <Heatmap year={year} days={data.days} />

      {data.episodes === 0 ? (
        <p className="muted small">Nothing in your watch history for {year}.</p>
      ) : (
        <>
          <div className="stat-tiles">
            <div className="stat-tile">
              <span className="stat-value">{data.episodes.toLocaleString()}</span>
              <span className="muted small">Episodes</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{minutesText(data.minutes)}</span>
              <span className="muted small">Time watched</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{data.anime.length}</span>
              <span className="muted small">Different anime</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{completed.length}</span>
              <span className="muted small">Completed</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">
                {data.longestStreak} day{data.longestStreak === 1 ? "" : "s"}
              </span>
              <span className="muted small">Longest streak</span>
            </div>
            {data.busiest && (
              <div className="stat-tile">
                <span className="stat-value">{minutesText(data.busiest.day.minutes)}</span>
                <span className="muted small">Busiest day · {DAY_LABEL.format(data.busiest.date)}</span>
              </div>
            )}
          </div>

          <div className="profile-grid">
            <section>
              <h3 className="section-title">Most watched</h3>
              {data.anime.slice(0, 5).map((a) => (
                <div key={a.id} className="share" title={`${title(a.id)}: ${a.episodes} episodes`}>
                  <button className="link share-label" onClick={() => app.openAnime(a.id)}>
                    {title(a.id)}
                  </button>
                  <span className="share-track">
                    <span style={{ width: `${(a.episodes / topEpisodes) * 100}%` }} />
                  </span>
                  <span className="share-value">{a.episodes}</span>
                </div>
              ))}
            </section>
            {data.genres.length > 0 && (
              <section>
                <h3 className="section-title">Genres, by episodes</h3>
                {data.genres.slice(0, 5).map((g) => (
                  <div key={g.name} className="share" title={`${g.name}: ${g.episodes} episodes`}>
                    <span className="share-label">{g.name}</span>
                    <span className="share-track">
                      <span style={{ width: `${(g.episodes / data.genres[0].episodes) * 100}%` }} />
                    </span>
                    <span className="share-value">{g.episodes}</span>
                  </div>
                ))}
              </section>
            )}
          </div>
        </>
      )}
    </section>
  );
}
