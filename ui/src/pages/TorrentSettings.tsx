import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import {
  api,
  errorMessage,
  type FeedFilter,
  type FilterCondition,
  type FilterElement,
  type FilterOperator,
} from "../api";
import { Field, LazyInput, Toggle } from "../components/inputs";
import { useApp } from "../lib/context";
import { AIRING_LABELS, LIST_STATUSES, LIST_STATUS_LABELS, TYPE_LABELS, preferredTitle } from "../lib/format";
import { keys, queryClient, useFilterPresets, useList } from "../lib/queries";
import { confirmAction } from "../lib/useAnimeMenu";
import type { SectionProps } from "./SettingsPage";

const ELEMENTS: [FilterElement, string][] = [
  ["file_title", "File name"],
  ["file_category", "File category"],
  ["file_description", "File description"],
  ["file_link", "File link"],
  ["file_size", "File size"],
  ["meta_id", "Anime ID"],
  ["episode_title", "Episode title"],
  ["meta_date_start", "Anime date started"],
  ["meta_date_end", "Anime date ended"],
  ["meta_episodes", "Anime episode count"],
  ["meta_status", "Anime airing status"],
  ["meta_type", "Anime type"],
  ["user_tags", "Anime notes"],
  ["user_status", "Anime watching status"],
  ["episode_number", "Episode number"],
  ["episode_version", "Episode version"],
  ["local_episode_available", "Episode available locally"],
  ["episode_group", "Episode fansub group"],
  ["episode_video_resolution", "Episode video resolution"],
  ["episode_video_type", "Episode video type"],
];

const OPERATORS: [FilterOperator, string][] = [
  ["equals", "is"],
  ["notequals", "is not"],
  ["gt", "is greater than"],
  ["ge", "is greater than or equal to"],
  ["lt", "is less than"],
  ["le", "is less than or equal to"],
  ["beginswith", "begins with"],
  ["endswith", "ends with"],
  ["contains", "contains"],
  ["notcontains", "does not contain"],
];

const ACTIONS: Record<FeedFilter["action"], string> = {
  discard: "Discard matched items",
  select: "Select matched items",
  prefer: "Prefer matched items to similar ones",
};

/** Numeric enum values (as Taiga stores them) for elements that have names. */
const VALUE_CHOICES: Partial<Record<FilterElement, [string, string][]>> = {
  user_status: [
    ["0", "Not in list"],
    ...LIST_STATUSES.map((s, i): [string, string] => [String(i + 1), LIST_STATUS_LABELS[s]]),
  ],
  meta_status: ["Unknown", "FinishedAiring", "Airing", "NotYetAired"].map((s, i) => [String(i), AIRING_LABELS[s] ?? s]),
  meta_type: ["Unknown", "Tv", "Ova", "Movie", "Special", "Ona", "Music"].map((s, i) => [String(i), TYPE_LABELS[s] ?? s]),
  local_episode_available: [
    ["True", "Yes"],
    ["0", "No"],
  ],
  file_category: [
    ["Anime", "Anime"],
    ["Batch", "Batch"],
    ["Other", "Other"],
  ],
};

function describe(c: FilterCondition): string {
  const element = ELEMENTS.find(([e]) => e === c.element)?.[1] ?? c.element;
  const op = OPERATORS.find(([o]) => o === c.operator)?.[1] ?? c.operator;
  const value = VALUE_CHOICES[c.element]?.find(([v]) => v === c.value)?.[1] ?? c.value;
  return `${element} ${op} “${value}”`;
}

