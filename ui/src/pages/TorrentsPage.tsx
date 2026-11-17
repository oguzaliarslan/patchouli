import { useMutation } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { api, errorMessage, type Feed, type FeedItem } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "../lib/clipboard";
import { useApp } from "../lib/context";
import { preferredTitle, relativeTime } from "../lib/format";
import { keys, queryClient, useList, useSettings, useTorrents } from "../lib/queries";

/** `1.4 GiB` from bytes. */
export function formatSize(bytes: number): string {
  if (bytes <= 0) return "";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

const STATE_LABELS: Record<FeedItem["state"], string> = {
  selected: "Selected",
  blank: "",
  discardedNormal: "Discarded",
  discardedInactive: "Discarded",
  discardedHidden: "Hidden",
};

export function TorrentsPage() {
  const app = useApp();
  const { data: feed } = useTorrents();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const [query, setQuery] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const language = settings?.["app.titleLanguage"] ?? "romaji";

  const anime = useMemo(() => new Map(list.map((i) => [i.entry.animeId, i])), [list]);

  const setFeed = (data: Feed | null) => queryClient.setQueryData(keys.torrents, data);
  const check = useMutation({
    mutationFn: (text: string) => (text.trim() ? api.searchTorrents(text) : api.checkTorrents()),
    onSuccess: setFeed,
    onError: (e) => app.notify(errorMessage(e)),
  });
  const download = useMutation({
    mutationFn: (links: string[]) => api.downloadTorrents(links),
    onSuccess: (result) => {
      const done = result.downloaded.length;
      const failed = result.errors.length;
      app.notify(
        done === 0
          ? "Nothing to download."
          : `Downloading ${done} torrent${done === 1 ? "" : "s"}${failed ? `; ${failed} failed: ${result.errors[0]}` : "."}`,
      );
      void queryClient.invalidateQueries({ queryKey: keys.torrentArchive });
    },
    onError: (e) => app.notify(errorMessage(e)),
  });

  const items = (feed?.items ?? []).filter((i) => showHidden || i.state !== "discardedHidden");
  const selected = items.filter((i) => i.state === "selected").length;
  const hidden = (feed?.items.length ?? 0) - items.length;

  const titleOf = (item: FeedItem) => {
    if (item.animeId == null) return item.episode.title || item.title;
    const listItem = anime.get(item.animeId);
    return preferredTitle(listItem?.anime, language, listItem?.displayTitle ?? undefined, item.animeId) || item.episode.title;
  };

  function openMenu(e: React.MouseEvent, item: FeedItem) {
    e.preventDefault();
    app.showMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Download", onSelect: () => download.mutate([item.link]) },
        item.state === "selected"
          ? {
              label: "Discard",
              onSelect: () => void api.setTorrentState(item.link, false).then(setFeed),
            }
          : {
              label: "Select",
              onSelect: () => void api.setTorrentState(item.link, true).then(setFeed),
            },
        "separator",
        {
          label: "Open anime",
          disabled: item.animeId == null,
          onSelect: () => item.animeId != null && app.openAnime(item.animeId),
        },
        {
          label: "Open info page",
          disabled: !item.infoLink,
          onSelect: () => void openUrl(item.infoLink),
        },
        { label: "Copy file name", onSelect: () => void writeText(item.title) },
      ],
    });
  }

  return (
    <div className="page list-page">
      <header className="page-header">
        <h1>Torrents</h1>
        <div className="page-actions">
          <button disabled={!selected || download.isPending} onClick={() => download.mutate([])}>
            {download.isPending ? "Downloading…" : `Download selected${selected ? ` (${selected})` : ""}`}
          </button>
        </div>
      </header>

      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          check.mutate(query);
        }}
      >
        <input
          type="search"
          placeholder="Search torrents (empty: check the feed)"
          aria-label="Search torrents"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="submit" className="primary" disabled={check.isPending}>
          {check.isPending ? "Checking…" : query.trim() ? "Search" : "Check feed"}
        </button>
        {hidden > 0 && (
          <label className="inline small">
            <input type="checkbox" checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} />
            Show {hidden} hidden
          </label>
        )}
        {feed && (
          <span className="muted small toolbar-count">
            {feed.title || feed.url} · {feed.items.length} items · {selected} selected
          </span>
        )}
      </form>

      {!feed ? (
        <p className="empty">
          Check the feed set in Settings › Torrents, or search for an anime. Filters select new episodes of
          anime you're watching.
        </p>
      ) : items.length === 0 ? (
        <p className="empty">The feed has no items.</p>
      ) : (
        <div className="grid-table" role="table">
          <div className="grid-body">
            <div className="torrent-row grid-head" role="row">
              <span className="cell" role="columnheader">Anime</span>
              <span className="cell" role="columnheader">Episode</span>
              <span className="cell" role="columnheader">Group</span>
              <span className="cell" role="columnheader">Video</span>
              <span className="cell" role="columnheader">Size</span>
              <span className="cell col-num" role="columnheader">Seeders</span>
              <span className="cell" role="columnheader">Released</span>
            </div>
            {items.map((item) => (
              <div
                key={item.link || item.title}
                role="row"
                className={`torrent-row state-${item.state}`}
                title={`${item.title}${item.description ? `\n${item.description}` : ""}`}
                onContextMenu={(e) => openMenu(e, item)}
                onDoubleClick={() => download.mutate([item.link])}
              >
                <span className="cell col-title" role="cell">
                  <input
                    type="checkbox"
                    aria-label={`Select ${item.title}`}
                    checked={item.state === "selected"}
                    onChange={(e) => void api.setTorrentState(item.link, e.target.checked).then(setFeed)}
                  />
                  <span className="title-clamp">{titleOf(item)}</span>
                  {item.category !== "Anime" && <span className="badge">{item.category}</span>}
                  {item.state !== "selected" && STATE_LABELS[item.state] && (
                    <span className="badge">{STATE_LABELS[item.state]}</span>
                  )}
                </span>
                <span className="cell" role="cell">
                  {item.episode.number}
                  {item.episode.version > 1 && ` v${item.episode.version}`}
                  {item.newEpisode && <span className="badge accent">New</span>}
                </span>
                <span className="cell" role="cell">{item.episode.group}</span>
                <span className="cell" role="cell">{[item.episode.resolution, item.episode.video].filter(Boolean).join(" ")}</span>
                <span className="cell" role="cell">{formatSize(item.fileSize)}</span>
                <span className="cell col-num" role="cell">{item.seeders ?? ""}</span>
                <span className="cell muted" role="cell">{item.pubTime ? relativeTime(item.pubTime) : ""}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
