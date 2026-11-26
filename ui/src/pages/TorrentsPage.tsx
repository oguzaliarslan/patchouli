import { useMutation } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";
import { api, errorMessage, type Feed, type FeedItem } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "../lib/clipboard";
import { useApp } from "../lib/context";
import { preferredTitle, relativeTime } from "../lib/format";
import { usePref } from "../lib/prefs";
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

type ColumnId =
  | "title"
  | "episode"
  | "group"
  | "video"
  | "file"
  | "size"
  | "seeders"
  | "leechers"
  | "downloads"
  | "released";

type SortValue = string | number | null;

interface Column {
  id: ColumnId;
  label: string;
  /** Default width in pixels. */
  width: number;
  numeric?: boolean;
  /** The title column is always shown. */
  required?: boolean;
  /** Clicking the header first sorts descending (largest, newest first). */
  descendingFirst?: boolean;
}

const COLUMNS: Column[] = [
  { id: "title", label: "Anime", width: 260, required: true },
  { id: "episode", label: "Episode", width: 84 },
  { id: "group", label: "Group", width: 130 },
  { id: "video", label: "Video", width: 140 },
  { id: "file", label: "File name", width: 360 },
  { id: "size", label: "Size", width: 84, numeric: true, descendingFirst: true },
  { id: "seeders", label: "Seeders", width: 76, numeric: true, descendingFirst: true },
  { id: "leechers", label: "Leechers", width: 80, numeric: true, descendingFirst: true },
  { id: "downloads", label: "Downloads", width: 96, numeric: true, descendingFirst: true },
  { id: "released", label: "Released", width: 110, descendingFirst: true },
];

const MIN_WIDTH = 48;

interface Sort {
  column: ColumnId;
  descending: boolean;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Orders by `value`, keeping missing values last in either direction. */
function compareValues(a: SortValue, b: SortValue, descending: boolean): number {
  if (a === null || a === "") return b === null || b === "" ? 0 : 1;
  if (b === null || b === "") return -1;
  const order = typeof a === "number" && typeof b === "number" ? a - b : collator.compare(String(a), String(b));
  return descending ? -order : order;
}

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

  const [sort, setSort] = usePref<Sort | null>("torrents.sort", null);
  const [widths, setWidths] = usePref<Partial<Record<ColumnId, number>>>("torrents.widths", {});
  const [hiddenColumns, setHiddenColumns] = usePref<ColumnId[]>("torrents.hiddenColumns", []);
  const columns = COLUMNS.filter((c) => c.required || !hiddenColumns.includes(c.id));
  const widthOf = (c: Column) => Math.max(MIN_WIDTH, widths[c.id] ?? c.width);
  // Fixed tracks plus a filler, so a column only changes when its own divider is dragged.
  const gridTemplateColumns = `${columns.map((c) => `${widthOf(c)}px`).join(" ")} minmax(0, 1fr)`;

  const titleOf = (item: FeedItem) => {
    if (item.animeId == null) return item.episode.title || item.title;
    const listItem = anime.get(item.animeId);
    return preferredTitle(listItem?.anime, language, listItem?.displayTitle ?? undefined, item.animeId) || item.episode.title;
  };

  const sortValue = (item: FeedItem, column: ColumnId): SortValue => {
    switch (column) {
      case "title":
        return titleOf(item);
      case "episode":
        return item.episode.number ? `${item.episode.number} v${item.episode.version}` : null;
      case "group":
        return item.episode.group;
      case "video":
        return [item.episode.resolution, item.episode.video].filter(Boolean).join(" ");
      case "file":
        return item.title;
      case "size":
        return item.fileSize > 0 ? item.fileSize : null;
      case "seeders":
        return item.seeders;
      case "leechers":
        return item.leechers;
      case "downloads":
        return item.downloads;
      case "released":
        return item.pubTime || null;
    }
  };

  const visible = (feed?.items ?? []).filter((i) => showHidden || i.state !== "discardedHidden");
  const activeSort = sort && COLUMNS.some((c) => c.id === sort.column) ? sort : null;
  const items = activeSort
    ? visible
        .map((item, index) => ({ item, index, value: sortValue(item, activeSort.column) }))
        .sort((a, b) => compareValues(a.value, b.value, activeSort.descending) || a.index - b.index)
        .map((r) => r.item)
    : visible;
  const selected = items.filter((i) => i.state === "selected").length;
  const hidden = (feed?.items.length ?? 0) - items.length;

  function onHeaderClick(column: Column) {
    setSort(
      activeSort?.column === column.id
        ? { column: column.id, descending: !activeSort.descending }
        : { column: column.id, descending: !!column.descendingFirst },
    );
  }

