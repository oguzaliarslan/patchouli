import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { api, errorMessage, type Anime, type AnimeType } from "../api";
import { useApp } from "../lib/context";
import { SEASON_NAMES, capitalize, isNsfw, preferredTitle, seasonOf, type SeasonName } from "../lib/format";
import { usePref } from "../lib/prefs";
import { useCurrentAccount, useList, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";
import { ResultCard } from "./SearchPage";

interface Season {
  name: SeasonName;
  year: number;
}

function currentSeason(): Season {
  const now = new Date();
  const date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
  return seasonOf(date) ?? { name: "winter", year: now.getFullYear() };
}

function step(season: Season, by: number): Season {
  const index = SEASON_NAMES.indexOf(season.name) + by;
  const year = season.year + Math.floor(index / 4);
  return { name: SEASON_NAMES[((index % 4) + 4) % 4], year };
}

/** Groups in Taiga's season window order. */
const GROUPS: [string, AnimeType[]][] = [
  ["TV", ["Tv"]],
  ["ONA", ["Ona"]],
  ["Movies", ["Movie"]],
  ["OVA & specials", ["Ova", "Special"]],
  ["Other", ["Music", "Unknown"]],
];

type Sort = "popularity" | "score" | "title" | "date";

export function SeasonsPage() {
  const app = useApp();
  const { connected } = useCurrentAccount();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const openMenu = useAnimeMenu();
  const [season, setSeason] = useState<Season>(currentSeason);
  const [sort, setSort] = usePref<Sort>("seasons.sort", "popularity");
  const [hideListed, setHideListed] = usePref("seasons.hideListed", false);
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;

  const query = useQuery({
    queryKey: ["season", season.name, season.year],
    queryFn: () => api.season(season.name, season.year),
    enabled: !!connected,
    staleTime: 30 * 60 * 1000,
  });

  const entries = useMemo(() => new Map(list.map((i) => [i.entry.animeId, i.entry])), [list]);
  const groups = useMemo(() => {
    const items = (query.data ?? []).filter(
      (a) => !(hideNsfw && isNsfw(a)) && !(hideListed && entries.has(a.id)),
    );
    const compare: Record<Sort, ((a: Anime, b: Anime) => number) | null> = {
      popularity: null, // as the service returns them
      score: (a, b) => b.score - a.score,
      title: (a, b) => preferredTitle(a, language).localeCompare(preferredTitle(b, language)),
      date: (a, b) => a.dateStarted.localeCompare(b.dateStarted),
    };
    const sorted = compare[sort] ? [...items].sort(compare[sort]!) : items;
    return GROUPS.map(([label, types]) => [label, sorted.filter((a) => types.includes(a.type))] as const).filter(
      ([, anime]) => anime.length > 0,
    );
  }, [query.data, hideNsfw, hideListed, entries, sort, language]);

  const label = `${capitalize(season.name)} ${season.year}`;

  return (
    <div className="page">
      <header className="page-header">
        <h1>Seasons</h1>
        <div className="season-nav">
          <button aria-label="Previous season" onClick={() => setSeason(step(season, -1))}>
            ‹
          </button>
          <strong>{label}</strong>
          <button aria-label="Next season" onClick={() => setSeason(step(season, 1))}>
            ›
          </button>
          <button className="link" onClick={() => setSeason(currentSeason())}>
            Current
          </button>
        </div>
        <div className="page-actions">
          <label className="inline small">
            <input type="checkbox" checked={hideListed} onChange={(e) => setHideListed(e.target.checked)} />
            Hide anime in my list
          </label>
          <select aria-label="Sort" value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            <option value="popularity">By popularity</option>
            <option value="score">By score</option>
            <option value="title">By title</option>
            <option value="date">By start date</option>
          </select>
        </div>
      </header>

      {!connected ? (
        <p className="empty">
          Seasons come from your list service.{" "}
          <button className="link" onClick={() => app.navigate("settings")}>
            Log in under Settings › Accounts
          </button>
        </p>
      ) : query.isPending ? (
        <p className="empty">Loading {label}…</p>
      ) : query.isError ? (
        <p className="error">{errorMessage(query.error)}</p>
      ) : groups.length === 0 ? (
        <p className="empty">No anime for {label}.</p>
      ) : (
        groups.map(([group, anime]) => (
          <section key={group}>
            <h2 className="section-title">
              {group} <span className="count">{anime.length}</span>
            </h2>
            <div className="result-grid">
              {anime.map((a) => {
                const title = preferredTitle(a, language);
                return (
                  <ResultCard
                    key={a.id}
                    anime={a}
                    title={title}
                    status={entries.get(a.id)?.status}
                    onOpen={() => app.openAnime(a.id)}
                    onMenu={(e) => openMenu(e, { animeId: a.id, anime: a, entry: entries.get(a.id), title })}
                  />
                );
              })}
            </div>
          </section>
        ))
      )}
      {query.data && query.data.length > 0 && (
        <p className="muted small">{query.data.length} anime starting in {label}, from your list service.</p>
      )}
    </div>
  );
}
