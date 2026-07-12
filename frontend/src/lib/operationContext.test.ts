import { describe, expect, it } from 'vitest';

import { makePass } from '../test/fixtures';
import {
  OPERATION_CONTEXT_VERSION,
  canonicalUtc,
  createOperationIntent,
  createPassContext,
  isOperationIntentV1,
  isOperationRfContextV1,
  isPass,
  isPassContextV1,
  passKey,
  samePassContext,
  type OperationRfContextV1,
  type PassContextV1,
} from './operationContext';

function makePassContext(overrides: Partial<PassContextV1> = {}): PassContextV1 {
  return {
    version: OPERATION_CONTEXT_VERSION,
    satellite: { noradId: 25544, name: 'ISS (ZARYA)' },
    pass: makePass(),
    source: 'pass-planner',
    ...overrides,
  };
}

function makeRf(overrides: Partial<OperationRfContextV1> = {}): OperationRfContextV1 {
  return {
    profileKey: null,
    frequencyHz: 145_960_000,
    mode: 'FM',
    label: 'Custom 145.960 MHz',
    ...overrides,
  };
}

describe('canonicalUtc', () => {
  it('normalizes an offset timestamp to the same UTC instant', () => {
    expect(canonicalUtc('2026-07-12T15:30:00+03:00')).toBe('2026-07-12T12:30:00.000Z');
    expect(canonicalUtc('2026-07-12T09:30:00-03:00')).toBe(canonicalUtc('2026-07-12T12:30:00Z'));
  });

  it('keeps fractional seconds to millisecond precision', () => {
    expect(canonicalUtc('2026-07-12T12:30:00.5Z')).toBe('2026-07-12T12:30:00.500Z');
    expect(canonicalUtc('2026-07-12T12:30:00.123456Z')).toBe('2026-07-12T12:30:00.123Z');
  });

  it('rejects timestamps without a timezone', () => {
    expect(canonicalUtc('2026-07-12T12:30:00')).toBeNull();
  });

  it('rejects calendar rollover instead of silently wrapping', () => {
    expect(canonicalUtc('2026-02-30T00:00:00Z')).toBeNull();
    expect(canonicalUtc('2026-02-29T00:00:00Z')).toBeNull(); // 2026 is not a leap year
    expect(canonicalUtc('2024-02-29T00:00:00Z')).not.toBeNull(); // 2024 is
    expect(canonicalUtc('2026-13-01T00:00:00Z')).toBeNull();
    expect(canonicalUtc('2026-07-12T24:00:00Z')).toBeNull();
    expect(canonicalUtc('2026-07-12T12:60:00Z')).toBeNull();
  });

  it('rejects garbage', () => {
    expect(canonicalUtc('')).toBeNull();
    expect(canonicalUtc('not a date')).toBeNull();
    expect(canonicalUtc('2026-07-12')).toBeNull();
  });
});

describe('passKey', () => {
  it('yields one identity for the same instant in different offsets', () => {
    const zulu = passKey(25544, '2026-07-12T12:30:00Z');
    expect(zulu).toBe('25544:2026-07-12T12:30:00.000Z');
    expect(passKey(25544, '2026-07-12T15:30:00+03:00')).toBe(zulu);
  });

  it('rejects invalid NORAD ids and malformed AOS', () => {
    expect(passKey(0, '2026-07-12T12:30:00Z')).toBeNull();
    expect(passKey(-1, '2026-07-12T12:30:00Z')).toBeNull();
    expect(passKey(1.5, '2026-07-12T12:30:00Z')).toBeNull();
    expect(passKey(25544, '2026-07-12T12:30:00')).toBeNull();
  });
});

describe('isPass', () => {
  it('accepts a fully valid pass', () => {
    expect(isPass(makePass())).toBe(true);
  });

  it('rejects a broken AOS <= TCA <= LOS chain', () => {
    expect(isPass(makePass({ tca: '2099-01-01T09:00:00Z' }))).toBe(false);
    expect(isPass(makePass({ los: '2099-01-01T10:01:00Z', tca: '2099-01-01T10:05:00Z' }))).toBe(
      false,
    );
  });

  it('rejects missing or non-finite numeric fields', () => {
    const withoutScore: Record<string, unknown> = { ...makePass() };
    delete withoutScore.score;
    expect(isPass(withoutScore)).toBe(false);
    expect(isPass(makePass({ score: Number.NaN }))).toBe(false);
    expect(isPass(makePass({ aosRangeKm: Number.POSITIVE_INFINITY }))).toBe(false);
  });

  it('rejects out-of-range values and unknown classifications', () => {
    expect(isPass(makePass({ durationSeconds: -1 }))).toBe(false);
    expect(isPass(makePass({ tcaRangeKm: -5 }))).toBe(false);
    expect(isPass(makePass({ maxElevationDeg: 95 }))).toBe(false);
    expect(isPass({ ...makePass(), classification: 'excellent' })).toBe(false);
    expect(isPass(null)).toBe(false);
    expect(isPass([])).toBe(false);
  });
});

