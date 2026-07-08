const STORAGE_KEY = 'skycomet.quickTrack.target';

/** Saved tracking target: the satellite plus its chosen RF profile index. */
export interface SavedTarget {
  norad: number;
  name: string;
  /** Index into the satellite's trackable frequency list, or null for "no RF". */
  rfIndex: number | null;
}

/**
 * Saved target persistence, localStorage-backed like favorites — frontend-only
 * deliberate UI state (ADR 0013 scope note). The RF index is re-validated
 * against the fetched frequency list on restore.
 */
export function readSavedTarget(): SavedTarget | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return null;
    const t = parsed as Record<string, unknown>;
    if (typeof t.norad !== 'number' || typeof t.name !== 'string') return null;
    return {
      norad: t.norad,
      name: t.name,
      rfIndex: typeof t.rfIndex === 'number' ? t.rfIndex : null,
    };
  } catch {
    return null;
  }
}

export function writeSavedTarget(target: SavedTarget | null): void {
  try {
    if (target === null) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, JSON.stringify(target));
  } catch {
    // localStorage may be unavailable (private mode); persistence is best-effort.
  }
}
