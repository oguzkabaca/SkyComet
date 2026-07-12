import type { FrequencyRecord } from '../../lib/ipc/commands';
import type { OperationIntentV1 } from '../../lib/operationContext';

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

/** The stable key of the selected profile, or null when nothing is selected. */
export function selectionKey(
  selection: RFSelection,
  frequencies: FrequencyRecord[],
): string | null {
  if (selection.kind !== 'profile') return null;
  const frequency = frequencies[selection.index];
  return frequency ? rfProfileKey(frequency) : null;
}

/** A synthetic frequency row for an RF Planner custom-frequency handoff. */
export function operationFrequency(
  norad: number,
  rf: NonNullable<OperationIntentV1['rf']>,
): FrequencyRecord {
  return {
    noradId: norad,
    uplinkLowHz: null,
    uplinkHighHz: null,
    downlinkLowHz: rf.frequencyHz,
    downlinkHighHz: rf.frequencyHz,
    mode: rf.mode,
    description: rf.label,
    status: null,
    updatedAt: null,
  };
}

export interface ResolvedRf {
  frequencies: FrequencyRecord[];
  selection: RFSelection;
  warning: string | null;
}

/**
 * Restore the RF selection for a satellite: an operation handoff wins over the
 * saved key, the saved key over the legacy v1 index; a lone profile is
 * auto-selected. Missing profiles fall back to "no RF" with a warning.
 */
export function resolveRfPreference(
  norad: number,
  available: FrequencyRecord[],
  rfKey: string | null,
  legacyRfIndex: number | null,
  operationRf: OperationIntentV1['rf'],
): ResolvedRf {
  if (operationRf !== null) {
    if (operationRf.profileKey !== null) {
      const index = findRfProfileIndex(operationRf.profileKey, available);
      return index >= 0
        ? { frequencies: available, selection: { kind: 'profile', index }, warning: null }
        : {
            frequencies: available,
            selection: { kind: 'none' },
            warning:
              'The requested RF profile is no longer available. Select an RF profile again.',
          };
    }
    const frequencies = [...available, operationFrequency(norad, operationRf)];
    return {
      frequencies,
      selection: { kind: 'profile', index: frequencies.length - 1 },
      warning: null,
    };
  }

  if (rfKey !== null) {
    const index = findRfProfileIndex(rfKey, available);
    return index >= 0
      ? { frequencies: available, selection: { kind: 'profile', index }, warning: null }
      : {
          frequencies: available,
          selection: { kind: 'none' },
          warning: 'The requested RF profile is no longer available. Select an RF profile again.',
        };
  }

  if (legacyRfIndex !== null) {
    return available[legacyRfIndex]
      ? {
          frequencies: available,
          selection: { kind: 'profile', index: legacyRfIndex },
          warning: null,
        }
      : {
          frequencies: available,
          selection: { kind: 'none' },
          warning: 'The saved RF profile is no longer available. Select an RF profile again.',
        };
  }

  return available.length === 1
    ? { frequencies: available, selection: { kind: 'profile', index: 0 }, warning: null }
    : { frequencies: available, selection: { kind: 'none' }, warning: null };
}
