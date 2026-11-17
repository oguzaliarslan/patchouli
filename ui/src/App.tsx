import { useCallback, useEffect, useState } from "react";
import {
  api,
  errorMessage,
  type AccountStatus,
  type AppInfo,
  type CommandError,
  type ListItem,
  type QueueItem,
} from "./api";

type Page = "home" | "search" | "list" | "history" | "library";

const PAGES: { id: Page; label: string; phase?: string }[] = [
  { id: "home", label: "Home" },
  { id: "list", label: "Anime List" },
  { id: "search", label: "Search", phase: "Phase 3" },
  { id: "history", label: "History", phase: "Phase 3" },
  { id: "library", label: "Library", phase: "Phase 4" },
];

const STATUS_LABELS: Record<string, string> = {
  Watching: "Watching",
  Completed: "Completed",
  OnHold: "On hold",
  Dropped: "Dropped",
  PlanToWatch: "Plan to watch",
  NotInList: "—",
};

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [accounts, setAccounts] = useState<AccountStatus[]>([]);
  const [items, setItems] = useState<ListItem[]>([]);
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [loggingIn, setLoggingIn] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api.appInfo().then(setInfo).catch((e) => setError(errorMessage(e)));
    api.accounts().then(setAccounts).catch((e) => setError(errorMessage(e)));
    api.list().then(setItems).catch((e) => setError(errorMessage(e)));
    api.queue().then(setQueue).catch((e) => setError(errorMessage(e)));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = api.onSync((event) => {
      if (event.type === "busy") setBusy(event.busy);
      else if (event.type === "error") setError(event.message);
      else refresh();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [refresh]);

  async function login(account: AccountStatus) {
    setError(null);
    setLoggingIn(account.service);
    try {
      try {
        await api.login(account.service);
      } catch (e) {
        if ((e as CommandError).code !== "replace-local-data") throw e;
        if (!window.confirm(`${(e as CommandError).message}\n\nContinue?`)) return;
        await api.login(account.service, true);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoggingIn(null);
      refresh();
    }
  }

  async function logout(account: AccountStatus) {
    setError(null);
    await api.logout(account.service).catch((e) => setError(errorMessage(e)));
    refresh();
  }

  async function syncNow() {
    setError(null);
    setBusy(true);
    try {
      await api.syncNow();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      refresh();
    }
  }

  async function addEpisode(item: ListItem) {
    const total = item.anime?.episodeCount ?? 0;
    const next = item.entry.watchedEpisodes + 1;
    if (total > 0 && next > total) return;
    await api
      .saveEntry({ ...item.entry, watchedEpisodes: next })
      .catch((e) => setError(errorMessage(e)));
  }

  const current = PAGES.find((p) => p.id === page)!;
  const connected = accounts.find((a) => a.connected);

  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="brand">Tracker</div>
        {PAGES.map((p) => (
          <button
            key={p.id}
            className={p.id === page ? "nav-item active" : "nav-item"}
            onClick={() => setPage(p.id)}
          >
            {p.label}
          </button>
        ))}
      </nav>

      <main className="content">
        <h1>{current.label}</h1>
        {current.phase && <p className="muted">Not implemented yet ({current.phase}).</p>}
        {error && (
          <p className="error" role="alert">
            {error}{" "}
            <button className="link" onClick={() => setError(null)}>
              Dismiss
            </button>
          </p>
        )}

        {page === "home" && (
          <>
            <section className="card">
              <h2>Accounts</h2>
              <table className="table">
                <tbody>
                  {accounts.map((a) => (
                    <tr key={a.service}>
                      <td>
                        {a.name}
                        {a.current && <span className="badge">current</span>}
                      </td>
                      <td className="muted">
                        {!a.configured
                          ? "No client ID configured"
                          : a.username
                            ? `Logged in as ${a.username}`
                            : "Not logged in"}
                      </td>
                      <td className="actions">
                        {a.username ? (
                          <button onClick={() => logout(a)}>Log out</button>
                        ) : (
                          <button
                            disabled={!a.configured || loggingIn !== null}
                            onClick={() => login(a)}
                          >
                            {loggingIn === a.service ? "Waiting for browser…" : "Log in"}
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="row">
                <button disabled={!connected || busy} onClick={syncNow}>
                  {busy ? "Syncing…" : "Sync now"}
                </button>
                <span className="muted">
                  {queue.length === 0
                    ? "No unsent changes"
                    : `${queue.length} unsent change${queue.length === 1 ? "" : "s"}`}
                </span>
              </div>
              {queue.some((q) => q.lastError) && (
                <ul className="muted small">
                  {queue
                    .filter((q) => q.lastError)
                    .map((q) => (
                      <li key={q.animeId}>
                        #{q.animeId}: {q.lastError} (tried {q.retryCount}×)
                      </li>
                    ))}
                </ul>
              )}
            </section>

            {info && (
              <section className="card">
                <h2>Storage</h2>
                <dl>
                  <dt>Version</dt>
                  <dd>
                    {info.name} {info.version}
                  </dd>
                  <dt>Data folder</dt>
                  <dd>
                    <code>{info.dataDir}</code>
                    {info.portable && " (portable)"}
                  </dd>
                  <dt>Anime / entries</dt>
                  <dd>
                    {info.animeCount} / {info.listCount}
                  </dd>
                  <dt>History</dt>
                  <dd>{info.historyCount}</dd>
                </dl>
              </section>
            )}
          </>
        )}

        {page === "list" && (
          <section className="card wide">
            {items.length === 0 ? (
              <p className="muted">Your list is empty. Log in and sync from Home.</p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th>Title</th>
                    <th>Progress</th>
                    <th>Score</th>
                    <th>Status</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => {
                    const queued = queue.some((q) => q.animeId === item.entry.animeId);
                    return (
                      <tr key={item.entry.animeId}>
                        <td>
                          {item.anime?.titles.romaji || `#${item.entry.animeId}`}
                          {queued && <span className="badge">unsent</span>}
                        </td>
                        <td>
                          {item.entry.watchedEpisodes}/{item.anime?.episodeCount || "?"}
                        </td>
                        <td>{item.entry.score ? item.entry.score / 10 : "—"}</td>
                        <td>{STATUS_LABELS[item.entry.status] ?? item.entry.status}</td>
                        <td className="actions">
                          <button onClick={() => addEpisode(item)} title="Watched next episode">
                            +1
                          </button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </section>
        )}
      </main>

      <footer className="statusbar">
        <span>
          Sync:{" "}
          {busy
            ? "syncing…"
            : connected
              ? `${connected.name} (${connected.username})`
              : "not connected"}
        </span>
        <span>Detection: off</span>
      </footer>
    </div>
  );
}
