import { describe, expect, it } from 'vitest';

import type { OperationRfContextV1 } from '../../lib/operationContext';
import { makeFrequency } from '../../test/fixtures';
import {
  findRfProfileIndex,
  fmtBand,
  fmtMHz,
  isTrackable,
  operationFrequency,
  profileName,
  resolveRfPreference,
  rfLabelOf,
  rfProfileKey,
  selectionKey,
} from './rf';

const customRf: OperationRfContextV1 = {
  profileKey: null,
  frequencyHz: 145_960_000,
  mode: 'FM',
  label: 'Custom 145.960 MHz',
};

describe('rfProfileKey / findRfProfileIndex', () => {
  it('is stable for identical rows and survives list reordering', () => {
    const a = makeFrequency();
    const b = makeFrequency({ description: 'Beacon', downlinkLowHz: 145_825_000 });
    expect(rfProfileKey(a)).toBe(rfProfileKey(makeFrequency()));
    expect(findRfProfileIndex(rfProfileKey(a), [b, a])).toBe(1);
    expect(findRfProfileIndex(rfProfileKey(a), [a, b])).toBe(0);
  });

  it('changes when any identity field changes, ignoring whitespace', () => {
    const base = rfProfileKey(makeFrequency());
    expect(rfProfileKey(makeFrequency({ downlinkLowHz: 1 }))).not.toBe(base);
    expect(rfProfileKey(makeFrequency({ mode: 'CW' }))).not.toBe(base);
    expect(rfProfileKey(makeFrequency({ mode: ' FM ' }))).toBe(base);
    expect(rfProfileKey(makeFrequency({ description: ' Repeater ' }))).toBe(base);
    // Row-position metadata must not affect identity.
    expect(rfProfileKey(makeFrequency({ status: null, updatedAt: null }))).toBe(base);
  });

  it('returns -1 for a key that no longer exists', () => {
    expect(findRfProfileIndex(rfProfileKey(makeFrequency()), [])).toBe(-1);
  });
});

describe('formatting', () => {
  it('fmtMHz renders Hz as MHz with three decimals', () => {
    expect(fmtMHz(145_960_000)).toBe('145.960');
    expect(fmtMHz(437_800_600)).toBe('437.801');
    expect(fmtMHz(null)).toBeNull();
    expect(fmtMHz(Number.NaN)).toBeNull();
  });

  it('fmtBand renders points, ranges and missing values', () => {
    expect(fmtBand(145_960_000, 145_960_000)).toBe('145.960 MHz');
    expect(fmtBand(145_925_000, 145_975_000)).toBe('145.925–145.975 MHz');
    expect(fmtBand(145_960_000, null)).toBe('145.960 MHz');
    expect(fmtBand(null, 145_960_000)).toBe('145.960 MHz');
    expect(fmtBand(null, null)).toBeNull();
  });

  it('profileName prefers the description, then transponder detection, then mode', () => {
    expect(profileName(makeFrequency())).toBe('Repeater');
    expect(
      profileName(
        makeFrequency({ description: null, downlinkLowHz: 145_925_000, downlinkHighHz: 145_975_000 }),
      ),
    ).toBe('Linear Transponder');
    expect(profileName(makeFrequency({ description: '  ', mode: 'CW' }))).toBe('CW');
    expect(profileName(makeFrequency({ description: null, mode: null }))).toBe('Channel');
  });

  it('isTrackable requires a downlink', () => {
    expect(isTrackable(makeFrequency())).toBe(true);
    expect(isTrackable(makeFrequency({ downlinkLowHz: null, downlinkHighHz: null }))).toBe(false);
  });

  it('rfLabelOf labels the selection and fails safe on a missing profile', () => {
    const f = makeFrequency();
    expect(rfLabelOf({ kind: 'none' }, [f])).toBe('No RF');
    expect(rfLabelOf({ kind: 'profile', index: 0 }, [f])).toBe('Repeater');
    expect(rfLabelOf({ kind: 'profile', index: 5 }, [f])).toBeNull();
  });
});

