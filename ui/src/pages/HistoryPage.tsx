import { useMutation } from "@tanstack/react-query";
import { useMemo } from "react";
import { api, errorMessage, type HistoryItem } from "../api";
import { Icon } from "../components/Icon";
import { useApp } from "../lib/context";
import { preferredTitle } from "../lib/format";
import { keys, queryClient, useHistory, useList, useSettings } from "../lib/queries";
import { confirmAction, useAnimeMenu } from "../lib/useAnimeMenu";

function dayLabel(seconds: number): string {
  const date = new Date(seconds * 1000);
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString(undefined, { weekday: "long", year: "numeric", month: "long", day: "numeric" });
}

export function HistoryPage() {
  const app = useApp();
  const { data, isLoading } = useHistory();
  const { data: settings } = useSettings();
  const { data: list = [] } = useList();
  const openMenu = useAnimeMenu();
  const language = settings?.["app.titleLanguage"] ?? "romaji";

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: keys.history });
    void queryClient.invalidateQueries({ queryKey: keys.info });
  };
  const remove = useMutation({ mutationFn: api.removeHistory, onSettled: refresh });
  const clear = useMutation({ mutationFn: api.clearHistory, onSettled: refresh });

  const groups = useMemo(() => {
    const result: { label: string; items: HistoryItem[] }[] = [];
    for (const item of data?.items ?? []) {
      const label = dayLabel(item.time);
      const last = result.at(-1);
      if (last?.label === label) last.items.push(item);
      else result.push({ label, items: [item] });
    }
    return result;
  }, [data]);

  const entries = useMemo(() => new Map(list.map((i) => [i.entry.animeId, i.entry])), [list]);
  const titleOf = (id: number) =>
    preferredTitle(data?.anime[id], language, data?.displayTitles[id], id);

  async function clearAll() {
    const ok = await confirmAction(
      "Clear your whole watch history? Your list is not affected.",
      "Clear history",
      "Clear",
    );
    if (ok) clear.mutate(undefined, { onError: (e) => app.notify(errorMessage(e)) });
  }

  return (
    <div className="page">
      <header className="page-header">
        <h1>History</h1>
        <div className="page-actions">
          <button disabled={!data?.items.length} onClick={() => void clearAll()}>
            Clear history
          </button>
        </div>
      </header>

      {isLoading ? (
        <p className="empty">Loading…</p>
      ) : groups.length === 0 ? (
        <p className="empty">Episodes you mark as watched appear here.</p>
      ) : (
        <div className="history">
          {groups.map((group) => (
            <section key={group.label}>
              <h2 className="section-title">{group.label}</h2>
              <ul className="history-list">
                {group.items.map((item) => {
                  const title = titleOf(item.animeId);
                  return (
                    <li
                      key={item.id}
                      onDoubleClick={() => app.openAnime(item.animeId)}
                      onContextMenu={(e) =>
                        openMenu(e, {
                          animeId: item.animeId,
                          anime: data?.anime[item.animeId],
                          entry: entries.get(item.animeId),
                          title,
                        })
                      }
                    >
                      <span className="history-time muted small">
                        {new Date(item.time * 1000).toLocaleTimeString(undefined, {
                          hour: "2-digit",
                          minute: "2-digit",
                        })}
                      </span>
                      <button className="link history-title" onClick={() => app.openAnime(item.animeId)}>
                        {title}
                      </button>
                      <span className="muted">Episode {item.episode}</span>
                      <button
                        className="icon-button"
                        aria-label={`Remove episode ${item.episode} of ${title} from history`}
                        onClick={() => remove.mutate(item.id)}
                      >
                        <Icon name="close" size={16} />
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
