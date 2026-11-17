import { useCallback, useState } from "react";

/**
 * View preferences (sort, view mode, selected tab) kept per device in
 * localStorage. Storage can be unavailable, so every access is guarded and
 * the default is used instead.
 */
export function usePref<T>(key: string, initial: T): [T, (value: T) => void] {
  const storageKey = `tracker.${key}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      return raw === null ? initial : (JSON.parse(raw) as T);
    } catch {
      return initial;
    }
  });
  const update = useCallback(
    (next: T) => {
      setValue(next);
      try {
        localStorage.setItem(storageKey, JSON.stringify(next));
      } catch {
        // Not persisted; the in-memory value still applies.
      }
    },
    [storageKey],
  );
  return [value, update];
}
