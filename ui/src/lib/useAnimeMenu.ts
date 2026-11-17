import { confirm } from "@tauri-apps/plugin-dialog";
import { useCallback } from "react";
import { errorMessage } from "../api";
import { useApp } from "./context";
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
      });
      app.showMenu({ x: event.clientX, y: event.clientY, items });
    },
    [app, settings, scoreFormat, save, remove],
  );
}
