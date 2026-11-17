import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { api, errorMessage } from "../api";
import { Icon } from "../components/Icon";
import { useApp } from "../lib/context";
import { preferredTitle } from "../lib/format";
import { useList, useSettings } from "../lib/queries";
import { playEpisode } from "../lib/useAnimeMenu";

/** Compact list of episode numbers: `1–3, 5, 7–8`. */
function formatEpisodes(numbers: number[]): string {
  const sorted = [...new Set(numbers)].sort((a, b) => a - b);
  const parts: string[] = [];
  for (let i = 0; i < sorted.length; i++) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    parts.push(i === j ? String(sorted[i]) : `${sorted[i]}–${sorted[j]}`);
    i = j;
  }
  return parts.join(", ");
}

export function LibraryPage() {
  const app = useApp();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const [showUnrecognized, setShowUnrecognized] = useState(false);
  const scan = useMutation({ mutationFn: api.scanLibrary });
  const folders = settings?.["library.folders"] ?? [];
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const entries = new Map(list.map((i) => [i.entry.animeId, i]));

  return (
    <div className="page">
      <header className="page-header">
        <h1>Library</h1>
        <div className="page-actions">
          <button className="primary" disabled={!folders.length || scan.isPending} onClick={() => scan.mutate()}>
            <Icon name="refresh" size={16} /> {scan.isPending ? "Scanning…" : scan.data ? "Scan again" : "Scan folders"}
          </button>
        </div>
      </header>

      {folders.length === 0 ? (
        <p className="empty">
          No library folders yet.{" "}
          <button className="link" onClick={() => app.navigate("settings")}>
            Add them in Settings › Library
          </button>
        </p>
      ) : !scan.data && !scan.isError ? (
        <p className="empty">
          Scans {folders.length === 1 ? folders[0] : `${folders.length} folders`} and matches video
          files to anime in your list.
        </p>
      ) : scan.isError ? (
        <p className="error">{errorMessage(scan.error)}</p>
      ) : (
        scan.data && (
          <>
            <p className="muted">
              {scan.data.scan.filesScanned} video files · {scan.data.scan.anime.length} anime ·{" "}
              {scan.data.scan.unrecognized.length} unrecognized
            </p>
            <ul className="library-list">
              {scan.data.scan.anime.map((group) => {
                const anime = scan.data!.anime[group.animeId];
                const item = entries.get(group.animeId);
                const title = preferredTitle(anime, language, item?.displayTitle, group.animeId);
                const numbers = group.files.flatMap((f) => f.episodes);
                const watched = item?.entry.watchedEpisodes ?? 0;
                const next = numbers.filter((n) => n > watched).sort((a, b) => a - b)[0];
                return (
                  <li key={group.animeId}>
                    <button className="link library-title" onClick={() => app.openAnime(group.animeId)}>
                      {title}
                    </button>
                    <span className="muted small">
                      {numbers.length
                        ? `${new Set(numbers).size === 1 ? "Episode" : "Episodes"} ${formatEpisodes(numbers)}`
                        : `${group.files.length} file${group.files.length === 1 ? "" : "s"}`}
                      {anime?.episodeCount ? ` of ${anime.episodeCount}` : ""}
                    </span>
                    <span className="actions">
                      {next !== undefined && (
                        <button onClick={() => void playEpisode(app, group.animeId, title, { episode: next })}>
                          <Icon name="play" size={14} /> {next}
                        </button>
                      )}
                    </span>
                  </li>
                );
              })}
            </ul>
            {scan.data.scan.unrecognized.length > 0 && (
              <section>
                <button className="link" onClick={() => setShowUnrecognized(!showUnrecognized)}>
                  {showUnrecognized ? "Hide" : "Show"} unrecognized files
                </button>
                {showUnrecognized && (
                  <ul className="unrecognized muted small selectable">
                    {scan.data.scan.unrecognized.map((path) => (
                      <li key={path}>{path}</li>
                    ))}
                  </ul>
                )}
              </section>
            )}
          </>
        )
      )}
    </div>
  );
}
