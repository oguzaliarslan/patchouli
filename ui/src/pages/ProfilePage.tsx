import { openUrl } from "@tauri-apps/plugin-opener";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { api, type ListStatus } from "../api";
import { WatchYear } from "../components/WatchYear";
import { useApp } from "../lib/context";
import { LIST_STATUSES, LIST_STATUS_LABELS, TYPE_LABELS, formatScore } from "../lib/format";
import { keys, queryClient, useCurrentAccount, useHistory, useList, useSettings } from "../lib/queries";

/** `3 days 4 hours` from seconds (omitting seconds). */
export function formatDuration(seconds: number): string {
  if (seconds < 60) return "None";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const part = (n: number, unit: string) => (n > 0 ? `${n} ${unit}${n === 1 ? "" : "s"}` : "");
  return [part(days, "day"), part(hours, "hour"), days ? "" : part(minutes, "minute")].filter(Boolean).join(" ");
}

const MONTH = new Intl.DateTimeFormat(undefined, { month: "short" });
const MONTH_LONG = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });

/**
 * One series of vertical bars, with the value on hover and in the
 * accessible label. Every bar is also listed as text by `label`.
 */
function BarChart({
  bars,
  label,
}: {
  bars: { key: string; label: string; value: number; tooltip: string }[];
  label: string;
}) {
  const max = Math.max(1, ...bars.map((b) => b.value));
  return (
    <figure className="bar-chart" aria-label={label}>
      <div className="bars" role="list">
        {bars.map((bar) => (
          <div key={bar.key} className="bar-slot" role="listitem" title={bar.tooltip} aria-label={bar.tooltip}>
            <span className="bar-value">{bar.value || ""}</span>
            <span className="bar" style={{ height: `${(bar.value / max) * 100}%` }} />
            <span className="bar-label">{bar.label}</span>
          </div>
        ))}
      </div>
    </figure>
  );
}

/** A horizontal proportion row: label, bar, count. */
function Share({ label, value, total }: { label: string; value: number; total: number }) {
  const percent = total ? (value / total) * 100 : 0;
  return (
    <div className="share" title={`${label}: ${value} (${percent.toFixed(0)}%)`}>
      <span className="share-label">{label}</span>
      <span className="share-track">
        <span style={{ width: `${percent}%` }} />
      </span>
      <span className="share-value">{value}</span>
    </div>
  );
}

export function ProfilePage() {
  const app = useApp();
  const { account, connected } = useCurrentAccount();
  const { data: stats } = useQuery({ queryKey: ["stats"], queryFn: api.stats, staleTime: 0 });
  const refresh = useMutation({
    mutationFn: api.refreshProfile,
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.accounts }),
  });

  // Accounts saved before pictures were stored have none; fetch it once.
  useEffect(() => {
    if (connected && !account?.avatarUrl && refresh.isIdle) refresh.mutate();
  }, [connected, account?.avatarUrl, refresh]);

  const { data: history } = useHistory();
  const { data: list = [] } = useList();
  const language = useSettings().data?.["app.titleLanguage"] ?? "romaji";
  const format = account?.scoreFormat ?? null;
  const scored = stats?.scoreCounts.reduce((a, b) => a + b, 0) ?? 0;

  return (
    <div className="page">
      <header className="page-header profile-header">
        {account?.avatarUrl ? (
          <img className="avatar" src={account.avatarUrl} alt="" />
        ) : (
          <span className="avatar placeholder" aria-hidden="true">
            {(account?.username ?? "?").slice(0, 1).toUpperCase()}
          </span>
        )}
        <div>
          <h1>{account?.username ?? "Profile"}</h1>
          <span className="muted">
            {account?.username ? `${account.name}${connected ? "" : " · not connected"}` : "Not logged in"}
          </span>
        </div>
        <div className="page-actions">
          {account?.profileUrl && (
            <button onClick={() => void openUrl(account.profileUrl!)}>Open profile on {account.name}</button>
          )}
          {!account?.username && <button onClick={() => app.navigate("settings")}>Log in</button>}
        </div>
      </header>

      {!stats ? (
        <p className="empty">Loading…</p>
      ) : stats.animeCount === 0 ? (
        <p className="empty">Statistics appear once your list has anime.</p>
      ) : (
        <>
          <div className="stat-tiles">
            <div className="stat-tile">
              <span className="stat-value">{stats.animeCount}</span>
              <span className="muted small">Anime in list</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{stats.episodeCount.toLocaleString()}</span>
              <span className="muted small">Episodes watched</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{formatDuration(stats.secondsWatched)}</span>
              <span className="muted small">Time watched</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{formatDuration(stats.secondsPlanned)}</span>
              <span className="muted small">Left to watch</span>
            </div>
            <div className="stat-tile">
              <span className="stat-value">{scored ? formatScore(Math.round(stats.meanScore), format) : "—"}</span>
              <span className="muted small">
                Mean score{scored ? ` · ±${(stats.scoreDeviation / 10).toFixed(1)}` : ""}
              </span>
            </div>
          </div>

          <div className="profile-grid">
            <section>
              <h2 className="section-title">Scores</h2>
              {scored === 0 ? (
                <p className="muted small">No scored anime yet.</p>
              ) : (
                <BarChart
                  label="Number of anime by score"
                  bars={stats.scoreCounts.slice(1).map((count, i) => {
                    const score = (i + 1) * 10;
                    return {
                      key: String(score),
                      label: formatScore(score, format === "POINT_100" ? "POINT_10" : format),
                      value: count,
                      tooltip: `Score ${formatScore(score, format)}: ${count} anime`,
                    };
                  })}
                />
              )}
            </section>

            <section>
              <h2 className="section-title">Episodes watched per month</h2>
              {stats.historyByMonth.length === 0 ? (
                <p className="muted small">Watch history appears here.</p>
              ) : (
                <BarChart
                  label="Episodes watched per month"
                  bars={stats.historyByMonth.map(([month, count]) => {
                    const date = new Date(`${month}-01T00:00:00`);
                    return {
                      key: month,
                      label: MONTH.format(date),
                      value: count,
                      tooltip: `${MONTH_LONG.format(date)}: ${count} episode${count === 1 ? "" : "s"}`,
                    };
                  })}
                />
              )}
            </section>

            <section>
              <h2 className="section-title">By status</h2>
              {LIST_STATUSES.map((s: ListStatus) => (
                <Share key={s} label={LIST_STATUS_LABELS[s]} value={stats.statusCounts[s] ?? 0} total={stats.animeCount} />
              ))}
            </section>

            <section>
              <h2 className="section-title">By type</h2>
              {Object.entries(stats.typeCounts)
                .sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0))
                .map(([type, count]) => (
                  <Share key={type} label={TYPE_LABELS[type] ?? type} value={count ?? 0} total={stats.animeCount} />
                ))}
            </section>

            {stats.topGenres.length > 0 && (
              <section>
                <h2 className="section-title">Top genres</h2>
                {stats.topGenres.map(([genre, count]) => (
                  <Share key={genre} label={genre} value={count} total={stats.animeCount} />
                ))}
              </section>
            )}
          </div>

          {history && <WatchYear history={history} list={list} language={language} />}
        </>
      )}
    </div>
  );
}
