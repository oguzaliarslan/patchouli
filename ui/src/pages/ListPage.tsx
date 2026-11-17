import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type ListItem, type ListStatus } from "../api";
import { Icon } from "../components/Icon";
import { Poster } from "../components/Poster";
import { useApp, type AppActions } from "../lib/context";
import { withEpisodeWatched } from "../lib/entries";
import {
  LIST_STATUSES,
  LIST_STATUS_LABELS,
  TYPE_LABELS,
  episodesText,
  formatDate,
  formatScore,
  formatSeason,
  isNsfw,
  itemTitle,
  parseFuzzy,
  progressRatio,
  relativeTime,
} from "../lib/format";
import { usePref } from "../lib/prefs";
import { useCurrentAccount, useList, useSaveEntry, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";

type Column = "title" | "progress" | "score" | "type" | "season" | "started" | "completed" | "updated";

interface Sort {
  column: Column;
  descending: boolean;
}

const COLUMNS: { id: Column; label: string; numeric?: boolean }[] = [
  { id: "title", label: "Title" },
  { id: "progress", label: "Progress" },
  { id: "score", label: "Score", numeric: true },
  { id: "type", label: "Type" },
  { id: "season", label: "Season" },
  { id: "started", label: "Started" },
  { id: "completed", label: "Completed" },
  { id: "updated", label: "Updated" },
];

const ROW_HEIGHT = 44;
const HEAD_HEIGHT = 32;
const CARD_WIDTH = 168;
/** Poster (2:3) plus progress, two title lines and a detail line. */
const CARD_HEIGHT = Math.round(CARD_WIDTH * 1.5) + 76;
const CARD_GAP = 16;

/** Asks where to save, then writes the list. */
async function exportList(app: AppActions, format: "markdown" | "myAnimeListXml") {
  const markdown = format === "markdown";
  try {
    const path = await save({
      title: "Export list",
      defaultPath: markdown ? "anime-list.md" : "animelist.xml",
      filters: [
        markdown
          ? { name: "Markdown", extensions: ["md"] }
          : { name: "MyAnimeList XML", extensions: ["xml"] },
      ],
    });
    if (!path) return;
    const count = await api.exportList(format, path);
    app.notify(`Exported ${count} entries.`);
  } catch (e) {
    app.notify(errorMessage(e));
  }
}

/** Fuzzy dates with unknown parts sort last, as in Taiga. */
function compareDates(a: string, b: string): number {
  const x = parseFuzzy(a);
  const y = parseFuzzy(b);
  for (const key of ["year", "month", "day"] as const) {
    if (x[key] === y[key]) continue;
    if (!x[key]) return 1;
    if (!y[key]) return -1;
    return x[key] - y[key];
  }
  return 0;
}

export function ListPage() {
  const app = useApp();
  const { data: items = [], isLoading } = useList();
  const { data: settings } = useSettings();
  const { scoreFormat } = useCurrentAccount();
  const save = useSaveEntry();
  const openMenu = useAnimeMenu();
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;

  const [status, setStatus] = usePref<ListStatus | "all">("list.status", "Watching");
  const [view, setView] = usePref<"table" | "cards">("list.view", "table");
  const [sort, setSort] = usePref<Sort>("list.sort", { column: "title", descending: false });
  const [secondary, setSecondary] = usePref<Sort | null>("list.sort2", null);
  const [text, setText] = useState("");
  const [type, setType] = useState("");
  const [selected, setSelected] = useState<number | null>(null);

  const visible = useMemo(
    () => items.filter((i) => !(hideNsfw && isNsfw(i.anime))),
    [items, hideNsfw],
  );

  const counts = useMemo(() => {
    const result: Record<string, number> = { all: visible.length };
    for (const s of LIST_STATUSES) result[s] = 0;
    for (const i of visible) result[i.entry.status] = (result[i.entry.status] ?? 0) + 1;
    return result;
  }, [visible]);

  const rows = useMemo(() => {
    const needle = text.trim().toLowerCase();
    const filtered = visible.filter((i) => {
      if (status !== "all" && i.entry.status !== status) return false;
      if (type && i.anime?.type !== type) return false;
      if (!needle) return true;
      const t = i.anime?.titles;
      const haystack = [i.displayTitle, t?.romaji, t?.english, t?.japanese, ...(t?.synonyms ?? [])];
      return haystack.some((s) => s?.toLowerCase().includes(needle));
    });
    const titles = new Map(filtered.map((i) => [i.entry.animeId, itemTitle(i, language)]));
    const compare = (a: ListItem, b: ListItem, column: Column): number => {
      switch (column) {
        case "title":
          return titles.get(a.entry.animeId)!.localeCompare(titles.get(b.entry.animeId)!);
        case "progress":
          return progressRatio(a) - progressRatio(b) || a.entry.watchedEpisodes - b.entry.watchedEpisodes;
        case "score":
          return a.entry.score - b.entry.score;
        case "type":
          return (TYPE_LABELS[a.anime?.type ?? "Unknown"] ?? "").localeCompare(
            TYPE_LABELS[b.anime?.type ?? "Unknown"] ?? "",
          );
        case "season":
          return compareDates(a.anime?.dateStarted ?? "", b.anime?.dateStarted ?? "");
        case "started":
          return compareDates(a.entry.dateStarted, b.entry.dateStarted);
        case "completed":
          return compareDates(a.entry.dateCompleted, b.entry.dateCompleted);
        case "updated":
          return a.entry.lastUpdated - b.entry.lastUpdated;
      }
    };
    const sorts = [sort, secondary, { column: "title" as Column, descending: false }].filter(
      (s): s is Sort => !!s,
    );
    return filtered.sort((a, b) => {
      for (const s of sorts) {
        const result = compare(a, b, s.column);
        if (result) return s.descending ? -result : result;
      }
      return 0;
    });
  }, [visible, status, type, text, sort, secondary, language]);

  function onHeaderClick(column: Column, shift: boolean) {
    if (shift) {
      // Shift-click sets the secondary sort.
      if (column === sort.column) return;
      setSecondary(
        secondary?.column === column ? { column, descending: !secondary.descending } : { column, descending: false },
      );
      return;
    }
    if (column === sort.column) {
      setSort({ column, descending: !sort.descending });
    } else {
      setSort({ column, descending: column === "score" || column === "updated" });
      if (secondary?.column === column) setSecondary(null);
    }
  }

  function step(item: ListItem, delta: 1 | -1) {
    const next = item.entry.watchedEpisodes + delta;
    const total = item.anime?.episodeCount ?? 0;
    if (next < 0 || (total > 0 && next > total)) return;
    const entry =
      delta > 0
        ? withEpisodeWatched(item.anime, item.entry, item.entry.animeId, next)
        : { ...item.entry, watchedEpisodes: next };
    save.mutate(entry);
  }

  const menuFor = (e: React.MouseEvent, item: ListItem) => {
    setSelected(item.entry.animeId);
    openMenu(e, {
      animeId: item.entry.animeId,
      anime: item.anime,
      entry: item.entry,
      title: itemTitle(item, language),
    });
  };

  return (
    <div className="page list-page">
      <header className="page-header">
        <h1>Anime List</h1>
        <div className="page-actions">
          <button
            disabled={items.length === 0}
            onClick={(e) =>
              app.showMenu({
                x: e.currentTarget.getBoundingClientRect().left,
                y: e.currentTarget.getBoundingClientRect().bottom + 4,
                items: [
                  { label: "As Markdown…", onSelect: () => void exportList(app, "markdown") },
                  {
                    label: "As MyAnimeList XML…",
                    onSelect: () => void exportList(app, "myAnimeListXml"),
                  },
                ],
              })
            }
          >
            Export
          </button>
          <div className="segmented" role="group" aria-label="View">
            <button aria-pressed={view === "table"} title="Table" onClick={() => setView("table")}>
              <Icon name="rows" />
            </button>
            <button aria-pressed={view === "cards"} title="Cards" onClick={() => setView("cards")}>
              <Icon name="grid" />
            </button>
          </div>
        </div>
      </header>

      <div className="tabs" role="tablist">
        {(["all", ...LIST_STATUSES] as const).map((s) => (
          <button key={s} role="tab" aria-selected={status === s} onClick={() => setStatus(s)}>
            {s === "all" ? "All" : LIST_STATUS_LABELS[s]}
            <span className="count">{counts[s] ?? 0}</span>
          </button>
        ))}
      </div>

      <div className="toolbar">
        <input
          type="search"
          placeholder="Filter by title"
          value={text}
          onChange={(e) => setText(e.target.value)}
          aria-label="Filter by title"
        />
        <select value={type} onChange={(e) => setType(e.target.value)} aria-label="Type">
          <option value="">All types</option>
          {Object.entries(TYPE_LABELS)
            .filter(([k]) => k !== "Unknown")
            .map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
        </select>
        {view === "cards" && (
          <select
            value={`${sort.column}:${sort.descending ? "desc" : "asc"}`}
            aria-label="Sort"
            onChange={(e) => {
              const [column, dir] = e.target.value.split(":");
              setSort({ column: column as Column, descending: dir === "desc" });
            }}
          >
            {COLUMNS.flatMap((c) => [
              <option key={`${c.id}:asc`} value={`${c.id}:asc`}>
                {c.label} ↑
              </option>,
              <option key={`${c.id}:desc`} value={`${c.id}:desc`}>
                {c.label} ↓
              </option>,
            ])}
          </select>
        )}
        <span className="muted small toolbar-count">
          {rows.length} of {visible.length}
          {secondary && view === "table" && (
            <>
              {" "}
              · then by {COLUMNS.find((c) => c.id === secondary.column)?.label}{" "}
              <button className="link" onClick={() => setSecondary(null)}>
                clear
              </button>
            </>
          )}
        </span>
      </div>

      {isLoading ? (
        <p className="empty">Loading…</p>
      ) : items.length === 0 ? (
        <p className="empty">Your list is empty. Log in under Settings › Accounts, then sync.</p>
      ) : rows.length === 0 ? (
        <p className="empty">No anime match these filters.</p>
      ) : view === "table" ? (
        <ListTable
          rows={rows}
          sort={sort}
          secondary={secondary}
          selected={selected}
          language={language}
          scoreFormat={scoreFormat}
          onHeaderClick={onHeaderClick}
          onSelect={setSelected}
          onOpen={(id) => app.openAnime(id)}
          onMenu={menuFor}
          onStep={step}
        />
      ) : (
        <ListCards
          rows={rows}
          language={language}
          scoreFormat={scoreFormat}
          onOpen={(id) => app.openAnime(id)}
          onMenu={menuFor}
        />
      )}
    </div>
  );
}

function ListTable({
  rows,
  sort,
  secondary,
  selected,
  language,
  scoreFormat,
  onHeaderClick,
  onSelect,
  onOpen,
  onMenu,
  onStep,
}: {
  rows: ListItem[];
  sort: Sort;
  secondary: Sort | null;
  selected: number | null;
  language: "romaji" | "english" | "native";
  scoreFormat: string | null;
  onHeaderClick: (column: Column, shift: boolean) => void;
  onSelect: (id: number) => void;
  onOpen: (id: number) => void;
  onMenu: (e: React.MouseEvent, item: ListItem) => void;
  onStep: (item: ListItem, delta: 1 | -1) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
    // The sticky header sits above the rows inside the same scroller.
    scrollMargin: HEAD_HEIGHT,
  });

  function onKeyDown(e: React.KeyboardEvent) {
    const index = rows.findIndex((r) => r.entry.animeId === selected);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(0, Math.min(rows.length - 1, index + (e.key === "ArrowDown" ? 1 : -1)));
      onSelect(rows[next].entry.animeId);
      virtualizer.scrollToIndex(next);
    } else if (e.key === "Enter" && index >= 0) {
      onOpen(rows[index].entry.animeId);
    }
  }

  return (
    <div className="grid-table" role="grid" aria-rowcount={rows.length + 1}>
      <div className="grid-body" ref={scroller} tabIndex={0} onKeyDown={onKeyDown}>
      <div className="grid-row grid-head" role="row" style={{ height: HEAD_HEIGHT }}>
        {COLUMNS.map((c) => {
          const primary = sort.column === c.id;
          const second = secondary?.column === c.id;
          return (
            <div
              key={c.id}
              role="columnheader"
              aria-sort={primary ? (sort.descending ? "descending" : "ascending") : "none"}
              className={`cell col-${c.id}`}
            >
              <button
                className="sort-button"
                title="Click to sort, Shift+click to sort by this next"
                onClick={(e) => onHeaderClick(c.id, e.shiftKey)}
              >
                {c.label}
                {primary && <span className="sort-mark">{sort.descending ? "▼" : "▲"}</span>}
                {second && <span className="sort-mark secondary">{secondary.descending ? "▽" : "△"}</span>}
              </button>
            </div>
          );
        })}
      </div>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const item = rows[v.index];
            const id = item.entry.animeId;
            const total = item.anime?.episodeCount ?? 0;
            const title = itemTitle(item, language);
            return (
              <div
                key={id}
                role="row"
                aria-rowindex={v.index + 2}
                aria-selected={selected === id}
                className={`grid-row${selected === id ? " selected" : ""}`}
                style={{ transform: `translateY(${v.start - HEAD_HEIGHT}px)`, height: ROW_HEIGHT }}
                onClick={() => onSelect(id)}
                onDoubleClick={() => onOpen(id)}
                onContextMenu={(e) => onMenu(e, item)}
              >
                <div role="gridcell" className="cell col-title">
                  <span className="title-ellipsis" title={title}>
                    {title}
                  </span>
                  {item.entry.rewatching && <span className="badge">rewatching</span>}
                  {item.queued && <span className="badge accent">unsent</span>}
                </div>
                <div role="gridcell" className="cell col-progress">
                  <button
                    className="step"
                    aria-label="One episode less"
                    disabled={item.entry.watchedEpisodes <= 0}
                    onClick={(e) => {
                      e.stopPropagation();
                      onStep(item, -1);
                    }}
                  >
                    −
                  </button>
                  <span className="progress-cell">
                    <span className="progress">
                      <span style={{ width: `${progressRatio(item) * 100}%` }} />
                    </span>
                    <span className="small">{episodesText(item.entry.watchedEpisodes, total)}</span>
                  </span>
                  <button
                    className="step"
                    aria-label="Watched next episode"
                    disabled={total > 0 && item.entry.watchedEpisodes >= total}
                    onClick={(e) => {
                      e.stopPropagation();
                      onStep(item, 1);
                    }}
                  >
                    +
                  </button>
                </div>
                <div role="gridcell" className="cell col-score">
                  {formatScore(item.entry.score, scoreFormat)}
                </div>
                <div role="gridcell" className="cell col-type">
                  {TYPE_LABELS[item.anime?.type ?? "Unknown"]}
                </div>
                <div role="gridcell" className="cell col-season">
                  {formatSeason(item.anime?.dateStarted ?? "")}
                </div>
                <div role="gridcell" className="cell col-started">
                  {formatDate(item.entry.dateStarted)}
                </div>
                <div role="gridcell" className="cell col-completed">
                  {formatDate(item.entry.dateCompleted)}
                </div>
                <div role="gridcell" className="cell col-updated" title={new Date(item.entry.lastUpdated * 1000).toLocaleString()}>
                  {relativeTime(item.entry.lastUpdated)}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function ListCards({
  rows,
  language,
  scoreFormat,
  onOpen,
  onMenu,
}: {
  rows: ListItem[];
  language: "romaji" | "english" | "native";
  scoreFormat: string | null;
  onOpen: (id: number) => void;
  onMenu: (e: React.MouseEvent, item: ListItem) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const perRow = Math.max(1, Math.floor((width + CARD_GAP) / (CARD_WIDTH + CARD_GAP)));
  const rowCount = Math.ceil(rows.length / perRow);
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scroller.current,
    estimateSize: () => CARD_HEIGHT + CARD_GAP,
    overscan: 3,
  });

  return (
    <div className="card-scroll" ref={scroller}>
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((v) => (
          <div
            key={v.key}
            className="card-row"
            style={{ transform: `translateY(${v.start}px)`, gridTemplateColumns: `repeat(${perRow}, ${CARD_WIDTH}px)` }}
          >
            {rows.slice(v.index * perRow, v.index * perRow + perRow).map((item) => {
              const title = itemTitle(item, language);
              return (
                <button
                  key={item.entry.animeId}
                  className="anime-card"
                  onClick={() => onOpen(item.entry.animeId)}
                  onContextMenu={(e) => onMenu(e, item)}
                >
                  <Poster anime={item.anime} title={title} />
                  <span className="progress flat">
                    <span style={{ width: `${progressRatio(item) * 100}%` }} />
                  </span>
                  <span className="anime-card-title title-clamp">{title}</span>
                  <span className="muted small">
                    {episodesText(item.entry.watchedEpisodes, item.anime?.episodeCount)}
                    {item.entry.score > 0 && ` · ${formatScore(item.entry.score, scoreFormat)}`}
                    {item.queued && " · unsent"}
                  </span>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
