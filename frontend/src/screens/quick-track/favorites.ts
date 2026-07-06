import { useCallback, useEffect, useState } from 'react';

const STORAGE_KEY = 'skycomet.quickTrack.favorites';

function read(): Set<number> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((n): n is number => typeof n === 'number'));
  } catch {
    return new Set();
  }
}

function write(ids: Set<number>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...ids]));
  } catch {
    // localStorage may be unavailable (private mode); favorites are best-effort.
  }
}

/**
 * Favorite NORAD ids, persisted in localStorage. Frontend-only — there is no
 * backend for favorites (ADR 0013 scope note); this is deliberate UI state.
 */
export function useFavorites() {
  const [favorites, setFavorites] = useState<Set<number>>(read);

  useEffect(() => {
    write(favorites);
  }, [favorites]);

  const toggle = useCallback((norad: number) => {
    setFavorites((prev) => {
      const next = new Set(prev);
      if (next.has(norad)) next.delete(norad);
      else next.add(norad);
      return next;
    });
  }, []);

  return { favorites, toggle };
}