  function openHeaderMenu(e: React.MouseEvent) {
    e.preventDefault();
    app.showMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        ...COLUMNS.filter((c) => !c.required).map((c) => ({
          label: c.label,
          checked: !hiddenColumns.includes(c.id),
          onSelect: () =>
            setHiddenColumns(
              hiddenColumns.includes(c.id) ? hiddenColumns.filter((id) => id !== c.id) : [...hiddenColumns, c.id],
            ),
        })),
        "separator",
        { label: "Feed order", disabled: !activeSort, onSelect: () => setSort(null) },
        { label: "Reset column widths", disabled: Object.keys(widths).length === 0, onSelect: () => setWidths({}) },
      ],
    });
  }

  // Dragging a divider: the column, the pointer's x and the column's width when the drag began.
  const drag = useRef<{ column: ColumnId; startX: number; startWidth: number } | null>(null);
  function startResize(e: React.PointerEvent, column: Column) {
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { column: column.id, startX: e.clientX, startWidth: widthOf(column) };
  }
  function moveResize(e: React.PointerEvent) {
    const d = drag.current;
    if (!d) return;
    const width = Math.max(MIN_WIDTH, Math.round(d.startWidth + e.clientX - d.startX));
    if (width !== widths[d.column]) setWidths({ ...widths, [d.column]: width });
  }
  function endResize() {
    drag.current = null;
  }
  function resetWidth(column: ColumnId) {
    const rest = { ...widths };
    delete rest[column];
    setWidths(rest);
  }

  function renderCell(item: FeedItem, column: ColumnId) {
    switch (column) {
      case "title":
        return (
          <span key={column} className="cell col-title" role="cell">
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
        );
      case "episode":
        return (
          <span key={column} className="cell" role="cell">
            {item.episode.number}
            {item.episode.version > 1 && ` v${item.episode.version}`}
            {item.newEpisode && <span className="badge accent">New</span>}
          </span>
        );
      case "group":
        return <span key={column} className="cell" role="cell">{item.episode.group}</span>;
      case "video":
        return (
          <span key={column} className="cell" role="cell">
            {[item.episode.resolution, item.episode.video].filter(Boolean).join(" ")}
          </span>
        );
      case "file":
        return <span key={column} className="cell muted" role="cell">{item.title}</span>;
      case "size":
        return <span key={column} className="cell col-num" role="cell">{formatSize(item.fileSize)}</span>;
      case "seeders":
        return <span key={column} className="cell col-num" role="cell">{item.seeders ?? ""}</span>;
      case "leechers":
        return <span key={column} className="cell col-num" role="cell">{item.leechers ?? ""}</span>;
      case "downloads":
        return <span key={column} className="cell col-num" role="cell">{item.downloads ?? ""}</span>;
      case "released":
        return (
          <span key={column} className="cell muted" role="cell">
            {item.pubTime ? relativeTime(item.pubTime) : ""}
          </span>
        );
    }
  }

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
            <div className="torrent-row grid-head" role="row" style={{ gridTemplateColumns }} onContextMenu={openHeaderMenu}>
              {columns.map((c) => {
                const sorted = activeSort?.column === c.id;
                return (
                  <span
                    key={c.id}
                    className={`cell torrent-head${c.numeric ? " col-num" : ""}`}
                    role="columnheader"
                    aria-sort={sorted ? (activeSort.descending ? "descending" : "ascending") : "none"}
                  >
                    <button
                      className="sort-button"
                      title="Click to sort; right-click to choose columns"
                      onClick={() => onHeaderClick(c)}
                    >
                      {c.label}
                      {sorted && <span className="sort-mark">{activeSort.descending ? "▼" : "▲"}</span>}
                    </button>
                    <span
                      className="col-resizer"
                      role="separator"
                      aria-orientation="vertical"
                      aria-label={`Resize ${c.label}`}
                      title="Drag to resize; double-click to reset"
                      onPointerDown={(e) => startResize(e, c)}
                      onPointerMove={moveResize}
                      onPointerUp={endResize}
                      onPointerCancel={endResize}
                      onDoubleClick={() => resetWidth(c.id)}
                    />
                  </span>
                );
              })}
            </div>
            {items.map((item) => (
              <div
                key={item.link || item.title}
                role="row"
                className={`torrent-row state-${item.state}`}
                style={{ gridTemplateColumns }}
                title={`${item.title}${item.description ? `\n${item.description}` : ""}`}
                onContextMenu={(e) => openMenu(e, item)}
                onDoubleClick={() => download.mutate([item.link])}
              >
                {columns.map((c) => renderCell(item, c.id))}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
