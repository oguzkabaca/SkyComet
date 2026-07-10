import { isPassContextV1, type PassContextV1 } from '../../lib/operationContext';

const STORAGE_KEY = 'skycomet.quickTrack.target';
const TARGET_VERSION = 2 as const;

/** Saved target v2: stable RF key plus an optional exact planned-pass scope. */
export interface SavedTarget {
  version: typeof TARGET_VERSION;
  norad: number;
  name: string;
  rfKey: string | null;
  passContext: PassContextV1 | null;
  /** Read-only bridge for v1 `{rfIndex}` records; never written again. */
  legacyRfIndex: number | null;
}

export interface TargetToSave {
  norad: number;
  name: string;
  rfKey: string | null;
  passContext: PassContextV1 | null;
}

function validSatellite(norad: unknown, name: unknown): norad is number {
  return (
    typeof norad === 'number' &&
    Number.isInteger(norad) &&
    norad > 0 &&
    typeof name === 'string' &&
    name.trim() !== ''
  );
}

/** Strict v2 read with a narrow, backward-compatible v1 `rfIndex` migration. */
export function readSavedTarget(): SavedTarget | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return null;
    const target = parsed as Record<string, unknown>;
    if (!validSatellite(target.norad, target.name)) return null;

    if (target.version === TARGET_VERSION) {
      const rfKey = target.rfKey === null || typeof target.rfKey === 'string' ? target.rfKey : null;
      const passContext = target.passContext === null ? null : target.passContext;
      if (
        (target.rfKey !== null && typeof target.rfKey !== 'string') ||
        (passContext !== null && !isPassContextV1(passContext))
      ) {
        return null;
      }
      if (passContext !== null && passContext.satellite.noradId !== target.norad) return null;
      return {
        version: TARGET_VERSION,
        norad: target.norad,
        name: target.name as string,
        rfKey,
        passContext,
        legacyRfIndex: null,
      };
    }

    // v1 had no version and used a position in the fetched frequency array.
    const legacyRfIndex =
      typeof target.rfIndex === 'number' &&
      Number.isInteger(target.rfIndex) &&
      target.rfIndex >= 0
        ? target.rfIndex
        : null;
    return {
      version: TARGET_VERSION,
      norad: target.norad,
      name: target.name as string,
      rfKey: null,
      passContext: null,
      legacyRfIndex,
    };
  } catch {
    return null;
  }
}

export function writeSavedTarget(target: TargetToSave | null): void {
  try {
    if (target === null) {
      localStorage.removeItem(STORAGE_KEY);
      return;
    }
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        version: TARGET_VERSION,
        norad: target.norad,
        name: target.name,
        rfKey: target.rfKey,
        passContext: target.passContext,
      }),
    );
  } catch {
    // localStorage may be unavailable (private mode); persistence is best-effort.
  }
}
