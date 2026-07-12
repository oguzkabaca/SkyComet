import { describe, expect, it } from 'vitest';

import { OPERATION_CONTEXT_VERSION, type PassContextV1 } from '../../lib/operationContext';
import { makePass } from '../../test/fixtures';
import { readSavedTarget, writeSavedTarget } from './target';

const STORAGE_KEY = 'skycomet.quickTrack.target';

function passContextFor(norad: number): PassContextV1 {
  return {
    version: OPERATION_CONTEXT_VERSION,
    satellite: { noradId: norad, name: `SAT-${norad}` },
    pass: makePass(),
    source: 'pass-plan',
  };
}

describe('v2 round-trip', () => {
  it('restores what was written, with the legacy bridge cleared', () => {
    writeSavedTarget({
      norad: 25544,
      name: 'ISS (ZARYA)',
      rfKey: '["a","b"]',
      passContext: passContextFor(25544),
    });
    const restored = readSavedTarget();
    expect(restored).not.toBeNull();
    expect(restored?.norad).toBe(25544);
    expect(restored?.name).toBe('ISS (ZARYA)');
    expect(restored?.rfKey).toBe('["a","b"]');
    expect(restored?.passContext?.satellite.noradId).toBe(25544);
    expect(restored?.legacyRfIndex).toBeNull();
  });

  it('round-trips a target without RF or pass scope', () => {
    writeSavedTarget({ norad: 7, name: 'AO-7', rfKey: null, passContext: null });
    expect(readSavedTarget()).toEqual({
      version: 2,
      norad: 7,
      name: 'AO-7',
      rfKey: null,
      passContext: null,
      legacyRfIndex: null,
    });
  });

  it('clears the saved target when writing null', () => {
    writeSavedTarget({ norad: 7, name: 'AO-7', rfKey: null, passContext: null });
    writeSavedTarget(null);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
    expect(readSavedTarget()).toBeNull();
  });
});

describe('v2 strict validation', () => {
  it('rejects a pass context saved for a different satellite', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        version: 2,
        norad: 25544,
        name: 'ISS (ZARYA)',
        rfKey: null,
        passContext: passContextFor(20442), // mismatched NORAD
      }),
    );
    expect(readSavedTarget()).toBeNull();
  });

  it('rejects an invalid pass context payload', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        version: 2,
        norad: 25544,
        name: 'ISS (ZARYA)',
        rfKey: null,
        passContext: { version: 1 },
      }),
    );
    expect(readSavedTarget()).toBeNull();
  });

  it('rejects a non-string rfKey', () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ version: 2, norad: 25544, name: 'ISS', rfKey: 3, passContext: null }),
    );
    expect(readSavedTarget()).toBeNull();
  });
});

describe('v1 migration', () => {
  it('bridges a valid v1 record into a v2 target with legacyRfIndex', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 25544, name: 'ISS', rfIndex: 2 }));
    expect(readSavedTarget()).toEqual({
      version: 2,
      norad: 25544,
      name: 'ISS',
      rfKey: null,
      passContext: null,
      legacyRfIndex: 2,
    });
  });

  it('drops an unusable v1 rfIndex but keeps the satellite', () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 25544, name: 'ISS', rfIndex: -1 }));
    expect(readSavedTarget()?.legacyRfIndex).toBeNull();
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 25544, name: 'ISS', rfIndex: 1.5 }));
    expect(readSavedTarget()?.legacyRfIndex).toBeNull();
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 25544, name: 'ISS' }));
    expect(readSavedTarget()?.legacyRfIndex).toBeNull();
  });
});

describe('malformed storage', () => {
  it('returns null instead of throwing', () => {
    expect(readSavedTarget()).toBeNull(); // nothing stored
    localStorage.setItem(STORAGE_KEY, '{broken');
    expect(readSavedTarget()).toBeNull();
    localStorage.setItem(STORAGE_KEY, JSON.stringify([1, 2, 3]));
    expect(readSavedTarget()).toBeNull();
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 0, name: 'X' }));
    expect(readSavedTarget()).toBeNull();
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ norad: 25544, name: '  ' }));
    expect(readSavedTarget()).toBeNull();
  });
});
