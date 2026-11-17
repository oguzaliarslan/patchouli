import { useEffect, useState } from "react";
import { api, type AppInfo } from "./api";

type Page = "home" | "search" | "list" | "history" | "library";

const PAGES: { id: Page; label: string; phase: string }[] = [
  { id: "home", label: "Now Playing", phase: "Phase 5" },
  { id: "list", label: "Anime List", phase: "Phase 3" },
  { id: "search", label: "Search", phase: "Phase 3" },
  { id: "history", label: "History", phase: "Phase 3" },
  { id: "library", label: "Library", phase: "Phase 4" },
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.appInfo().then(setInfo).catch((e) => setError(String(e)));
  }, []);

  const current = PAGES.find((p) => p.id === page)!;

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
        <p className="muted">Not implemented yet ({current.phase}).</p>

        {page === "home" && (
          <section className="card">
            <h2>Backend</h2>
            {error && <p className="error">{error}</p>}
            {!info && !error && <p className="muted">Connecting…</p>}
            {info && (
              <dl>
                <dt>Version</dt>
                <dd>
                  {info.name} {info.version}
                </dd>
                <dt>Data folder</dt>
                <dd>
                  <code>{info.dataDir}</code>
                </dd>
                <dt>Database tables</dt>
                <dd>{info.tables.join(", ")}</dd>
              </dl>
            )}
          </section>
        )}
      </main>

      <footer className="statusbar">
        <span>Sync: not connected</span>
        <span>Detection: off</span>
      </footer>
    </div>
  );
}