describe('isPassContextV1', () => {
  it('accepts a valid context', () => {
    expect(isPassContextV1(makePassContext())).toBe(true);
  });

  it('rejects wrong version, blank name, bad NORAD and unknown source', () => {
    expect(isPassContextV1({ ...makePassContext(), version: 2 })).toBe(false);
    expect(
      isPassContextV1(makePassContext({ satellite: { noradId: 25544, name: '  ' } })),
    ).toBe(false);
    expect(isPassContextV1(makePassContext({ satellite: { noradId: 0, name: 'X' } }))).toBe(false);
    expect(isPassContextV1({ ...makePassContext(), source: 'catalog' })).toBe(false);
  });
});

describe('isOperationRfContextV1', () => {
  it('accepts a profile handoff and a custom frequency', () => {
    expect(isOperationRfContextV1(makeRf({ profileKey: 'key-1' }))).toBe(true);
    expect(isOperationRfContextV1(makeRf({ profileKey: null }))).toBe(true);
  });

  it('rejects non-positive frequency and blank mode/label', () => {
    expect(isOperationRfContextV1(makeRf({ frequencyHz: 0 }))).toBe(false);
    expect(isOperationRfContextV1(makeRf({ frequencyHz: -145e6 }))).toBe(false);
    expect(isOperationRfContextV1(makeRf({ mode: ' ' }))).toBe(false);
    expect(isOperationRfContextV1(makeRf({ label: '' }))).toBe(false);
    expect(isOperationRfContextV1({ ...makeRf(), profileKey: 7 })).toBe(false);
  });
});

describe('isOperationIntentV1', () => {
  function makeIntent() {
    return createOperationIntent('quick-track', makePassContext(), makeRf());
  }

  it('round-trips createOperationIntent through validation', () => {
    expect(isOperationIntentV1(makeIntent())).toBe(true);
    expect(isOperationIntentV1(createOperationIntent('rf-planner', makePassContext()))).toBe(true);
  });

  it('survives a JSON round-trip (navigation payload shape)', () => {
    expect(isOperationIntentV1(JSON.parse(JSON.stringify(makeIntent())))).toBe(true);
  });

  it('rejects bad destination, createdAt, pass context and rf payloads', () => {
    expect(isOperationIntentV1({ ...makeIntent(), destination: 'catalog' })).toBe(false);
    expect(isOperationIntentV1({ ...makeIntent(), createdAt: 'yesterday' })).toBe(false);
    expect(isOperationIntentV1({ ...makeIntent(), passContext: {} })).toBe(false);
    expect(isOperationIntentV1({ ...makeIntent(), rf: { frequencyHz: 1 } })).toBe(false);
    expect(isOperationIntentV1({ ...makeIntent(), version: 0 })).toBe(false);
    expect(isOperationIntentV1(null)).toBe(false);
  });
});

describe('samePassContext', () => {
  it('treats different offset spellings of the same AOS as the same pass', () => {
    const a = makePassContext({ pass: makePass({ aos: '2099-01-01T10:00:00Z' }) });
    const b = makePassContext({
      pass: makePass({ aos: '2099-01-01T13:00:00+03:00' }),
      source: 'quick-track',
    });
    expect(samePassContext(a, b)).toBe(true);
  });

  it('distinguishes different satellites and different passes', () => {
    const a = makePassContext();
    expect(samePassContext(a, makePassContext({ satellite: { noradId: 7, name: 'X' } }))).toBe(
      false,
    );
    expect(
      samePassContext(a, makePassContext({ pass: makePass({ aos: '2099-01-02T10:00:00Z' }) })),
    ).toBe(false);
  });

  it('handles null on either side', () => {
    const a = makePassContext();
    expect(samePassContext(null, null)).toBe(true);
    expect(samePassContext(a, null)).toBe(false);
    expect(samePassContext(null, a)).toBe(false);
  });
});

describe('createPassContext', () => {
  it('copies the satellite identity and stamps the current version', () => {
    const context = createPassContext({ norad_id: 25544, name: 'ISS (ZARYA)' }, makePass(), 'pass-plan');
    expect(context.version).toBe(OPERATION_CONTEXT_VERSION);
    expect(context.satellite).toEqual({ noradId: 25544, name: 'ISS (ZARYA)' });
    expect(isPassContextV1(context)).toBe(true);
  });
});
