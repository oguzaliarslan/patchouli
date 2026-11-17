import {
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, errorMessage, type ListEntry, type Settings } from "../api";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: Infinity, retry: false, refetchOnWindowFocus: false },
  },
});

export const keys = {
  settings: ["settings"] as const,
  accounts: ["accounts"] as const,
  list: ["list"] as const,
  queue: ["queue"] as const,
  history: ["history"] as const,
  info: ["info"] as const,
  anime: (id: number) => ["anime", id] as const,
  players: ["players"] as const,
};

/** Data that changes when the list or queue changes. */
function invalidateListData() {
  for (const key of [keys.list, keys.queue, keys.history, keys.info, ["anime"]]) {
    void queryClient.invalidateQueries({ queryKey: key });
  }
}

export const useSettings = () => useQuery({ queryKey: keys.settings, queryFn: api.getSettings });
export const useAccounts = () => useQuery({ queryKey: keys.accounts, queryFn: api.accounts });
export const useList = () => useQuery({ queryKey: keys.list, queryFn: api.list });
export const useQueue = () => useQuery({ queryKey: keys.queue, queryFn: api.queue });
export const useHistory = () => useQuery({ queryKey: keys.history, queryFn: api.history });
export const useAppInfo = () => useQuery({ queryKey: keys.info, queryFn: api.appInfo });
export const usePlayers = () => useQuery({ queryKey: keys.players, queryFn: api.players });
export const useAnime = (id: number | null) =>
  useQuery({
    queryKey: keys.anime(id ?? 0),
    queryFn: () => api.anime(id!),
    enabled: id !== null,
  });

/** The connected account, if any, and its score format. */
export function useCurrentAccount() {
  const { data: accounts } = useAccounts();
  const current = accounts?.find((a) => a.current);
  return {
    account: current,
    connected: accounts?.find((a) => a.connected),
    scoreFormat: current?.scoreFormat ?? null,
  };
}

/** Saves settings: `update` gets the current settings and returns new ones. */
export function useUpdateSettings() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (update: (s: Settings) => Settings) => {
      const current = client.getQueryData<Settings>(keys.settings) ?? (await api.getSettings());
      const next = update(current);
      client.setQueryData(keys.settings, next);
      await api.setSettings(next);
      return next;
    },
    onError: () => client.invalidateQueries({ queryKey: keys.settings }),
  });
}

export function useSaveEntry() {
  return useMutation({
    mutationFn: (entry: ListEntry) => api.saveEntry(entry),
    onSettled: invalidateListData,
  });
}

export function useRemoveEntry() {
  return useMutation({
    mutationFn: (animeId: number) => api.removeEntry(animeId),
    onSettled: invalidateListData,
  });
}

export interface SyncState {
  busy: boolean;
  error: string | null;
  unauthorized: boolean;
  clearError: () => void;
  syncNow: () => Promise<void>;
}

/**
 * Listens to backend events for the whole app: refreshes cached data when the
 * list, queue or settings change, and tracks sync progress and errors.
 */
export function useBackendEvents(): SyncState {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [unauthorized, setUnauthorized] = useState(false);

  useEffect(() => {
    const sync = api.onSync((event) => {
      switch (event.type) {
        case "busy":
          setBusy(event.busy);
          if (event.busy) setError(null);
          break;
        case "error":
          setError(event.message);
          setUnauthorized(event.unauthorized);
          break;
        default:
          invalidateListData();
      }
    });
    const settings = api.onSettingsChanged((changed) => {
      void queryClient.invalidateQueries({ queryKey: keys.settings });
      if (changed.includes("sync.service")) {
        void queryClient.invalidateQueries({ queryKey: keys.accounts });
      }
    });
    return () => {
      void sync.then((f) => f());
      void settings.then((f) => f());
    };
  }, []);

  return {
    busy,
    error,
    unauthorized,
    clearError: () => setError(null),
    syncNow: async () => {
      setError(null);
      setBusy(true);
      try {
        await api.syncNow();
      } catch (e) {
        setError(errorMessage(e));
      } finally {
        setBusy(false);
        invalidateListData();
      }
    },
  };
}
