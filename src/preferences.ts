import { useState } from 'react';

// UI preferences are independent of game edits and survive WebView restarts.
function useSavedChoice<T extends string>(key: string, fallback: T, allowed: readonly T[]) {
  const [value, setValue] = useState<T>(() => {
    try {
      const stored = localStorage.getItem(key) as T;
      return allowed.includes(stored) ? stored : fallback;
    } catch {
      return fallback;
    }
  });
  const update = (next: T) => {
    if (!allowed.includes(next)) return;
    setValue(next);
    try {
      localStorage.setItem(key, next);
    } catch {
      /* The preference remains usable if storage is unavailable. */
    }
  };
  return [value, update] as const;
}
export function useSavedSort<T extends string>(module: string, fallback: T, allowed: readonly T[]) {
  return useSavedChoice(`butter-manager.sort.${module}`, fallback, allowed);
}
export function useLibraryLayout() {
  return useSavedChoice('butter-manager.view.library', 'list', ['list', 'grid'] as const);
}
