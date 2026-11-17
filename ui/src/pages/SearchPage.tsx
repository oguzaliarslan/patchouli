import { useMutation } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { api, errorMessage, type Anime, type AnimeType, type AiringStatus, type SearchParams } from "../api";
import { Poster } from "../components/Poster";
import { useApp } from "../lib/context";
import {
  AIRING_LABELS,
  LIST_STATUS_LABELS,
  SEASON_NAMES,
  TYPE_LABELS,
  capitalize,
  formatSeason,
  isNsfw,
  preferredTitle,
} from "../lib/format";
import { useCurrentAccount, useList, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";

const CURRENT_YEAR = new Date().getFullYear();

export function SearchPage() {
  const app = useApp();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const { connected } = useCurrentAccount();
  const openMenu = useAnimeMenu();
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;

  const [params, setParams] = useState<SearchParams>({ text: "" });
  const set = (patch: Partial<SearchParams>) => setParams((p) => ({ ...p, ...patch }));
  const search = useMutation({ mutationFn: api.search });

  const entries = useMemo(() => new Map(list.map((i) => [i.entry.animeId, i])), [list]);
  const results = (search.data ?? []).filter((a) => !(hideNsfw && isNsfw(a)));
  const browsingSeason = !params.text && params.season && params.year;
  const canSearch = !!connected && (!!params.text?.trim() || !!browsingSeason);

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (canSearch) search.mutate({ ...params, text: params.text?.trim() });
  }

  return (
    <div className="page">
      <header className="page-header">
        <h1>Search</h1>
      </header>

      <form className="toolbar search-form" onSubmit={submit}>
        <input
          type="search"
          placeholder="Search for anime"
          value={params.text ?? ""}
          onChange={(e) => set({ text: e.target.value })}
          aria-label="Title"
          autoFocus
        />
        <select
          aria-label="Season"
          value={params.season ?? ""}
          onChange={(e) => set({ season: (e.target.value || null) as SearchParams["season"] })}
        >
          <option value="">Any season</option>
          {SEASON_NAMES.map((s) => (
            <option key={s} value={s}>
              {capitalize(s)}
            </option>
          ))}
        </select>
        <select
          aria-label="Year"
          value={params.year ?? ""}
          onChange={(e) => set({ year: e.target.value ? Number(e.target.value) : null })}
        >
          <option value="">Any year</option>
          {Array.from({ length: CURRENT_YEAR + 1 - 1960 + 1 }, (_, i) => CURRENT_YEAR + 1 - i).map((y) => (
            <option key={y} value={y}>
              {y}
            </option>
          ))}
        </select>
        <select
          aria-label="Type"
          value={params.type ?? ""}
          onChange={(e) => set({ type: (e.target.value || null) as AnimeType | null })}
        >
          <option value="">Any type</option>
          {Object.entries(TYPE_LABELS)
            .filter(([k]) => k !== "Unknown")
            .map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
        </select>
        <select
          aria-label="Airing status"
          value={params.status ?? ""}
          onChange={(e) => set({ status: (e.target.value || null) as AiringStatus | null })}
        >
          <option value="">Any status</option>
          {Object.entries(AIRING_LABELS)
            .filter(([k]) => k !== "Unknown")
            .map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
        </select>
        <button type="submit" className="primary" disabled={!canSearch || search.isPending}>
          {search.isPending ? "Searching…" : browsingSeason ? "Browse season" : "Search"}
        </button>
      </form>

      {!connected ? (
        <p className="empty">
          Searching uses your list service.{" "}
          <button className="link" onClick={() => app.navigate("settings")}>
            Log in under Settings › Accounts
          </button>
        </p>
      ) : search.isError ? (
        <p className="error">{errorMessage(search.error)}</p>
      ) : !search.data ? (
        <p className="empty">Search by title, or pick a season and year to browse it.</p>
      ) : results.length === 0 ? (
        <p className="empty">No results.</p>
      ) : (
        <div className="result-grid">
          {results.map((anime) => (
            <ResultCard
              key={anime.id}
              anime={anime}
              title={preferredTitle(anime, language)}
              status={entries.get(anime.id)?.entry.status}
              onOpen={() => app.openAnime(anime.id)}
              onMenu={(e) =>
                openMenu(e, {
                  animeId: anime.id,
                  anime,
                  entry: entries.get(anime.id)?.entry,
                  title: preferredTitle(anime, language),
                })
              }
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function ResultCard({
  anime,
  title,
  status,
  onOpen,
  onMenu,
}: {
  anime: Anime;
  title: string;
  status: keyof typeof LIST_STATUS_LABELS | undefined;
  onOpen: () => void;
  onMenu: (e: React.MouseEvent) => void;
}) {
  return (
    <button className="anime-card" onClick={onOpen} onContextMenu={onMenu}>
      <Poster anime={anime} title={title} />
      <span className="anime-card-title title-clamp">{title}</span>
      <span className="muted small">
        {TYPE_LABELS[anime.type]} · {formatSeason(anime.dateStarted)}
        {anime.episodeCount > 0 && ` · ${anime.episodeCount} ep`}
      </span>
      {status && <span className="badge accent">{LIST_STATUS_LABELS[status]}</span>}
    </button>
  );
}
