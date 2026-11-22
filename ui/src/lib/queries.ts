import {
  QueryClient,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, errorMessage, type ListEntry, type Settings, type TrackingState } from "../api";

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
  discover: ["discover"] as const,
  sequels: ["discover", "sequels"] as const,
  recommendations: ["discover", "recommendations"] as const,
  /** Under `queue`, so queue events refresh it. */
  mirror: ["queue", "mirror"] as const,
  history: ["history"] as const,
  info: ["info"] as const,
  anime: (id: number) => ["anime", id] as const,
  players: ["players"] as const,
  tracking: ["tracking"] as const,
  taiga: ["taiga"] as const,
  torrents: ["torrents"] as const,
  available: ["available"] as const,
  filterPresets: ["filterPresets"] as const,
  torrentArchive: ["torrentArchive"] as const,
};

/** What's playing and the update state, kept current by backend events. */
export function useTracking() {
  const client = useQueryClient();
  const query = useQuery({ queryKey: keys.tracking, queryFn: api.tracking });
  useEffect(() => {
    const nowPlaying = api.onNowPlaying((playing) =>
      client.setQueryData<TrackingState>(keys.tracking, (old) =>
        old ? { ...old, nowPlaying: playing } : old,
      ),
    );
    const update = api.onUpdateState((state) =>
      client.setQueryData<TrackingState>(keys.tracking, (old) =>
        old ? { ...old, update: state } : old,
      ),
    );
    return () => {
      void nowPlaying.then((f) => f());
      void update.then((f) => f());
    };
  }, [client]);
  return query;
}

/** Data that changes when the list or queue changes. */
function invalidateListData() {
  for (const key of [keys.list, keys.queue, keys.history, keys.info, keys.discover, ["anime"]]) {
    void queryClient.invalidateQueries({ queryKey: key });
  }
}

export const useSettings = () => useQuery({ queryKey: keys.settings, queryFn: api.getSettings });
export const useAccounts = () => useQuery({ queryKey: keys.accounts, queryFn: api.accounts });
export const useList = () => useQuery({ queryKey: keys.list, queryFn: api.list });
export const useSequels = () => useQuery({ queryKey: keys.sequels, queryFn: api.sequels });
export const useRecommendations = () => useQuery({ queryKey: keys.recommendations, queryFn: api.recommendations });
export const useQueue = () => useQuery({ queryKey: keys.queue, queryFn: api.queue });
export const useHistory = () => useQuery({ queryKey: keys.history, queryFn: api.history });
export const useAppInfo = () => useQuery({ queryKey: keys.info, queryFn: api.appInfo });
export const useTorrents = () => useQuery({ queryKey: keys.torrents, queryFn: api.torrents });
export const useFilterPresets = () => useQuery({ queryKey: keys.filterPresets, queryFn: api.filterPresets });
export const useTaigaDetection = () => useQuery({ queryKey: keys.taiga, queryFn: api.taigaDetect });
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
    const torrents = api.onTorrents((feed) => queryClient.setQueryData(keys.torrents, feed));
    const available = api.onLibraryAvailable((map) => queryClient.setQueryData(keys.available, map));
    const discover = api.onDiscoverChanged(() => void queryClient.invalidateQueries({ queryKey: keys.discover }));
    const settings = api.onSettingsChanged((changed) => {
      void queryClient.invalidateQueries({ queryKey: keys.settings });
      if (changed.includes("sync.service") || changed.includes("sync.mirror.enabled")) {
        void queryClient.invalidateQueries({ queryKey: keys.mirror });
        void queryClient.invalidateQueries({ queryKey: keys.accounts });
      }
    });
    return () => {
      void sync.then((f) => f());
      void settings.then((f) => f());
      void torrents.then((f) => f());
      void available.then((f) => f());
      void discover.then((f) => f());
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
