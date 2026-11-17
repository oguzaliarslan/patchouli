import { api, errorCode, errorMessage, type TaigaSummary } from "../api";
import type { AppActions } from "./context";
import { keys, queryClient } from "./queries";
import { confirmAction } from "./useAnimeMenu";

function describe(s: TaigaSummary): string {
  const who = s.username ? `${s.username} on ${s.serviceName}` : s.serviceName ?? "unknown service";
  const lines = [
    `Taiga ${s.version === "V1" ? "1.x" : "2"} data in ${s.path} (${who}):`,
    `${s.entries} list entries, ${s.history} history items, ${s.queue} unsent changes, ${s.anime} anime, ${s.settings} settings.`,
  ];
  if (s.warnings.length > 0) lines.push("", ...s.warnings.map((w) => `• ${w}`));
  lines.push("", "Taiga's files aren't changed. You'll need to log in again afterwards.");
  return lines.join("\n");
}

function refresh() {
  for (const key of [keys.settings, keys.accounts, keys.list, keys.queue, keys.history, keys.info, keys.taiga]) {
    void queryClient.invalidateQueries({ queryKey: key });
  }
}

/**
 * Shows what will be imported, asks, and imports. `path` is a Taiga folder,
 * or null for the default location. Returns whether data was imported.
 */
export async function importFromTaiga(app: AppActions, path: string | null): Promise<boolean> {
  try {
    const preview = await api.taigaPreview(path);
    if (!(await confirmAction(describe(preview), "Import from Taiga", "Import"))) return false;
    let summary: TaigaSummary;
    try {
      summary = await api.taigaImport(preview.path);
    } catch (e) {
      if (errorCode(e) !== "replace-local-data") throw e;
      if (!(await confirmAction(errorMessage(e), "Import from Taiga", "Replace list"))) return false;
      summary = await api.taigaImport(preview.path, true);
    }
    app.notify(
      `Imported ${summary.entries} entries from Taiga. Log in to ${summary.serviceName ?? "your service"} to sync.`,
    );
    return true;
  } catch (e) {
    app.notify(errorMessage(e));
    return false;
  } finally {
    refresh();
  }
}

export async function dismissTaigaImport() {
  await api.taigaDismiss();
  void queryClient.invalidateQueries({ queryKey: keys.taiga });
}
