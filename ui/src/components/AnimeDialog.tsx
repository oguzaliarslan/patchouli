import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useRef, useState } from "react";
import {
  api,
  errorMessage,
  newEntry,
  type Anime,
  type AnimeDetails,
  type AnimeSettings,
  type ListEntry,
} from "../api";
import { useApp } from "../lib/context";
import {
  AIRING_LABELS,
  LIST_STATUSES,
  LIST_STATUS_LABELS,
  TYPE_LABELS,
  formatDate,
  formatSeason,
  formatTime,
  isEmptyDate,
  plainText,
  preferredTitle,
} from "../lib/format";
import { nextEpisode, withStatus } from "../lib/entries";
import { keys, queryClient, useAnime, useCurrentAccount, useRemoveEntry, useSaveEntry, useSettings } from "../lib/queries";
import { confirmAction, openFolder, playEpisode } from "../lib/useAnimeMenu";
import { Icon } from "./Icon";
import { FuzzyDateInput, ScoreSelect } from "./inputs";
import { Poster } from "./Poster";

type Tab = "details" | "entry" | "settings";

/** Taiga's media dialog: details, list entry editor and per-anime settings. */
export function AnimeDialog({
  animeId,
  initialTab,
  onClose,
}: {
  animeId: number;
  initialTab: Tab;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [tab, setTab] = useState<Tab>(initialTab);
  const { data, isLoading } = useAnime(animeId);
  const { data: settings } = useSettings();
  const language = settings?.["app.titleLanguage"] ?? "romaji";

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  const title = preferredTitle(data?.anime, language, data?.settings?.displayTitle, animeId);

  return (
    <dialog
      ref={ref}
      className="anime-dialog"
      aria-labelledby="anime-dialog-title"
      onClose={onClose}
      onMouseDown={(e) => {
        // Close on a click on the backdrop.
        if (e.target === ref.current) ref.current?.close();
      }}
    >
      <header className="dialog-header">
        <h2 id="anime-dialog-title" className="title-ellipsis">
          {isLoading ? "Loading…" : title}
        </h2>
        <button className="icon-button" aria-label="Close" onClick={() => ref.current?.close()}>
          <Icon name="close" />
        </button>
      </header>
      <div className="tabs" role="tablist">
        {(
          [
            ["details", "Details"],
            ["entry", "My list"],
            ["settings", "Settings"],
          ] as const
        ).map(([id, label]) => (
          <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
      </div>
      <div className="dialog-body">
        {data && tab === "details" && <DetailsTab details={data} title={title} />}
        {data && tab === "entry" && (
          <EntryTab details={data} animeId={animeId} title={title} onDone={() => ref.current?.close()} />
        )}
        {data && tab === "settings" && <SettingsTab details={data} animeId={animeId} />}
      </div>
    </dialog>
  );
}

function DetailsTab({ details, title }: { details: AnimeDetails; title: string }) {
  const app = useApp();
  const { connected } = useCurrentAccount();
  const [refreshing, setRefreshing] = useState(false);
  const anime = details.anime;

  async function refresh() {
    if (!anime) return;
    setRefreshing(true);
    try {
      const updated = await api.refreshAnime(anime.id);
      queryClient.setQueryData(keys.anime(anime.id), updated);
      void queryClient.invalidateQueries({ queryKey: keys.list });
      if (!updated.anime) app.notify("This anime no longer exists on the service.");
    } catch (e) {
      app.notify(errorMessage(e));
    } finally {
      setRefreshing(false);
    }
  }

  if (!anime) {
    return <p className="empty">No details are stored for this anime yet.</p>;
  }
  const alternative = [anime.titles.english, anime.titles.japanese, ...anime.titles.synonyms].filter(
    (t) => t && t !== title,
  );
  const synopsis = plainText(anime.synopsis);

  return (
    <div className="details">
      <div className="details-side">
        <Poster anime={anime} title={title} className="poster large" />
        <div className="details-actions">
          <button
            className="primary"
            onClick={() => void playEpisode(app, anime.id, title, {})}
          >
            <Icon name="play" size={16} /> Play episode {nextEpisode(anime, details.entry)}
          </button>
          <button onClick={() => void openFolder(app, anime.id)}>
            <Icon name="folder" size={16} /> Open folder
          </button>
          {details.pageUrl && (
            <button onClick={() => void openUrl(details.pageUrl!)}>
              <Icon name="external" size={16} /> Open page
            </button>
          )}
          <button disabled={!connected || refreshing} onClick={() => void refresh()}>
            <Icon name="refresh" size={16} /> {refreshing ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </div>
      <div className="details-main">
        {alternative.length > 0 && <p className="muted alt-titles">{alternative.join(" · ")}</p>}
        <dl className="facts">
          <Fact label="Type" value={TYPE_LABELS[anime.type]} />
          <Fact label="Episodes" value={anime.episodeCount > 0 ? String(anime.episodeCount) : "Unknown"} />
          <Fact label="Duration" value={anime.episodeLength > 0 ? `${anime.episodeLength} min` : "Unknown"} />
          <Fact label="Status" value={AIRING_LABELS[anime.status]} />
          <Fact label="Season" value={formatSeason(anime.dateStarted)} />
          <Fact
            label="Aired"
            value={`${formatDate(anime.dateStarted)} – ${isEmptyDate(anime.dateFinished) ? "?" : formatDate(anime.dateFinished)}`}
          />
          {anime.nextEpisodeTime > 0 && (
            <Fact
              label="Next episode"
              value={`#${anime.lastAiredEpisode + 1}, ${formatTime(anime.nextEpisodeTime)}`}
            />
          )}
          <Fact label="Average score" value={anime.score ? anime.score.toFixed(2) : "—"} />
          <Fact label="Popularity" value={anime.popularityRank ? `#${anime.popularityRank}` : "—"} />
          {anime.studios.length > 0 && <Fact label="Studios" value={anime.studios.join(", ")} />}
        </dl>
        {anime.genres.length > 0 && (
          <div className="chips">
            {anime.genres.map((g) => (
              <span key={g} className="chip">
                {g}
              </span>
            ))}
          </div>
        )}
        {synopsis && <p className="synopsis">{synopsis}</p>}
      </div>
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}

function EntryTab({
  details,
  animeId,
  title,
  onDone,
}: {
  details: AnimeDetails;
  animeId: number;
  title: string;
  onDone: () => void;
}) {
  const app = useApp();
  const { scoreFormat } = useCurrentAccount();
  const save = useSaveEntry();
  const remove = useRemoveEntry();
  const original = details.entry;
  const [entry, setEntry] = useState<ListEntry>(() => original ?? newEntry(animeId, "PlanToWatch"));
  const total = details.anime?.episodeCount ?? 0;
  const set = (patch: Partial<ListEntry>) => setEntry((e) => ({ ...e, ...patch }));
  const changed = JSON.stringify(entry) !== JSON.stringify(original);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    try {
      await save.mutateAsync(entry);
      void queryClient.invalidateQueries({ queryKey: keys.anime(animeId) });
      app.notify(original ? "Saved." : "Added to your list.");
      onDone();
    } catch (error) {
      app.notify(errorMessage(error));
    }
  }

  async function removeEntry() {
    const ok = await confirmAction(`Remove “${title}” from your list?`, "Remove from list", "Remove");
    if (!ok) return;
    remove.mutate(animeId, {
      onSuccess: onDone,
      onError: (error) => app.notify(errorMessage(error)),
    });
  }

  return (
    <form className="form entry-form" onSubmit={submit}>
      {!original && <p className="callout">This anime isn't in your list. Saving adds it.</p>}
      {details.queued && (
        <p className="callout">
          Changes not yet sent to the service
          {details.queued.lastError && `: ${details.queued.lastError}`}
        </p>
      )}
      <div className="form-grid">
        <label htmlFor="entry-status">Status</label>
        <select
          id="entry-status"
          value={entry.status === "NotInList" ? "PlanToWatch" : entry.status}
          onChange={(e) => setEntry(withStatus(entry, e.target.value as ListEntry["status"], total))}
        >
          {LIST_STATUSES.map((s) => (
            <option key={s} value={s}>
              {LIST_STATUS_LABELS[s]}
            </option>
          ))}
        </select>

        <label htmlFor="entry-episodes">Episodes watched</label>
        <span className="inline">
          <input
            id="entry-episodes"
            type="number"
            min={0}
            max={total > 0 ? total : undefined}
            value={entry.watchedEpisodes}
            onChange={(e) => {
              const value = Math.max(0, Number.parseInt(e.target.value, 10) || 0);
              set({ watchedEpisodes: total > 0 ? Math.min(value, total) : value });
            }}
          />
          <span className="muted">of {total > 0 ? total : "?"}</span>
        </span>

        <label htmlFor="entry-score">Score</label>
        <ScoreSelect id="entry-score" value={entry.score} format={scoreFormat} onChange={(score) => set({ score })} />

        <label htmlFor="entry-rewatching">Rewatching</label>
        <span className="inline">
          <input
            id="entry-rewatching"
            type="checkbox"
            checked={entry.rewatching}
            onChange={(e) => set({ rewatching: e.target.checked })}
          />
          <label htmlFor="entry-rewatches" className="muted">
            Rewatched
          </label>
          <input
            id="entry-rewatches"
            type="number"
            min={0}
            className="narrow"
            value={entry.rewatchedTimes}
            onChange={(e) => set({ rewatchedTimes: Math.max(0, Number.parseInt(e.target.value, 10) || 0) })}
          />
          <span className="muted">times</span>
        </span>

        <label htmlFor="entry-started">Started</label>
        <FuzzyDateInput id="entry-started" value={entry.dateStarted} onChange={(dateStarted) => set({ dateStarted })} />

        <label htmlFor="entry-completed">Completed</label>
        <FuzzyDateInput
          id="entry-completed"
          value={entry.dateCompleted}
          onChange={(dateCompleted) => set({ dateCompleted })}
        />

        <label htmlFor="entry-notes">Notes</label>
        <textarea id="entry-notes" rows={3} value={entry.notes} onChange={(e) => set({ notes: e.target.value })} />
      </div>
      <footer className="dialog-footer">
        {original && (
          <button type="button" className="danger" onClick={() => void removeEntry()}>
            Remove from list
          </button>
        )}
        <span className="footer-spacer" />
        {original && original.lastUpdated > 0 && (
          <span className="muted small">Updated {formatTime(original.lastUpdated)}</span>
        )}
        <button type="submit" className="primary" disabled={(!changed && !!original) || save.isPending}>
          {original ? "Save" : "Add to list"}
        </button>
      </footer>
    </form>
  );
}

function SettingsTab({ details, animeId }: { details: AnimeDetails; animeId: number }) {
  const app = useApp();
  const [settings, setSettings] = useState<AnimeSettings>(
    () => details.settings ?? { id: animeId, displayTitle: "", folder: "", synonyms: [] },
  );
  const [synonyms, setSynonyms] = useState(settings.synonyms.join("\n"));
  const [saving, setSaving] = useState(false);
  const titles = titleChoices(details.anime);

  async function browse() {
    try {
      const folder = await openDialog({ directory: true, title: "Select anime folder" });
      if (typeof folder === "string") setSettings((s) => ({ ...s, folder }));
    } catch (e) {
      app.notify(errorMessage(e));
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    try {
      await api.saveAnimeSettings({
        ...settings,
        synonyms: synonyms
          .split("\n")
          .map((s) => s.trim())
          .filter(Boolean),
      });
      void queryClient.invalidateQueries({ queryKey: keys.anime(animeId) });
      void queryClient.invalidateQueries({ queryKey: keys.list });
      void queryClient.invalidateQueries({ queryKey: keys.history });
      app.notify("Saved.");
    } catch (error) {
      app.notify(errorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form className="form" onSubmit={submit}>
      <div className="form-grid">
        <label htmlFor="settings-title">Display title</label>
        <span className="inline">
          <input
            id="settings-title"
            list="settings-title-choices"
            placeholder="Default (from title language)"
            value={settings.displayTitle}
            onChange={(e) => setSettings((s) => ({ ...s, displayTitle: e.target.value }))}
          />
          <datalist id="settings-title-choices">
            {titles.map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
        </span>

        <label htmlFor="settings-folder">Folder</label>
        <span className="inline">
          <input
            id="settings-folder"
            placeholder="Found automatically in library folders"
            value={settings.folder}
            onChange={(e) => setSettings((s) => ({ ...s, folder: e.target.value }))}
          />
          <button type="button" onClick={() => void browse()}>
            Browse…
          </button>
        </span>

        <label htmlFor="settings-synonyms">Alternative titles</label>
        <span>
          <textarea
            id="settings-synonyms"
            rows={4}
            placeholder="One per line"
            value={synonyms}
            onChange={(e) => setSynonyms(e.target.value)}
          />
          <small className="muted">Extra titles used to recognize this anime in file names.</small>
        </span>
      </div>
      <footer className="dialog-footer">
        <span className="footer-spacer" />
        <button type="submit" className="primary" disabled={saving}>
          Save
        </button>
      </footer>
    </form>
  );
}

function titleChoices(anime: Anime | null): string[] {
  if (!anime) return [];
  const t = anime.titles;
  return [...new Set([t.romaji, t.english, t.japanese, ...t.synonyms].filter(Boolean))];
}
