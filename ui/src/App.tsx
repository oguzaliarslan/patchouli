import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimeDialog } from "./components/AnimeDialog";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenu, type MenuState } from "./components/ContextMenu";
import { Icon, type IconName } from "./components/Icon";
import { AppContext, type AppActions, type Page } from "./lib/context";
import { usePref } from "./lib/prefs";
import {
  useBackendEvents,
  useCurrentAccount,
  useList,
  useQueue,
  useSettings,
  useTorrents,
  useTracking,
} from "./lib/queries";
import { useColorScheme } from "./lib/theme";
import { HistoryPage } from "./pages/HistoryPage";
import { HomePage } from "./pages/HomePage";
import { LibraryPage } from "./pages/LibraryPage";
import { ListPage } from "./pages/ListPage";
import { ComparePage } from "./pages/ComparePage";
import { ProfilePage } from "./pages/ProfilePage";
import { SchedulePage } from "./pages/SchedulePage";
import { SearchPage } from "./pages/SearchPage";
import { SeasonsPage } from "./pages/SeasonsPage";
import { SequelsPage } from "./pages/SequelsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TorrentsPage } from "./pages/TorrentsPage";

type NavItem = { id: Page; label: string; icon: IconName };

const NAV: { label: string; items: NavItem[] }[] = [
  {
    label: "Tracking",
    items: [
      { id: "home", label: "Home", icon: "home" },
      { id: "list", label: "Anime List", icon: "list" },
      { id: "schedule", label: "Schedule", icon: "clock" },
      { id: "history", label: "History", icon: "history" },
    ],
  },
  {
    label: "Discover & local",
    items: [
      { id: "search", label: "Search", icon: "search" },
      { id: "seasons", label: "Seasons", icon: "calendar" },
      { id: "sequels", label: "Sequels", icon: "layers" },
      { id: "library", label: "Library", icon: "folder" },
      { id: "torrents", label: "Torrents", icon: "download" },
    ],
  },
  {
    label: "Account",
    items: [
      { id: "profile", label: "Profile", icon: "person" },
      { id: "compare", label: "Compare lists", icon: "compare" },
    ],
  },
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
  const { data: list } = useList();
  const syncNow = useCallback(() => void sync.syncNow(), [sync]);
  const { data: torrents } = useTorrents();
  const counts: Partial<Record<Page, number>> = {
    list: list?.length,
    torrents: torrents?.items.filter((i) => i.state === "selected").length,
  };

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
          <div className="brand">
            <span className="brand-mark" aria-hidden="true">
              P
            </span>
            Patchouli
          </div>
          {NAV.map((group) => (
            <div key={group.label} className="nav-group" role="group" aria-label={group.label}>
              <span className="nav-group-label" aria-hidden="true">
                {group.label}
              </span>
              {group.items.map((item) => (
                <NavButton
                  key={item.id}
                  item={item}
                  count={counts[item.id]}
                  active={page === item.id}
                  onClick={() => setPage(item.id)}
                />
              ))}
            </div>
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
          {page === "schedule" && <SchedulePage />}
          {page === "search" && <SearchPage />}
          {page === "history" && <HistoryPage />}
          {page === "library" && <LibraryPage />}
          {page === "seasons" && <SeasonsPage />}
          {page === "sequels" && <SequelsPage />}
          {page === "torrents" && <TorrentsPage />}
          {page === "profile" && <ProfilePage />}
          {page === "compare" && <ComparePage />}
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
      <CommandPalette onSyncNow={syncNow} />
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
      ? update.byPosition
        ? ` · ${update.positionPercent}% of ${update.watchedAtPercent}%`
        : ` · updating in ${update.remainingSecs}s`
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
  count,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  count?: number;
  onClick: () => void;
}) {
  return (
    <button className={active ? "nav-item active" : "nav-item"} aria-current={active ? "page" : undefined} onClick={onClick}>
      <Icon name={item.icon} />
      <span>{item.label}</span>
      {!!count && <span className="nav-count">{count}</span>}
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
