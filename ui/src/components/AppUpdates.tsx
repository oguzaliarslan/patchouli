import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import { Field } from "./inputs";

const key = ["app-updates"] as const;

export function AppUpdates() {
  const cache = useQueryClient();
  const status = useQuery({ queryKey: key, queryFn: api.updateStatus });
  const [progress, setProgress] = useState<{ downloaded: number; total: number | null }>();
  const check = useMutation({
    mutationFn: api.checkUpdate,
    onSuccess: () => cache.invalidateQueries({ queryKey: key }),
  });
  const install = useMutation({
    mutationFn: api.installUpdate,
    onMutate: () => setProgress(undefined),
    onSettled: () => cache.invalidateQueries({ queryKey: key }),
  });

  useEffect(() => {
    const available = api.onUpdateAvailable(() => {
      void cache.invalidateQueries({ queryKey: key });
    });
    const downloading = api.onUpdateProgress(setProgress);
    return () => {
      void available.then((stop) => stop());
      void downloading.then((stop) => stop());
    };
  }, [cache]);

  const update = status.data?.available;
  const error = install.error ?? check.error ?? status.error;
  const busy = install.isPending || check.isPending;
  return (
    <Field label="App updates">
      <div className="inline">
        <button disabled={busy || !status.data?.configured} onClick={() => { install.reset(); check.mutate(); }}>
          {check.isPending ? "Checking…" : "Check for updates"}
        </button>
        {update && (
          <button disabled={busy} onClick={() => install.mutate()}>
            Install {update.version} and restart
          </button>
        )}
      </div>
      {install.isPending && (
        <p role="status">
          {progress?.total
            ? `Downloaded ${Math.min(100, Math.floor(100 * progress.downloaded / progress.total))}%`
            : "Downloading and installing…"}
        </p>
      )}
      {error && <p className="error" role="alert">{errorMessage(error)}</p>}
      {check.isSuccess && !update && !install.isPending && <p role="status">Patchouli is up to date.</p>}
      {status.data && !status.data.configured && (
        <p className="muted small">Download updates for this build from GitHub Releases.</p>
      )}
      {update?.notes && <p className="muted small">{update.notes}</p>}
    </Field>
  );
}
