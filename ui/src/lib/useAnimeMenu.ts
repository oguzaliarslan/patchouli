import { confirm } from "@tauri-apps/plugin-dialog";
import { useCallback } from "react";
import { api, errorCode, errorMessage } from "../api";
import { useApp, type AppActions } from "./context";
import { animeMenu, type MenuTarget } from "./entries";
import { useCurrentAccount, useRemoveEntry, useSaveEntry, useSettings } from "./queries";

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
