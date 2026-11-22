import { useMutation } from "@tanstack/react-query";
import { api, errorMessage, type Suggestion } from "../api";
import { SuggestionCard, airingText } from "../components/SuggestionCard";
import { useApp } from "../lib/context";
import { isNsfw } from "../lib/format";
import { keys, queryClient, useSequels, useSettings } from "../lib/queries";

const GROUPS: { label: string; test: (s: Suggestion) => boolean }[] = [
  { label: "Airing now", test: (s) => s.anime.status === "Airing" },
  { label: "Coming soon", test: (s) => s.anime.status === "NotYetAired" },
  { label: "Already out", test: (s) => s.anime.status !== "Airing" && s.anime.status !== "NotYetAired" },
];

export function SequelsPage() {
  const app = useApp();
  const { data = [], isLoading } = useSequels();
  const hideNsfw = useSettings().data?.["app.hideNsfw"] ?? true;
  const sequels = data.filter((s) => !(hideNsfw && isNsfw(s.anime)));
  const refresh = useMutation({
    mutationFn: api.discoverRefresh,
    onSuccess: (count) => app.notify(count ? `Checked ${count} anime.` : "Nothing to check. Log in first."),
    onError: (e) => app.notify(errorMessage(e)),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.discover }),
  });

  return (
    <div className="page">
      <header className="page-header">
        <h1>Sequels</h1>
        <div className="page-actions">
          <button disabled={refresh.isPending} onClick={() => refresh.mutate()}>
            {refresh.isPending ? "Checking…" : "Check again"}
          </button>
        </div>
      </header>
      <p className="muted">Sequels of anime you've completed that aren't in your list.</p>

      {isLoading ? (
        <p className="empty">Loading…</p>
      ) : sequels.length === 0 ? (
        <p className="empty">
          No sequels missing. Your completed anime are checked in the background, which takes a while after you first
          log in.
        </p>
      ) : (
        GROUPS.map((group) => {
          const items = sequels.filter(group.test);
          if (items.length === 0) return null;
          return (
            <section key={group.label}>
              <h2 className="section-title">
                {group.label} <span className="count">{items.length}</span>
              </h2>
              <div className="suggest-grid">
                {items.map((s) => (
                  <SuggestionCard
                    key={s.anime.id}
                    anime={s.anime}
                    detail={[airingText(s.anime), `after ${s.becauseTitle}`].filter(Boolean).join(" · ")}
                  />
                ))}
              </div>
            </section>
          );
        })
      )}
    </div>
  );
}
