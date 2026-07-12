import type { FrequencyRecord, Pass } from '../lib/ipc/commands';

/** A valid overhead pass; timestamps default to a far-future window so
 * Date.now()-based pruning never trips during the suite's lifetime. */
export function makePass(overrides: Partial<Pass> = {}): Pass {
  return {
    aos: '2099-01-01T10:00:00Z',
    tca: '2099-01-01T10:05:00Z',
    los: '2099-01-01T10:10:00Z',
    durationSeconds: 600,
    maxElevationDeg: 78.4,
    aosAzimuthDeg: 195.2,
    tcaAzimuthDeg: 270.1,
    losAzimuthDeg: 344.9,
    aosRangeKm: 2350.7,
    tcaRangeKm: 431.2,
    score: 92.5,
    classification: 'overhead',
    ...overrides,
  };
}

export function makeFrequency(overrides: Partial<FrequencyRecord> = {}): FrequencyRecord {
  return {
    noradId: 25544,
    uplinkLowHz: 145_990_000,
    uplinkHighHz: 145_990_000,
    downlinkLowHz: 437_800_000,
    downlinkHighHz: 437_800_000,
    mode: 'FM',
    description: 'Repeater',
    status: 'active',
    updatedAt: '2026-07-01T00:00:00Z',
    ...overrides,
  };
}