export function TorrentsSection({ settings, change }: SectionProps) {
  const app = useApp();
  const filters = settings["torrents.filters"];
  const [editing, setEditing] = useState<number | null>(null);
  const { data: presets = [] } = useFilterPresets();
  const { data: archive } = useQuery({ queryKey: keys.torrentArchive, queryFn: api.torrentArchive });

  const setFilters = (next: FeedFilter[]) => change("torrents.filters", next);
  const browse = async (key: "torrents.download.folder" | "torrents.download.fileFolder" | "torrents.app.path") => {
    const picked = await openDialog(key === "torrents.app.path" ? { title: "Select BitTorrent client" } : { directory: true });
    if (typeof picked === "string") change(key, picked);
  };

  async function clearArchive() {
    if (!(await confirmAction("Forget which torrents were downloaded? They can then be selected again.", "Clear archive", "Clear"))) return;
    try {
      await api.clearTorrentArchive();
      void queryClient.invalidateQueries({ queryKey: keys.torrentArchive });
    } catch (e) {
      app.notify(errorMessage(e));
    }
  }

  return (
    <section className="settings-section">
      <h2>Torrents</h2>
      <Field label="Feed" htmlFor="torrent-source" hint="An RSS feed of torrents, checked by Torrents › Check feed.">
        <LazyInput id="torrent-source" type="url" value={settings["torrents.source"]} onCommit={(v) => change("torrents.source", v.trim())} />
      </Field>
      <Field label="Search" htmlFor="torrent-search" hint="%title% is replaced with what you search for.">
        <LazyInput id="torrent-search" type="url" value={settings["torrents.searchUrl"]} onCommit={(v) => change("torrents.searchUrl", v.trim())} />
      </Field>
      <Toggle
        label="Check the feed automatically"
        checked={settings["torrents.autoCheck.enabled"]}
        onChange={(v) => change("torrents.autoCheck.enabled", v)}
      />
      {settings["torrents.autoCheck.enabled"] && (
        <>
          <Field label="Every" htmlFor="torrent-interval">
            <span className="inline">
              <input
                id="torrent-interval"
                type="number"
                className="narrow"
                min={10}
                value={settings["torrents.autoCheck.interval"]}
                onChange={(e) => {
                  const minutes = Number.parseInt(e.target.value, 10);
                  if (minutes >= 10) change("torrents.autoCheck.interval", minutes);
                }}
              />
              <span className="muted">minutes</span>
            </span>
          </Field>
          <Field label="When new items are selected" htmlFor="torrent-action">
            <select
              id="torrent-action"
              value={settings["torrents.autoCheck.action"]}
              onChange={(e) => change("torrents.autoCheck.action", e.target.value as "notify" | "download")}
            >
              <option value="notify">Notify me</option>
              <option value="download">Download them</option>
            </select>
          </Field>
        </>
      )}

      <h3>Downloads</h3>
      <Toggle
        label="Open downloaded torrents"
        checked={settings["torrents.app.open"]}
        onChange={(v) => change("torrents.app.open", v)}
      />
      <Field label="With" htmlFor="torrent-app">
        <select
          id="torrent-app"
          value={settings["torrents.app.mode"]}
          onChange={(e) => change("torrents.app.mode", e.target.value as "default" | "custom")}
        >
          <option value="default">The default program for torrents</option>
          <option value="custom">This program</option>
        </select>
      </Field>
      {settings["torrents.app.mode"] === "custom" && (
        <Field label="Program" htmlFor="torrent-app-path" hint="aria2, Deluge, PicoTorrent, qBittorrent, Transmission and µTorrent can be told where to save.">
          <span className="inline">
            <LazyInput id="torrent-app-path" value={settings["torrents.app.path"]} onCommit={(v) => change("torrents.app.path", v.trim())} />
            <button onClick={() => void browse("torrents.app.path")}>Browse…</button>
          </span>
        </Field>
      )}
      <Toggle
        label="Save into the anime's folder"
        description="Tells the client where to save, using the folder from the anime's settings."
        checked={settings["torrents.download.useAnimeFolder"]}
        onChange={(v) => change("torrents.download.useAnimeFolder", v)}
      />
      {settings["torrents.download.useAnimeFolder"] && (
        <>
          <Toggle
            label="Otherwise, save into a download folder"
            checked={settings["torrents.download.fallbackToFolder"]}
            onChange={(v) => change("torrents.download.fallbackToFolder", v)}
          />
          {settings["torrents.download.fallbackToFolder"] && (
            <>
              <Field label="Download folder" htmlFor="torrent-folder">
                <span className="inline">
                  <LazyInput id="torrent-folder" value={settings["torrents.download.folder"]} onCommit={(v) => change("torrents.download.folder", v.trim())} />
                  <button onClick={() => void browse("torrents.download.folder")}>Browse…</button>
                </span>
              </Field>
              <Toggle
                label="In a subfolder named after the anime"
                checked={settings["torrents.download.createSubfolder"]}
                onChange={(v) => change("torrents.download.createSubfolder", v)}
              />
            </>
          )}
        </>
      )}
      <Field label="Save .torrent files in" htmlFor="torrent-files" hint="Empty: the app's data folder.">
        <span className="inline">
          <LazyInput id="torrent-files" value={settings["torrents.download.fileFolder"]} onCommit={(v) => change("torrents.download.fileFolder", v.trim())} />
          <button onClick={() => void browse("torrents.download.fileFolder")}>Browse…</button>
        </span>
      </Field>
      <Toggle
        label="Use magnet links when available"
        checked={settings["torrents.download.useMagnet"]}
        onChange={(v) => change("torrents.download.useMagnet", v)}
      />
      <Field label="Download order" htmlFor="torrent-order">
        <span className="inline">
          <select
            id="torrent-order"
            value={settings["torrents.download.sortBy"]}
            onChange={(e) => change("torrents.download.sortBy", e.target.value as "episodeNumber" | "releaseDate")}
          >
            <option value="episodeNumber">By episode number</option>
            <option value="releaseDate">By release date</option>
          </select>
          <select
            aria-label="Direction"
            value={settings["torrents.download.descending"] ? "desc" : "asc"}
            onChange={(e) => change("torrents.download.descending", e.target.value === "desc")}
          >
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </select>
        </span>
      </Field>

      <h3>Filters</h3>
      <Toggle
        label="Use filters"
        description="Filters select and discard items; downloaded items are always discarded."
        checked={settings["torrents.filter.enabled"]}
        onChange={(v) => change("torrents.filter.enabled", v)}
      />
      <ul className="filter-list">
        {filters.map((filter, index) => (
          <li key={index}>
            <div className="filter-row">
              <input
                type="checkbox"
                aria-label={`Enable ${filter.name}`}
                checked={filter.enabled}
                onChange={(e) => setFilters(filters.map((f, i) => (i === index ? { ...f, enabled: e.target.checked } : f)))}
              />
              <button className="link filter-name" onClick={() => setEditing(editing === index ? null : index)}>
                {filter.name}
              </button>
              <span className="muted small">{ACTIONS[filter.action]}</span>
              <span className="filter-actions">
                <button disabled={index === 0} aria-label="Move up" onClick={() => setFilters(move(filters, index, -1))}>↑</button>
                <button disabled={index === filters.length - 1} aria-label="Move down" onClick={() => setFilters(move(filters, index, 1))}>↓</button>
                <button onClick={() => setFilters(filters.filter((_, i) => i !== index))}>Remove</button>
              </span>
            </div>
            {editing === index ? (
              <FilterEditor
                filter={filter}
                onChange={(next) => setFilters(filters.map((f, i) => (i === index ? next : f)))}
              />
            ) : (
              <p className="muted small filter-summary">
                {filter.conditions.map(describe).join(filter.match === "all" ? " and " : " or ") || "No conditions"}
                {filter.animeIds.length > 0 && ` · ${filter.animeIds.length} anime`}
              </p>
            )}
          </li>
        ))}
      </ul>
      <Field label="Add filter" htmlFor="filter-preset">
        <select
          id="filter-preset"
          value=""
          onChange={(e) => {
            const preset = presets[Number(e.target.value)];
            if (!preset) return;
            setFilters([...filters, structuredClone(preset.filter)]);
            setEditing(filters.length);
          }}
        >
          <option value="">Choose a preset…</option>
          {presets.map((p, i) => (
            <option key={i} value={i} title={p.description}>
              {p.filter.name} — {p.description}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Downloaded" hint={`${archive?.count ?? 0} torrents are remembered, up to ${settings["torrents.filter.archiveMaxCount"]}.`}>
        <button disabled={!archive?.count} onClick={() => void clearArchive()}>
          Clear archive…
        </button>
      </Field>
    </section>
  );
}

function move<T>(items: T[], index: number, by: number): T[] {
  const next = [...items];
  const [item] = next.splice(index, 1);
  next.splice(index + by, 0, item);
  return next;
}

function FilterEditor({ filter, onChange }: { filter: FeedFilter; onChange: (f: FeedFilter) => void }) {
  const { data: list = [] } = useList();
  const set = <K extends keyof FeedFilter>(key: K, value: FeedFilter[K]) => onChange({ ...filter, [key]: value });
  const setCondition = (index: number, condition: FilterCondition) =>
    set("conditions", filter.conditions.map((c, i) => (i === index ? condition : c)));
  const titleOf = (id: number) => {
    const item = list.find((i) => i.entry.animeId === id);
    return preferredTitle(item?.anime, "romaji", item?.displayTitle ?? undefined, id);
  };
  const sorted = [...list].sort((a, b) => titleOf(a.entry.animeId).localeCompare(titleOf(b.entry.animeId)));

  return (
    <div className="filter-editor">
      <Field label="Name" htmlFor="filter-name">
        <LazyInput id="filter-name" value={filter.name} onCommit={(v) => set("name", v.trim() || "Filter")} />
      </Field>
      <Field label="Action" htmlFor="filter-action">
        <select id="filter-action" value={filter.action} onChange={(e) => set("action", e.target.value as FeedFilter["action"])}>
          {Object.entries(ACTIONS).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Match" htmlFor="filter-match">
        <select id="filter-match" value={filter.match} onChange={(e) => set("match", e.target.value as FeedFilter["match"])}>
          <option value="all">All conditions</option>
          <option value="any">Any condition</option>
        </select>
      </Field>
      <Field label="Discarded items" htmlFor="filter-option">
        <select id="filter-option" value={filter.option} onChange={(e) => set("option", e.target.value as FeedFilter["option"])}>
          <option value="default">Show</option>
          <option value="deactivate">Show greyed out</option>
          <option value="hide">Hide</option>
        </select>
      </Field>
      <Field label="Conditions">
        <div className="condition-list">
          {filter.conditions.map((c, index) => {
            const choices = VALUE_CHOICES[c.element];
            return (
              <div key={index} className="condition-row">
                <select
                  aria-label="Element"
                  value={c.element}
                  onChange={(e) => setCondition(index, { ...c, element: e.target.value as FilterElement })}
                >
                  {ELEMENTS.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Operator"
                  value={c.operator}
                  onChange={(e) => setCondition(index, { ...c, operator: e.target.value as FilterOperator })}
                >
                  {OPERATORS.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
                {choices ? (
                  <select aria-label="Value" value={c.value} onChange={(e) => setCondition(index, { ...c, value: e.target.value })}>
                    {!choices.some(([v]) => v === c.value) && <option value={c.value}>{c.value || "(empty)"}</option>}
                    {choices.map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                ) : (
                  <LazyInput value={c.value} placeholder="Value, or %watched%, %total%…" onCommit={(v) => setCondition(index, { ...c, value: v })} />
                )}
                <button aria-label="Remove condition" onClick={() => set("conditions", filter.conditions.filter((_, i) => i !== index))}>
                  −
                </button>
              </div>
            );
          })}
          <button
            onClick={() => set("conditions", [...filter.conditions, { element: "file_title", operator: "contains", value: "" }])}
          >
            Add condition
          </button>
        </div>
      </Field>
      <Field label="Anime" hint="Limit the filter to these anime; empty means all.">
        <div className="chips">
          {filter.animeIds.map((id) => (
            <span key={id} className="chip">
              {titleOf(id)}{" "}
              <button className="link" aria-label={`Remove ${titleOf(id)}`} onClick={() => set("animeIds", filter.animeIds.filter((a) => a !== id))}>
                ×
              </button>
            </span>
          ))}
          <select
            aria-label="Add anime"
            value=""
            onChange={(e) => {
              const id = Number(e.target.value);
              if (id && !filter.animeIds.includes(id)) set("animeIds", [...filter.animeIds, id]);
            }}
          >
            <option value="">Add anime…</option>
            {sorted.map((i) => (
              <option key={i.entry.animeId} value={i.entry.animeId}>
                {titleOf(i.entry.animeId)}
              </option>
            ))}
          </select>
        </div>
      </Field>
    </div>
  );
}