describe('selectionKey', () => {
  it('returns the stable key for a selected profile and null otherwise', () => {
    const f = makeFrequency();
    expect(selectionKey({ kind: 'profile', index: 0 }, [f])).toBe(rfProfileKey(f));
    expect(selectionKey({ kind: 'none' }, [f])).toBeNull();
    expect(selectionKey({ kind: 'profile', index: 3 }, [f])).toBeNull();
  });
});

describe('resolveRfPreference', () => {
  const repeater = makeFrequency();
  const beacon = makeFrequency({ description: 'Beacon', downlinkLowHz: 145_825_000 });

  it('restores a saved rfKey even after the list is reordered', () => {
    const resolved = resolveRfPreference(25544, [beacon, repeater], rfProfileKey(repeater), null, null);
    expect(resolved.selection).toEqual({ kind: 'profile', index: 1 });
    expect(resolved.warning).toBeNull();
  });

  it('falls back to no RF with a warning when the saved profile is gone', () => {
    const resolved = resolveRfPreference(25544, [beacon], rfProfileKey(repeater), null, null);
    expect(resolved.selection).toEqual({ kind: 'none' });
    expect(resolved.warning).toMatch(/no longer available/);
  });

  it('honors a legacy v1 index only while it is still in range', () => {
    expect(resolveRfPreference(25544, [beacon, repeater], null, 1, null).selection).toEqual({
      kind: 'profile',
      index: 1,
    });
    const outOfRange = resolveRfPreference(25544, [beacon], null, 4, null);
    expect(outOfRange.selection).toEqual({ kind: 'none' });
    expect(outOfRange.warning).toMatch(/no longer available/);
  });

  it('resolves an operation handoff by profile key ahead of the saved target', () => {
    const resolved = resolveRfPreference(
      25544,
      [beacon, repeater],
      rfProfileKey(beacon), // saved target says beacon…
      null,
      { ...customRf, profileKey: rfProfileKey(repeater) }, // …but the handoff wins
    );
    expect(resolved.selection).toEqual({ kind: 'profile', index: 1 });
    expect(resolved.warning).toBeNull();
  });

  it('warns instead of guessing when the handoff profile is gone', () => {
    const resolved = resolveRfPreference(25544, [beacon], null, null, {
      ...customRf,
      profileKey: rfProfileKey(repeater),
    });
    expect(resolved.selection).toEqual({ kind: 'none' });
    expect(resolved.warning).toMatch(/no longer available/);
  });

  it('round-trips a custom frequency handoff into a selectable, trackable row', () => {
    const resolved = resolveRfPreference(25544, [beacon], null, null, customRf);
    expect(resolved.warning).toBeNull();
    expect(resolved.frequencies).toHaveLength(2);
    expect(resolved.selection).toEqual({ kind: 'profile', index: 1 });

    const synthetic = resolved.frequencies[1];
    expect(synthetic).toEqual(operationFrequency(25544, customRf));
    expect(isTrackable(synthetic)).toBe(true);
    expect(fmtBand(synthetic.downlinkLowHz, synthetic.downlinkHighHz)).toBe('145.960 MHz');
    expect(rfLabelOf(resolved.selection, resolved.frequencies)).toBe('Custom 145.960 MHz');
    // The synthetic row must yield a stable key so the choice persists as v2 rfKey.
    expect(selectionKey(resolved.selection, resolved.frequencies)).toBe(rfProfileKey(synthetic));
  });

  it('auto-selects a lone profile and stays neutral otherwise', () => {
    expect(resolveRfPreference(25544, [repeater], null, null, null)).toEqual({
      frequencies: [repeater],
      selection: { kind: 'profile', index: 0 },
      warning: null,
    });
    expect(resolveRfPreference(25544, [beacon, repeater], null, null, null).selection).toEqual({
      kind: 'none',
    });
    expect(resolveRfPreference(25544, [], null, null, null).selection).toEqual({ kind: 'none' });
  });
});
