import { api, errorMessage, newEntry, type Anime } from "../api";
import { useApp } from "../lib/context";
import { formatDate, formatSeason, isEmptyDate, preferredTitle } from "../lib/format";
import { keys, queryClient, useSaveEntry, useSettings } from "../lib/queries";
import { Icon } from "./Icon";
import { Poster } from "./Poster";

/** "Airing now", "Starts Jan 4, 2027", "Winter 2024"… */
export function airingText(anime: Anime): string {
  if (anime.status === "Airing") return "Airing now";
  if (anime.status === "NotYetAired") {
    return isEmptyDate(anime.dateStarted) ? "Coming soon" : `Starts ${formatDate(anime.dateStarted)}`;
  }
  return isEmptyDate(anime.dateStarted) ? "" : formatSeason(anime.dateStarted);
}

/** A suggested anime with buttons to add it to Plan to watch or hide it. */
export function SuggestionCard({ anime, detail }: { anime: Anime; detail: string }) {
  const app = useApp();
  const save = useSaveEntry();
  const language = useSettings().data?.["app.titleLanguage"] ?? "romaji";
  const title = preferredTitle(anime, language);

  function add() {
    save.mutate(newEntry(anime.id), {
      onSuccess: () => app.notify(`Added ${title} to Plan to watch.`),
      onError: (e) => app.notify(errorMessage(e)),
    });
  }

  async function dismiss() {
    try {
      await api.dismissSuggestion(anime.id);
      void queryClient.invalidateQueries({ queryKey: keys.discover });
    } catch (e) {
      app.notify(errorMessage(e));
    }
  }

  return (
    <div className="suggest-card">
      <button className="suggest-open" onClick={() => app.openAnime(anime.id)}>
        <Poster anime={anime} title={title} />
        <span className="anime-card-title title-clamp">{title}</span>
        <span className="muted small title-clamp">{detail}</span>
      </button>
      <span className="suggest-actions">
        <button disabled={save.isPending} onClick={add}>
          <Icon name="plus" size={16} /> Plan to watch
        </button>
        <button
          className="icon-button"
          aria-label={`Don't suggest ${title}`}
          title="Don't suggest this"
          onClick={() => void dismiss()}
        >
          <Icon name="close" size={16} />
        </button>
      </span>
    </div>
  );
}
