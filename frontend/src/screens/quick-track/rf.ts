import type { FrequencyRecord } from '../../lib/ipc/commands';

/** RF selection: an index into the satellite's frequency list, or "no RF". */
export type RFSelection = { kind: 'none' } | { kind: 'profile'; index: number };

/** Stable identity for a DB frequency row; unlike a list index it survives reordering. */
export function rfProfileKey(f: FrequencyRecord): string {
  return JSON.stringify([
    f.downlinkLowHz,
    f.downlinkHighHz,
    f.uplinkLowHz,
    f.uplinkHighHz,
    f.mode?.trim() ?? null,
    f.description?.trim() ?? null,
  ]);
}

export function findRfProfileIndex(key: string, frequencies: FrequencyRecord[]): number {
  return frequencies.findIndex((frequency) => rfProfileKey(frequency) === key);
}

export function fmtMHz(hz: number | null): string | null {
  if (hz === null || !Number.isFinite(hz)) return null;
  return (hz / 1e6).toFixed(3);
}

/** "145.960 MHz" for a point, "145.925–145.975 MHz" for a transponder range. */
export function fmtBand(low: number | null, high: number | null): string | null {
  const lo = fmtMHz(low);
  const hi = fmtMHz(high);
  if (lo === null && hi === null) return null;
  if (lo !== null && hi !== null && lo !== hi) return `${lo}–${hi} MHz`;
  return `${lo ?? hi} MHz`;
}

export function profileName(f: FrequencyRecord): string {
  if (f.description && f.description.trim() !== '') return f.description;
  const isRange =
    f.downlinkLowHz !== null && f.downlinkHighHz !== null && f.downlinkLowHz !== f.downlinkHighHz;
  return isRange ? 'Linear Transponder' : (f.mode ?? 'Channel');
}

export function isTrackable(f: FrequencyRecord): boolean {
  return fmtBand(f.downlinkLowHz, f.downlinkHighHz) !== null;
}

export function rfLabelOf(selection: RFSelection, frequencies: FrequencyRecord[]): string | null {
  if (selection.kind === 'none') return 'No RF';
  const f = frequencies[selection.index];
  if (!f) return null;
  return f.description?.trim() || f.mode || 'Channel';
}
