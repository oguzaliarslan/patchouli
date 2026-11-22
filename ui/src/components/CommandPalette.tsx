import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { useApp, type Page } from "../lib/context";
import { itemTitle } from "../lib/format";
import { keys, queryClient, useList, useSettings, useUpdateSettings } from "../lib/queries";

interface Command {
  id: string;
  label: string;
  /** Shown on the right: "Page", "Action", or the list status. */
  kind: string;
  /** Extra text to match, such as other titles. */
  keywords?: string;
  run: () => void;
}

const PAGES: [Page, string][] = [
  ["home", "Home"],
  ["list", "Anime List"],
  ["schedule", "Schedule"],
  ["history", "History"],
  ["search", "Search"],
  ["seasons", "Seasons"],
  ["sequels", "Sequels"],
  ["library", "Library"],
  ["torrents", "Torrents"],
  ["profile", "Profile"],
  ["settings", "Settings"],
];

const MAX_RESULTS = 50;

/** Every word of the query appears somewhere in the label or keywords. */
export function matches(command: Pick<Command, "label" | "keywords">, query: string): boolean {
  const haystack = `${command.label} ${command.keywords ?? ""}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((word) => haystack.includes(word));
}

/** Ctrl+K (⌘K): jump to a page or anime, or run an action. */
export function CommandPalette({ onSyncNow }: { onSyncNow: () => void }) {
  const app = useApp();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const dialog = useRef<HTMLDialogElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const { data: items = [] } = useList();
  const { data: settings } = useSettings();
  const updateSettings = useUpdateSettings();
  const language = settings?.["app.titleLanguage"] ?? "romaji";

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    const el = dialog.current;
    if (!el) return;
    if (open && !el.open) {
      setQuery("");
      setIndex(0);
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
  }, [open]);

  const commands = useMemo<Command[]>(() => {
    const detection = settings?.["track.detection.enabled"] ?? true;
    const result: Command[] = PAGES.map(([page, label]) => ({
      id: `page:${page}`,
      label,
      kind: "Page",
      run: () => app.navigate(page),
    }));
    result.push(
      { id: "sync", label: "Sync now", kind: "Action", run: onSyncNow },
      {
        id: "detection",
        label: detection ? "Pause detection" : "Resume detection",
        kind: "Action",
        keywords: "tracking players",
        run: () => updateSettings.mutate((s) => ({ ...s, "track.detection.enabled": !s["track.detection.enabled"] })),
      },
      {
        id: "backup",
        label: "Back up now",
        kind: "Action",
        run: () =>
          void api.backupNow().then(
            () => app.notify("Backed up."),
            (e) => app.notify(errorMessage(e)),
          ),
      },
      {
        id: "sequels",
        label: "Check for sequels and recommendations",
        kind: "Action",
        run: () =>
          void api.discoverRefresh().then(
            () => void queryClient.invalidateQueries({ queryKey: keys.discover }),
            (e) => app.notify(errorMessage(e)),
          ),
      },
    );
    for (const item of items) {
      const t = item.anime?.titles;
      result.push({
        id: `anime:${item.entry.animeId}`,
        label: itemTitle(item, language),
        kind: item.entry.status === "PlanToWatch" ? "Planned" : item.entry.status.replace(/([a-z])([A-Z])/g, "$1 $2"),
        keywords: [t?.romaji, t?.english, t?.japanese, ...(t?.synonyms ?? [])].filter(Boolean).join(" "),
        run: () => app.openAnime(item.entry.animeId),
      });
    }
    return result;
  }, [app, items, language, onSyncNow, settings, updateSettings]);

  const results = useMemo(
    // With nothing typed, pages and actions; anime once there's a query.
    () =>
      (query.trim() ? commands.filter((c) => matches(c, query)) : commands.filter((c) => !c.id.startsWith("anime:"))).slice(
        0,
        MAX_RESULTS,
      ),
    [commands, query],
  );

  useEffect(() => {
    list.current?.children[index]?.scrollIntoView({ block: "nearest" });
  }, [index]);

  function run(command: Command | undefined) {
    if (!command) return;
    setOpen(false);
    command.run();
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(results.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(results[index]);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="palette"
      aria-label="Go to"
      onClose={() => setOpen(false)}
      onClick={(e) => e.target === dialog.current && setOpen(false)}
    >
      {open && (
        <>
          <input
            autoFocus
            type="search"
            placeholder="Go to a page or anime, or run an action…"
            aria-label="Search pages, anime and actions"
            aria-controls="palette-results"
            aria-activedescendant={results[index] ? `palette-${results[index].id}` : undefined}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={onKeyDown}
          />
          {results.length === 0 ? (
            <p className="empty">Nothing matches.</p>
          ) : (
            <ul id="palette-results" ref={list} role="listbox">
              {results.map((c, i) => (
                <li
                  key={c.id}
                  id={`palette-${c.id}`}
                  role="option"
                  aria-selected={i === index}
                  onMouseMove={() => setIndex(i)}
                  onClick={() => run(c)}
                >
                  <span className="title-ellipsis">{c.label}</span>
                  <span className="muted small">{c.kind}</span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </dialog>
  );
}
