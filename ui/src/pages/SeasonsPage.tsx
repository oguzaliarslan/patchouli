import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo, useState } from "react";
import { api, errorMessage, type Anime, type AnimeType, type ListEntry, type ListStatus, type ServiceId } from "../api";
import { Icon } from "../components/Icon";
import { Poster } from "../components/Poster";
import { useApp } from "../lib/context";
import { pageUrl } from "../lib/entries";
import {
  AIRING_LABELS,
  LIST_STATUSES,
  LIST_STATUS_LABELS,
  SEASON_NAMES,
  TYPE_LABELS,
  capitalize,
  formatDate,
  isNsfw,
  plainText,
  preferredTitle,
  seasonOf,
  type SeasonName,
} from "../lib/format";
import { usePref } from "../lib/prefs";
import { useCurrentAccount, useList, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";

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

/** Months each season covers, as Taiga's status bar shows them. */
const SEASON_MONTHS: Record<SeasonName, string> = {
  winter: "January to March",
  spring: "April to June",
  summer: "July to September",
  fall: "October to December",
};

/** The first season either service has data for. */
const FIRST_YEAR = 1940;

type GroupBy = "type" | "airing" | "listStatus";
type Sort = "popularity" | "score" | "title" | "date" | "episodes";
type View = "details" | "images";

/** Type groups in Taiga's season window order. */
const TYPE_GROUPS: [string, AnimeType[]][] = [
  ["TV", ["Tv"]],
  ["ONA", ["Ona"]],
  ["Movies", ["Movie"]],
  ["OVA & specials", ["Ova", "Special"]],
  ["Other", ["Music", "Unknown"]],
];

const AIRING_ORDER: Anime["status"][] = ["Airing", "FinishedAiring", "NotYetAired", "Unknown"];

const GROUP_LABELS: Record<GroupBy, string> = {
  type: "Type",
  airing: "Airing status",
  listStatus: "List status",
};

const SORT_LABELS: Record<Sort, string> = {
  popularity: "Popularity",
  score: "Score",
  title: "Title",
  date: "Airing date",
  episodes: "Episodes",
};

/** Taiga v1's grouping: by type, airing status, or list status with a "Not in list" group. */
function groupAnime(
  items: Anime[],
  by: GroupBy,
  entries: Map<number, ListEntry>,
): (readonly [string, Anime[]])[] {
  let groups: [string, (a: Anime) => boolean][];
  switch (by) {
    case "type":
      groups = TYPE_GROUPS.map(([label, types]) => [label, (a) => types.includes(a.type)]);
      break;
    case "airing":
      groups = AIRING_ORDER.map((status) => [AIRING_LABELS[status], (a) => a.status === status]);
      break;
    case "listStatus": {
      const statusOf = (a: Anime): ListStatus | null => entries.get(a.id)?.status ?? null;
      groups = [
        ...LIST_STATUSES.map((s): [string, (a: Anime) => boolean] => [LIST_STATUS_LABELS[s], (a) => statusOf(a) === s]),
        ["Not in list", (a) => statusOf(a) === null],
      ];
      break;
    }
  }
  return groups.map(([label, test]) => [label, items.filter(test)] as const).filter(([, anime]) => anime.length > 0);
}

/** Taiga v1's filter: every word must be in a title, genre, tag, studio or producer. */
function matches(anime: Anime, words: string[]): boolean {
  if (words.length === 0) return true;
  const t = anime.titles;
  const haystack = [t.romaji, t.english, t.japanese, ...t.synonyms, ...anime.genres, ...anime.tags, ...anime.studios, ...anime.producers]
    .join("\n")
    .toLowerCase();
  return words.every((w) => haystack.includes(w));
}

function popularityText(anime: Anime, service: ServiceId | undefined): string {
  if (anime.popularityRank <= 0) return "?";
  // AniList counts users; MyAnimeList ranks.
  return service === "anilist"
    ? `${anime.popularityRank.toLocaleString()} users`
    : `#${anime.popularityRank.toLocaleString()}`;
}

function scoreText(anime: Anime): string {
  return anime.score > 0 ? anime.score.toFixed(2) : "?";
}

function episodeCountText(anime: Anime): string {
  if (anime.episodeCount <= 0) return "?";
  return anime.episodeCount === 1 ? "1 episode" : `${anime.episodeCount} episodes`;
}

function airedText(anime: Anime): string {
  const start = formatDate(anime.dateStarted);
  const end = anime.dateFinished !== anime.dateStarted ? formatDate(anime.dateFinished) : null;
  return end && end !== "—" ? `${start} to ${end}` : start;
}

export function SeasonsPage() {
  const app = useApp();
  const { connected } = useCurrentAccount();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const openMenu = useAnimeMenu();
  const [season, setSeasonState] = usePref<Season>("seasons.last", currentSeason());
  const [sort, setSort] = usePref<Sort>("seasons.sort", "popularity");
  const [groupBy, setGroupBy] = usePref<GroupBy>("seasons.groupBy", "type");
  const [view, setView] = usePref<View>("seasons.view", "details");
  const [hideListed, setHideListed] = usePref("seasons.hideListed", false);
  const [text, setText] = useState("");
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;
  const service = settings?.["sync.service"];

  const setSeason = (next: Season) => {
    setSeasonState(next);
    setText("");
  };

  const query = useQuery({
    queryKey: ["season", season.name, season.year],
    queryFn: () => api.season(season.name, season.year),
    enabled: !!connected,
    staleTime: 30 * 60 * 1000,
  });

  const entries = useMemo(
    () => new Map(list.filter((i) => !i.entry.pendingDelete).map((i) => [i.entry.animeId, i.entry])),
    [list],
  );
  const words = useMemo(() => text.toLowerCase().split(/\s+/).filter(Boolean), [text]);

  const groups = useMemo(() => {
    const items = (query.data ?? []).filter(
      (a) => !(hideNsfw && isNsfw(a)) && !(hideListed && entries.has(a.id)) && matches(a, words),
    );
    const compare: Record<Sort, ((a: Anime, b: Anime) => number) | null> = {
      popularity: null, // as the service returns them, most popular first
      score: (a, b) => b.score - a.score,
      title: (a, b) => preferredTitle(a, language).localeCompare(preferredTitle(b, language)),
      date: (a, b) => a.dateStarted.localeCompare(b.dateStarted),
      episodes: (a, b) => b.episodeCount - a.episodeCount,
    };
    const sorted = compare[sort] ? [...items].sort(compare[sort]!) : items;
    return groupAnime(sorted, groupBy, entries);
  }, [query.data, hideNsfw, hideListed, entries, sort, groupBy, language, words]);

  const label = `${capitalize(season.name)} ${season.year}`;
  const thisYear = new Date().getFullYear();
  const years = Array.from({ length: thisYear + 2 - FIRST_YEAR }, (_, i) => thisYear + 1 - i);
  const shown = groups.reduce((n, [, anime]) => n + anime.length, 0);

  const openPage = (id: number) => {
    const url = pageUrl(service, id);
    if (url) void openUrl(url);
  };

  return (
    <div className="page">
      <header className="page-header">
        <h1>Seasons</h1>
        <div className="season-nav">
          <button aria-label="Previous season" title="Previous season" onClick={() => setSeason(step(season, -1))}>
            ‹
          </button>
          <select
            aria-label="Season"
            value={season.name}
            onChange={(e) => setSeason({ ...season, name: e.target.value as SeasonName })}
          >
            {SEASON_NAMES.map((name) => (
              <option key={name} value={name}>
                {capitalize(name)}
              </option>
            ))}
          </select>
          <select
            aria-label="Year"
            value={season.year}
            onChange={(e) => setSeason({ ...season, year: Number(e.target.value) })}
          >
            {years.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </select>
          <button aria-label="Next season" title="Next season" onClick={() => setSeason(step(season, 1))}>
            ›
          </button>
          <button className="link" onClick={() => setSeason(currentSeason())}>
            Current
          </button>
        </div>
        <div className="page-actions">
          <button
            title="Download this season again"
            disabled={!connected || query.isFetching}
            onClick={() => void query.refetch()}
          >
            <Icon name="refresh" /> {query.isFetching && !query.isPending ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </header>

      <div className="toolbar">
        <input
          type="search"
          placeholder="Filter by title, genre, tag or studio"
          value={text}
          onChange={(e) => setText(e.target.value)}
          aria-label="Filter"
        />
        <label className="inline small">
          Group by
          <select aria-label="Group by" value={groupBy} onChange={(e) => setGroupBy(e.target.value as GroupBy)}>
            {(Object.keys(GROUP_LABELS) as GroupBy[]).map((g) => (
              <option key={g} value={g}>
                {GROUP_LABELS[g]}
              </option>
            ))}
          </select>
        </label>
        <label className="inline small">
          Sort by
          <select aria-label="Sort by" value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            {(Object.keys(SORT_LABELS) as Sort[]).map((s) => (
              <option key={s} value={s}>
                {SORT_LABELS[s]}
              </option>
            ))}
          </select>
        </label>
        <label className="inline small">
          <input type="checkbox" checked={hideListed} onChange={(e) => setHideListed(e.target.checked)} />
          Hide anime in my list
        </label>
        <div className="segmented" role="group" aria-label="View">
          <button aria-pressed={view === "details"} title="Details" onClick={() => setView("details")}>
            <Icon name="rows" />
          </button>
          <button aria-pressed={view === "images"} title="Images" onClick={() => setView("images")}>
            <Icon name="grid" />
          </button>
        </div>
      </div>

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
      ) : (query.data ?? []).length === 0 ? (
        <p className="empty">No anime found for {label}.</p>
      ) : groups.length === 0 ? (
        <p className="empty">{words.length > 0 ? `No anime found for “${text.trim()}”.` : "Every anime of this season is hidden."}</p>
      ) : (
        groups.map(([group, anime]) => (
          <section key={group}>
            <h2 className="section-title">
              {group} <span className="count">{anime.length}</span>
            </h2>
            <div className={view === "details" ? "season-tiles" : "result-grid"}>
              {anime.map((a) => {
                const title = preferredTitle(a, language);
                const entry = entries.get(a.id);
                const props: CardProps = {
                  anime: a,
                  title,
                  entry,
                  service,
                  sort,
                  onOpen: () => app.openAnime(a.id),
                  onPage: () => openPage(a.id),
                  onMenu: (e: React.MouseEvent) => openMenu(e, { animeId: a.id, anime: a, entry, title }),
                };
                return view === "details" ? <SeasonTile key={a.id} {...props} /> : <SeasonImage key={a.id} {...props} />;
              })}
            </div>
          </section>
        ))
      )}
      {query.data && query.data.length > 0 && (
        <p className="muted small">
          {label}, from {SEASON_MONTHS[season.name]}: {shown === query.data.length ? shown : `${shown} of ${query.data.length}`} anime, from your
          list service.
        </p>
      )}
    </div>
  );
}

interface CardProps {
  anime: Anime;
  title: string;
  entry: ListEntry | undefined;
  service: ServiceId | undefined;
  sort: Sort;
  onOpen: () => void;
  onPage: () => void;
  onMenu: (e: React.MouseEvent) => void;
}

/** Middle click opens the anime's page, as in Taiga's season window. */
function onMiddle(onPage: () => void) {
  return (e: React.MouseEvent) => {
    if (e.button !== 1) return;
    e.preventDefault();
    onPage();
  };
}

/** Taiga's "Details" view: poster, a title bar coloured by airing status, facts and the synopsis. */
function SeasonTile({ anime, title, entry, service, onOpen, onPage, onMenu }: CardProps) {
  const studios = anime.studios.length > 0 ? anime.studios : anime.producers;
  return (
    <button
      className="season-tile"
      onClick={onOpen}
      onAuxClick={onMiddle(onPage)}
      onMouseDown={(e) => e.button === 1 && e.preventDefault()}
      onContextMenu={onMenu}
    >
      <Poster anime={anime} title={title} />
      <span className="season-tile-body">
        <span className={`season-tile-title airing-${anime.status}`}>
          <span className="title-ellipsis" title={title}>
            {title}
          </span>
          {entry && <span className="badge accent">{LIST_STATUS_LABELS[entry.status]}</span>}
        </span>
        <dl className="season-facts">
          <dt>Aired</dt>
          <dd>
            {airedText(anime)} ({AIRING_LABELS[anime.status]})
          </dd>
          <dt>Type</dt>
          <dd>
            {TYPE_LABELS[anime.type]} · {episodeCountText(anime)}
            {anime.episodeLength > 0 && `, ${anime.episodeLength} min`}
          </dd>
          <dt>Genres</dt>
          <dd>{anime.genres.length > 0 ? anime.genres.join(", ") : "?"}</dd>
          <dt>Studios</dt>
          <dd>{studios.length > 0 ? studios.join(", ") : "?"}</dd>
          <dt>Score</dt>
          <dd>
            {scoreText(anime)} · {popularityText(anime, service)}
          </dd>
        </dl>
        {anime.synopsis && <span className="season-synopsis">{plainText(anime.synopsis)}</span>}
      </span>
    </button>
  );
}

/** What the "Images" view shows under each poster: the value it's sorted by, as in Taiga. */
function sortValue(anime: Anime, sort: Sort, service: ServiceId | undefined): string {
  switch (sort) {
    case "date":
      return formatDate(anime.dateStarted);
    case "episodes":
      return episodeCountText(anime);
    case "popularity":
      return popularityText(anime, service);
    case "score":
      return `Score ${scoreText(anime)}`;
    case "title":
      return `${TYPE_LABELS[anime.type]} · ${episodeCountText(anime)}`;
  }
}

/** Taiga's "Images" view: posters, with the details in a tooltip. */
function SeasonImage({ anime, title, entry, service, sort, onOpen, onPage, onMenu }: CardProps) {
  const studios = anime.studios.length > 0 ? anime.studios : anime.producers;
  const tooltip = [
    title,
    `${TYPE_LABELS[anime.type]} • ${episodeCountText(anime)} • ${scoreText(anime)} • ${popularityText(anime, service)}`,
    anime.genres.join(", "),
    studios.join(", "),
  ]
    .filter(Boolean)
    .join("\n");
  return (
    <button
      className="anime-card"
      title={tooltip}
      onClick={onOpen}
      onAuxClick={onMiddle(onPage)}
      onMouseDown={(e) => e.button === 1 && e.preventDefault()}
      onContextMenu={onMenu}
    >
      <Poster anime={anime} title={title} />
      <span className={`season-airing-bar airing-${anime.status}`} />
      <span className="anime-card-title title-clamp">{title}</span>
      <span className="muted small">{sortValue(anime, sort, service)}</span>
      {entry && <span className="badge accent">{LIST_STATUS_LABELS[entry.status]}</span>}
    </button>
  );
}
