import { describe, expect, it } from 'vitest';

import { makePass } from '../test/fixtures';
import { formatCountdown, isPlanned, readPlan, type PlannedPass } from './passPlan';

const STORAGE_KEY = 'skycomet.passPlan';

function entry(norad: number, aos: string, los: string, name = `SAT-${norad}`): PlannedPass {
  return { norad, name, pass: makePass({ aos, tca: aos, los }) };
}

function store(value: unknown): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

describe('readPlan', () => {
  it('returns an empty plan when nothing is stored', () => {
    expect(readPlan()).toEqual([]);
  });

  it('returns an empty plan for malformed storage', () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    expect(readPlan()).toEqual([]);
    store({ norad: 25544 }); // an object, not an array
    expect(readPlan()).toEqual([]);
    store('plain string');
    expect(readPlan()).toEqual([]);
  });

  it('drops entries that fail strict pass validation and keeps the rest', () => {
    const good = entry(25544, '2099-01-01T10:00:00Z', '2099-01-01T10:10:00Z');
    store([
      good,
      { norad: 7, name: 'BROKEN' }, // no pass at all
      { ...good, norad: -1 }, // invalid NORAD
      { ...good, pass: { ...good.pass, classification: 'excellent' } }, // unknown class
      { ...good, pass: { ...good.pass, aos: '2099-01-01T10:00:00' } }, // no timezone
      42,
      null,
    ]);
    expect(readPlan()).toEqual([good]);
  });

  it('prunes passes whose LOS is already in the past', () => {
    const past = entry(1, '2000-01-01T10:00:00Z', '2000-01-01T10:10:00Z');
    const future = entry(2, '2099-01-01T10:00:00Z', '2099-01-01T10:10:00Z');
    store([past, future]);
    expect(readPlan()).toEqual([future]);
  });

  it('sorts the plan chronologically by AOS regardless of stored order', () => {
    const first = entry(1, '2099-01-01T08:00:00Z', '2099-01-01T08:10:00Z');
    const second = entry(2, '2099-01-01T09:00:00Z', '2099-01-01T09:10:00Z');
    const third = entry(3, '2099-01-02T07:00:00Z', '2099-01-02T07:10:00Z');
    store([third, first, second]);
    expect(readPlan().map((e) => e.norad)).toEqual([1, 2, 3]);
  });
});

describe('isPlanned', () => {
  it('matches the same pass across different timezone spellings', () => {
    const entries = [entry(25544, '2099-01-01T10:00:00Z', '2099-01-01T10:10:00Z')];
    expect(isPlanned(entries, 25544, '2099-01-01T13:00:00+03:00')).toBe(true);
    expect(isPlanned(entries, 25544, '2099-01-01T11:00:00Z')).toBe(false);
    expect(isPlanned(entries, 7, '2099-01-01T10:00:00Z')).toBe(false);
  });

  it('never matches a malformed AOS', () => {
    const entries = [entry(25544, '2099-01-01T10:00:00Z', '2099-01-01T10:10:00Z')];
    expect(isPlanned(entries, 25544, 'not a date')).toBe(false);
  });
});

describe('formatCountdown', () => {
  const aos = '2099-01-01T10:00:00Z';
  const los = '2099-01-01T10:10:00Z';
  const aosMs = new Date(aos).getTime();
  const losMs = new Date(los).getTime();

  it('reports an active pass as in progress', () => {
    expect(formatCountdown(aos, los, aosMs)).toBe('in progress');
    expect(formatCountdown(aos, los, losMs - 1)).toBe('in progress');
  });

  it('reports a finished pass as ended', () => {
    expect(formatCountdown(aos, los, losMs)).toBe('ended');
  });

  it('formats sub-minute, minute and hour countdowns', () => {
    expect(formatCountdown(aos, los, aosMs - 30_000)).toBe('in <1m');
    expect(formatCountdown(aos, los, aosMs - 12 * 60_000)).toBe('in 12m');
    expect(formatCountdown(aos, los, aosMs - (60 + 5) * 60_000)).toBe('in 1h 05m');
    expect(formatCountdown(aos, los, aosMs - 2 * 60 * 60_000)).toBe('in 2h 00m');
  });
});
