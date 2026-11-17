import { NowPlayingCard } from "../components/NowPlaying";
import { Poster } from "../components/Poster";
import { useApp } from "../lib/context";
import { episodesText, itemTitle, progressRatio, relativeTime } from "../lib/format";
import { useAnimeMenu } from "../lib/useAnimeMenu";
import { useCurrentAccount, useList, useSettings, useTaigaDetection, type SyncState } from "../lib/queries";
import { dismissTaigaImport, importFromTaiga } from "../lib/taiga";

export function HomePage({ sync }: { sync: SyncState }) {
  const app = useApp();
  const { data: settings } = useSettings();
  const { data: items = [] } = useList();
  const { connected, account } = useCurrentAccount();
  const openMenu = useAnimeMenu();
  const { data: taiga } = useTaigaDetection();
  const language = settings?.["app.titleLanguage"] ?? "romaji";

  const watching = items
    .filter((i) => i.entry.status === "Watching")
    .sort((a, b) => b.entry.lastUpdated - a.entry.lastUpdated)
    .slice(0, 12);

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
                <Poster anime={item.anime} title={title} className="poster small" />
                <span className="continue-text">
                  <span className="title-clamp">{title}</span>
                  <span className="muted small">
                    Episode {episodesText(item.entry.watchedEpisodes, item.anime?.episodeCount)} ·{" "}
                    {relativeTime(item.entry.lastUpdated)}
                  </span>
                  <span className="progress">
                    <span style={{ width: `${progressRatio(item) * 100}%` }} />
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
