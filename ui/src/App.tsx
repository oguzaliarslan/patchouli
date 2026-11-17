import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimeDialog } from "./components/AnimeDialog";
import { ContextMenu, type MenuState } from "./components/ContextMenu";
import { Icon, type IconName } from "./components/Icon";
import { AppContext, type AppActions, type Page } from "./lib/context";
import { usePref } from "./lib/prefs";
import { useBackendEvents, useCurrentAccount, useQueue, useSettings, useTracking } from "./lib/queries";
import { useColorScheme } from "./lib/theme";
import { HistoryPage } from "./pages/HistoryPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { ListPage } from "./pages/ListPage";
import { ProfilePage } from "./pages/ProfilePage";
import { SearchPage } from "./pages/SearchPage";
import { SeasonsPage } from "./pages/SeasonsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TorrentsPage } from "./pages/TorrentsPage";

const NAV: { id: Page; label: string; icon: IconName }[] = [
  { id: "home", label: "Home", icon: "home" },
  { id: "list", label: "Anime List", icon: "list" },
  { id: "search", label: "Search", icon: "search" },
  { id: "seasons", label: "Seasons", icon: "calendar" },
  { id: "history", label: "History", icon: "history" },
  { id: "library", label: "Library", icon: "folder" },
  { id: "torrents", label: "Torrents", icon: "download" },
  { id: "profile", label: "Profile", icon: "person" },
];

export default function App() {
  const [page, setPage] = usePref<Page>("page", "home");
  const [dialog, setDialog] = useState<{ id: number; tab: "details" | "entry" | "settings" } | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  const { data: settings } = useSettings();
  useColorScheme(settings?.["app.colorScheme"]);
  const sync = useBackendEvents();

  const notify = useCallback((message: string) => {
    setToast(message);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3500);
  }, []);

  const actions = useMemo<AppActions>(
    () => ({
      navigate: setPage,
      openAnime: (id, tab = "details") => setDialog({ id, tab }),
      showMenu: setMenu,
      notify,
    }),
    [setPage, notify],
  );

  // Block the browser's own context menu outside inputs.
  useEffect(() => {
    const block = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest("input, textarea, [contenteditable]")) e.preventDefault();
    };
    window.addEventListener("contextmenu", block);
    return () => window.removeEventListener("contextmenu", block);
  }, []);

  return (
    <AppContext.Provider value={actions}>
      <div className="shell">
        <nav className="sidebar" aria-label="Main">
          <div className="brand">Patchouli</div>
          {NAV.map((item) => (
            <NavButton key={item.id} item={item} active={page === item.id} onClick={() => setPage(item.id)} />
          ))}
          <div className="sidebar-spacer" />
          <NavButton
            item={{ id: "settings", label: "Settings", icon: "settings" }}
            active={page === "settings"}
            onClick={() => setPage("settings")}
          />
        </nav>

        <main className="content">
          {page === "home" && <HomePage sync={sync} />}
          {page === "list" && <ListPage />}
          {page === "search" && <SearchPage />}
          {page === "history" && <HistoryPage />}
          {page === "library" && <LibraryPage />}
          {page === "seasons" && <SeasonsPage />}
          {page === "torrents" && <TorrentsPage />}
          {page === "profile" && <ProfilePage />}
          {page === "settings" && <SettingsPage />}
        </main>

        <StatusBar sync={sync} onOpenAccounts={() => setPage("settings")} />
      </div>

      {dialog && (
        <AnimeDialog
          key={dialog.id}
          animeId={dialog.id}
          initialTab={dialog.tab}
          onClose={() => setDialog(null)}
        />
      )}
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
    </AppContext.Provider>
  );
}

function DetectionStatus() {
  const { data: settings } = useSettings();
  const { data: tracking } = useTracking();
  if (!(settings?.["track.detection.enabled"] ?? true)) {
    return <span className="muted">Detection: off</span>;
  }
  const playing = tracking?.nowPlaying;
  const update = tracking?.update;
  if (!playing) return <span className="muted">Detection: on</span>;
  const episode = playing.episodes.length ? ` #${playing.episodes.join("–")}` : "";
  const suffix =
    update?.phase === "countdown"
      ? ` · updating in ${update.remainingSecs}s`
      : update?.phase === "committed"
        ? " · updated"
        : update?.phase === "confirming"
          ? " · waiting for you"
          : "";
  return (
    <span className="status-playing" title={playing.file ?? playing.title}>
      ▶ {playing.title || playing.player}
      {episode}
      {suffix}
    </span>
  );
}

function NavButton({
  item,
  active,
  onClick,
}: {
  item: { id: Page; label: string; icon: IconName };
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button className={active ? "nav-item active" : "nav-item"} aria-current={active ? "page" : undefined} onClick={onClick}>
      <Icon name={item.icon} />
      <span>{item.label}</span>
    </button>
  );
}

function StatusBar({
  sync,
  onOpenAccounts,
}: {
  sync: ReturnType<typeof useBackendEvents>;
  onOpenAccounts: () => void;
}) {
  const { connected, account } = useCurrentAccount();
  const { data: queue } = useQueue();
  const pending = queue?.length ?? 0;

  return (
    <footer className="statusbar">
      <span className={sync.busy ? "status-sync busy" : "status-sync"}>
        <Icon name="sync" size={14} />
        {sync.busy
          ? "Syncing…"
          : connected
            ? `${connected.name}: ${connected.username}`
            : account?.username
              ? `${account.name}: not connected`
              : "Not logged in"}
      </span>
      {pending > 0 && (
        <span>
          {pending} unsent change{pending === 1 ? "" : "s"}
        </span>
      )}
      {sync.error && (
        <span className="status-error" title={sync.error}>
          {sync.error}
          {sync.unauthorized && (
            <button className="link" onClick={onOpenAccounts}>
              Log in again
            </button>
          )}
          <button className="link" onClick={sync.clearError}>
            Dismiss
          </button>
        </span>
      )}
      <span className="statusbar-spacer" />
      <DetectionStatus />
    </footer>
  );
}
