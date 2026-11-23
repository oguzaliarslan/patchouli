import { NowPlayingCard } from "../components/NowPlaying";
import { Poster } from "../components/Poster";
import { Progress } from "../components/Progress";
import { SuggestionCard, airingText } from "../components/SuggestionCard";
import { availability, useAvailable } from "../lib/available";
import { useApp } from "../lib/context";
import { episodesText, isNsfw, itemTitle, relativeTime } from "../lib/format";
import { useAnimeMenu } from "../lib/useAnimeMenu";
import { useState } from "react";
import {
  useCurrentAccount,
  useList,
  useRecommendations,
  useSequels,
  useSettings,
  useTaigaDetection,
  type SyncState,
} from "../lib/queries";
import { dismissTaigaImport, importFromTaiga } from "../lib/taiga";

export function HomePage({ sync }: { sync: SyncState }) {
  const app = useApp();
  const { data: settings } = useSettings();
  const { data: items = [] } = useList();
  const { connected, account } = useCurrentAccount();
  const openMenu = useAnimeMenu();
  const { data: available } = useAvailable();
  const highlight = settings?.["app.list.highlightNewEpisodes"] ?? true;
  const { data: taiga } = useTaigaDetection();
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;
  const { data: allSequels = [] } = useSequels();
  const { data: allGroups = [] } = useRecommendations();
  const groups = allGroups
    .map((g) => ({ ...g, items: g.items.filter((a) => !(hideNsfw && isNsfw(a))) }))
    .filter((g) => g.items.length >= 3);
  // A different liked anime each day; "Another" moves on.
  const [offset, setOffset] = useState(() => Math.floor(Date.now() / 86_400_000));
  const group = groups.length ? groups[offset % groups.length] : null;
  // Only what's airing or coming up: the Sequels page has the rest.
  const sequels = allSequels.filter(
    (s) => (s.anime.status === "Airing" || s.anime.status === "NotYetAired") && !(hideNsfw && isNsfw(s.anime)),
  );

  const watching = items
    .filter((i) => i.entry.status === "Watching")
    .sort((a, b) => b.entry.lastUpdated - a.entry.lastUpdated)
    .slice(0, 6);

  return (
    <div className="page">
      <header className="page-header">
        <h1>Home</h1>
        <div className="page-actions">
          <button disabled={!connected || sync.busy} onClick={() => void sync.syncNow()}>
            {sync.busy ? "Syncing…" : "Sync now"}
          </button>
        </div>
      </header>

      <NowPlayingCard />

      {taiga?.prompt && (
        <section className="callout">
          <p>
            Taiga data was found. Import your list, history and settings from Taiga?{" "}
            <button className="link" onClick={() => void importFromTaiga(app, taiga.found[0].path)}>
              Import
            </button>{" "}
            ·{" "}
            <button className="link" onClick={() => void dismissTaigaImport()}>
              Not now
            </button>
          </p>
        </section>
      )}

      {!taiga?.prompt && !account?.username && (
        <section className="callout">
          <p>
            Log in to AniList or MyAnimeList to load your list.{" "}
            <button className="link" onClick={() => app.navigate("settings")}>
              Go to Settings › Accounts
            </button>
          </p>
        </section>
      )}

      <h2 className="section-title">Continue watching</h2>
      {watching.length === 0 ? (
        <p className="empty">Nothing in progress.</p>
      ) : (
        <div className="continue-grid">
          {watching.map((item) => {
            const title = itemTitle(item, language);
            return (
              <button
                key={item.entry.animeId}
                className="continue-card"
                onClick={() => app.openAnime(item.entry.animeId)}
                onContextMenu={(e) =>
                  openMenu(e, { animeId: item.entry.animeId, anime: item.anime, entry: item.entry, title })
                }
              >
                <Poster anime={item.anime} title={title} />
                <Progress item={item} availability={availability(item, available)} flat />
                <span className="continue-text">
                  <span className="anime-card-title title-clamp">{title}</span>
                  <span className="muted small">
                    Episode {episodesText(item.entry.watchedEpisodes, item.anime?.episodeCount)} ·{" "}
                    {relativeTime(item.entry.lastUpdated)}
                  </span>
                  {highlight && availability(item, available).next != null && (
                    <span className="small on-disk">Episode {availability(item, available).next} is on disk</span>
                  )}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {group && (
        <>
          <div className="section-head">
            <h2 className="section-title">
              Because you liked{" "}
              <button className="link" onClick={() => app.openAnime(group.becauseId)}>
                {group.becauseTitle}
              </button>
            </h2>
            {groups.length > 1 && (
              <button className="link" onClick={() => setOffset((o) => o + 1)}>
                Another
              </button>
            )}
          </div>
          <div className="suggest-grid">
            {group.items.slice(0, 6).map((anime) => (
              <SuggestionCard key={anime.id} anime={anime} detail={airingText(anime)} />
            ))}
          </div>
        </>
      )}

      {sequels.length > 0 && (
        <>
          <div className="section-head">
            <h2 className="section-title">New seasons of what you finished</h2>
            <button className="link" onClick={() => app.navigate("sequels")}>
              All sequels
            </button>
          </div>
          <div className="suggest-grid">
            {sequels.slice(0, 6).map((s) => (
              <SuggestionCard
                key={s.anime.id}
                anime={s.anime}
                detail={`${airingText(s.anime)} · after ${s.becauseTitle}`}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
