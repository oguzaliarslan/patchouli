import { confirm } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback } from "react";
import { api, errorCode, errorMessage, type ListAction, type ListEntry } from "../api";
import { useApp, type AppActions } from "./context";
import { animeMenu, batchMenu, pageUrl, type MenuTarget } from "./entries";
import { invalidateListData, useCurrentAccount, useRemoveEntry, useSaveEntry, useSettings } from "./queries";

/** Asks with a native dialog, or the browser's outside the app. */
export async function confirmAction(message: string, title: string, okLabel = "OK"): Promise<boolean> {
  try {
    return await confirm(message, { title, kind: "warning", okLabel, cancelLabel: "Cancel" });
  } catch {
    return window.confirm(message);
  }
}

/** Plays an episode from the library folders, reporting what happened. */
export async function playEpisode(
  app: AppActions,
  animeId: number,
  title: string,
  options: { episode?: number; random?: boolean } = {},
) {
  app.notify(`Looking for ${title}…`);
  try {
    const result = await api.play(animeId, options);
    app.notify(`Playing episode ${result.episode} of ${title}.`);
  } catch (e) {
    app.notify(errorMessage(e));
    if (errorCode(e) === "no-library-folders") app.navigate("settings");
  }
}

export async function openFolder(app: AppActions, animeId: number) {
  try {
    await api.openFolder(animeId);
  } catch (e) {
    app.notify(errorMessage(e));
  }
}

/** Returns a handler that opens the anime menu at the mouse position. */
export function useAnimeMenu() {
  const app = useApp();
  const { data: settings } = useSettings();
  const { scoreFormat } = useCurrentAccount();
  const save = useSaveEntry();
  const remove = useRemoveEntry();

  return useCallback(
    (event: React.MouseEvent, target: MenuTarget) => {
      event.preventDefault();
      const items = animeMenu(target, {
        service: settings?.["sync.service"],
        scoreFormat,
        openAnime: app.openAnime,
        notify: app.notify,
        save: (entry) =>
          save.mutate(entry, { onError: (e) => app.notify(errorMessage(e)) }),
        remove: (animeId, title) => {
          void confirmAction(
            `Remove “${title}” from your list?`,
            "Remove from list",
            "Remove",
          ).then((ok) => {
            if (ok) remove.mutate(animeId, { onError: (e) => app.notify(errorMessage(e)) });
          });
        },
        play: (animeId, title, options) => void playEpisode(app, animeId, title, options),
        openFolder: (animeId) => void openFolder(app, animeId),
      });
      app.showMenu({ x: event.clientX, y: event.clientY, items });
    },
    [app, settings, scoreFormat, save, remove],
  );
}

/** Returns a handler that opens the menu for several selected anime. */
export function useBatchMenu() {
  const app = useApp();
  const { data: settings } = useSettings();
  const { scoreFormat } = useCurrentAccount();

  return useCallback(
    (event: React.MouseEvent, targets: MenuTarget[]) => {
      event.preventDefault();
      // One at a time, so the sync queue sees them in order; the list refreshes once.
      const run = async (label: string, ids: number[], each: (id: number) => Promise<unknown>) => {
        let failed = 0;
        for (const id of ids) {
          try {
            await each(id);
          } catch (e) {
            failed += 1;
            console.error(e);
          }
        }
        invalidateListData();
        app.notify(failed ? `${label}: ${failed} of ${ids.length} failed.` : `${label}: ${ids.length} anime.`);
      };
      const items = batchMenu(targets, {
        service: settings?.["sync.service"],
        scoreFormat,
        notify: app.notify,
        saveAll: (entries: ListEntry[]) => {
          const byId = new Map(entries.map((e) => [e.animeId, e]));
          void run("Updated", [...byId.keys()], (id) => api.saveEntry(byId.get(id)!));
        },
        removeAll: (remove) => {
          void confirmAction(
            `Remove ${remove.length} anime from your list?`,
            "Remove from list",
            "Remove",
          ).then((ok) => {
            if (ok) void run("Removed", remove.map((t) => t.animeId), (id) => api.removeEntry(id));
          });
        },
      });
      app.showMenu({ x: event.clientX, y: event.clientY, items });
    },
    [app, settings, scoreFormat],
  );
}

/** Returns a handler for the double- and middle-click actions chosen in Settings › Application. */
export function useListAction() {
  const app = useApp();
  const { data: settings } = useSettings();

  return useCallback(
    (click: "double" | "middle", target: MenuTarget) => {
      const action: ListAction =
        click === "double"
          ? (settings?.["app.list.doubleClickAction"] ?? "info")
          : (settings?.["app.list.middleClickAction"] ?? "playNext");
      switch (action) {
        case "none":
          return;
        case "edit":
          return app.openAnime(target.animeId, "entry");
        case "info":
          return app.openAnime(target.animeId);
        case "openFolder":
          return void openFolder(app, target.animeId);
        case "playNext":
          return void playEpisode(app, target.animeId, target.title);
        case "page": {
          const url = pageUrl(settings?.["sync.service"], target.animeId);
          if (url) void openUrl(url);
          return;
        }
      }
    },
    [app, settings],
  );
}
