import { useCallback, useState } from 'react';

import { type Pass } from './ipc/commands';
import { isPass, passKey } from './operationContext';

const STORAGE_KEY = 'skycomet.passPlan';

/** A pass queued from the Pass Planner for the Quick Track picker. */
export interface PlannedPass {
  norad: number;
  name: string;
  /** Full schedule row; the AOS timestamp identifies the pass. */
  pass: Pass;
}

function isPlannedPass(value: unknown): value is PlannedPass {
  if (typeof value !== 'object' || value === null) return false;
  const e = value as Record<string, unknown>;
  if (typeof e.norad !== 'number' || typeof e.name !== 'string') return false;
  return isPass(e.pass) && passKey(e.norad, e.pass.aos) !== null;
}

/**
 * Planned passes, pruned of entries whose pass already ended and sorted by
 * AOS. localStorage-backed like favorites/target — frontend-only deliberate
 * UI state (ADR 0013 scope note).
 */
export function readPlan(): PlannedPass[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const now = Date.now();
    return parsed
      .filter(isPlannedPass)
      .filter((e) => new Date(e.pass.los).getTime() > now)
      .sort((a, b) => a.pass.aos.localeCompare(b.pass.aos));
  } catch {
    return [];
  }
}

function writePlan(entries: PlannedPass[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    // localStorage may be unavailable (private mode); the plan is best-effort.
  }
}

export function isPlanned(entries: PlannedPass[], norad: number, aos: string): boolean {
  const key = passKey(norad, aos);
  return key !== null && entries.some((e) => passKey(e.norad, e.pass.aos) === key);
}

/**
 * Shared pass-plan state for a mounted component. Reads on mount (both
 * consumers mount fresh: the dialog is conditional, the detail card is keyed
 * per selection), mutations persist and update local state.
 */
export function usePassPlan() {
  const [plan, setPlan] = useState<PlannedPass[]>(readPlan);

  const add = useCallback((entry: PlannedPass) => {
    setPlan(() => {
      const next = readPlan().filter(
        (e) => passKey(e.norad, e.pass.aos) !== passKey(entry.norad, entry.pass.aos),
      );
      next.push(entry);
      next.sort((a, b) => a.pass.aos.localeCompare(b.pass.aos));
      writePlan(next);
      return next;
    });
  }, []);

  const remove = useCallback((norad: number, aos: string) => {
    setPlan(() => {
      const key = passKey(norad, aos);
      const next = readPlan().filter((e) => passKey(e.norad, e.pass.aos) !== key);
      writePlan(next);
      return next;
    });
  }, []);

  return { plan, add, remove };
}

/** "in 1h 24m" / "in 12m" / "in <1m" / "in progress" for a pass window. */
export function formatCountdown(aosIso: string, losIso: string, nowMs: number): string {
  const aosMs = new Date(aosIso).getTime();
  const losMs = new Date(losIso).getTime();
  if (aosMs <= nowMs && nowMs < losMs) return 'in progress';
  if (nowMs >= losMs) return 'ended';
  const totalMin = Math.floor((aosMs - nowMs) / 60_000);
  if (totalMin < 1) return 'in <1m';
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h > 0 ? `in ${h}h ${m.toString().padStart(2, '0')}m` : `in ${m}m`;
}
