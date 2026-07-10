import type { Pass, SatelliteSummary } from './ipc/commands';

export const OPERATION_CONTEXT_VERSION = 1 as const;

export type OperationDestination = 'rf-planner' | 'quick-track';
export type OperationSource = 'pass-planner' | 'pass-plan' | 'rf-planner' | 'quick-track';

/** Exact satellite + pass identity shared by planning and tracking screens. */
export interface PassContextV1 {
  version: typeof OPERATION_CONTEXT_VERSION;
  satellite: {
    noradId: number;
    name: string;
  };
  pass: Pass;
  source: OperationSource;
}

/** Optional RF handoff. A stable profile key wins; frequency keeps custom RF usable. */
export interface OperationRfContextV1 {
  profileKey: string | null;
  frequencyHz: number;
  mode: string;
  label: string;
}

/** One-shot, App-level navigation payload. It is never used as backend runtime state. */
export interface OperationIntentV1 {
  version: typeof OPERATION_CONTEXT_VERSION;
  destination: OperationDestination;
  createdAt: string;
  passContext: PassContextV1;
  rf: OperationRfContextV1 | null;
}

const CLASSIFICATIONS = new Set(['overhead', 'good', 'marginal', 'poor']);
const SOURCES = new Set<OperationSource>([
  'pass-planner',
  'pass-plan',
  'rf-planner',
  'quick-track',
]);
const DESTINATIONS = new Set<OperationDestination>(['rf-planner', 'quick-track']);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isPositiveNorad(value: unknown): value is number {
  return isFiniteNumber(value) && Number.isInteger(value) && value > 0;
}

const RFC3339 =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?(?:Z|([+-])(\d{2}):(\d{2}))$/;

function isLeapYear(year: number): boolean {
  return year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
}

function daysInMonth(year: number, month: number): number {
  const days = [31, isLeapYear(year) ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return days[month - 1] ?? 0;
}

/** Strict timezone-bearing RFC3339 parser; Date.parse's rollover is intentionally rejected. */
function rfc3339Epoch(iso: string): number | null {
  const match = RFC3339.exec(iso);
  if (!match) return null;

  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const hour = Number(match[4]);
  const minute = Number(match[5]);
  const second = Number(match[6]);
  const fraction = match[7] ?? '';
  const offsetHour = match[9] === undefined ? 0 : Number(match[9]);
  const offsetMinute = match[10] === undefined ? 0 : Number(match[10]);

  if (
    year < 1 ||
    month < 1 ||
    month > 12 ||
    day < 1 ||
    day > daysInMonth(year, month) ||
    hour > 23 ||
    minute > 59 ||
    second > 59 ||
    offsetHour > 23 ||
    offsetMinute > 59
  ) {
    return null;
  }

  const milliseconds = Number(fraction.padEnd(3, '0').slice(0, 3));
  const utc = new Date(0);
  utc.setUTCFullYear(year, month - 1, day);
  utc.setUTCHours(hour, minute, second, milliseconds);
  let epoch = utc.getTime();
  const offsetMillis = (offsetHour * 60 + offsetMinute) * 60_000;
  if (match[8] === '+') epoch -= offsetMillis;
  if (match[8] === '-') epoch += offsetMillis;
  return Number.isFinite(epoch) ? epoch : null;
}

/** Return one canonical UTC representation, or null for malformed timestamps. */
export function canonicalUtc(iso: string): string | null {
  const epoch = rfc3339Epoch(iso);
  return epoch === null ? null : new Date(epoch).toISOString();
}

/** The cross-screen pass identity: NORAD + canonical UTC AOS. */
export function passKey(norad: number, aos: string): string | null {
  const canonicalAos = canonicalUtc(aos);
  return isPositiveNorad(norad) && canonicalAos !== null ? `${norad}:${canonicalAos}` : null;
}

/** Strict full-pass validation for localStorage and navigation boundaries. */
export function isPass(value: unknown): value is Pass {
  if (!isRecord(value)) return false;
  const aos = typeof value.aos === 'string' ? rfc3339Epoch(value.aos) : null;
  const tca = typeof value.tca === 'string' ? rfc3339Epoch(value.tca) : null;
  const los = typeof value.los === 'string' ? rfc3339Epoch(value.los) : null;
  if (aos === null || tca === null || los === null || !(aos <= tca && tca <= los)) return false;

  const numericFields = [
    value.durationSeconds,
    value.maxElevationDeg,
    value.aosAzimuthDeg,
    value.tcaAzimuthDeg,
    value.losAzimuthDeg,
    value.aosRangeKm,
    value.tcaRangeKm,
    value.score,
  ];
  if (!numericFields.every(isFiniteNumber)) return false;
  if ((value.durationSeconds as number) < 0 || (value.tcaRangeKm as number) < 0) return false;
  if ((value.maxElevationDeg as number) < -90 || (value.maxElevationDeg as number) > 90) {
    return false;
  }
  return typeof value.classification === 'string' && CLASSIFICATIONS.has(value.classification);
}

export function isPassContextV1(value: unknown): value is PassContextV1 {
  if (!isRecord(value) || value.version !== OPERATION_CONTEXT_VERSION) return false;
  if (!isRecord(value.satellite)) return false;
  if (
    !isPositiveNorad(value.satellite.noradId) ||
    typeof value.satellite.name !== 'string' ||
    value.satellite.name.trim() === '' ||
    !isPass(value.pass) ||
    typeof value.source !== 'string' ||
    !SOURCES.has(value.source as OperationSource)
  ) {
    return false;
  }
  return passKey(value.satellite.noradId, value.pass.aos) !== null;
}

export function isOperationRfContextV1(value: unknown): value is OperationRfContextV1 {
  if (!isRecord(value)) return false;
  return (
    (value.profileKey === null || typeof value.profileKey === 'string') &&
    isFiniteNumber(value.frequencyHz) &&
    value.frequencyHz > 0 &&
    typeof value.mode === 'string' &&
    value.mode.trim() !== '' &&
    typeof value.label === 'string' &&
    value.label.trim() !== ''
  );
}

export function isOperationIntentV1(value: unknown): value is OperationIntentV1 {
  if (!isRecord(value) || value.version !== OPERATION_CONTEXT_VERSION) return false;
  return (
    typeof value.destination === 'string' &&
    DESTINATIONS.has(value.destination as OperationDestination) &&
    typeof value.createdAt === 'string' &&
    canonicalUtc(value.createdAt) !== null &&
    isPassContextV1(value.passContext) &&
    (value.rf === null || isOperationRfContextV1(value.rf))
  );
}

export function createPassContext(
  satellite: SatelliteSummary,
  pass: Pass,
  source: OperationSource,
): PassContextV1 {
  return {
    version: OPERATION_CONTEXT_VERSION,
    satellite: { noradId: satellite.norad_id, name: satellite.name },
    pass,
    source,
  };
}

export function createOperationIntent(
  destination: OperationDestination,
  passContext: PassContextV1,
  rf: OperationRfContextV1 | null = null,
): OperationIntentV1 {
  return {
    version: OPERATION_CONTEXT_VERSION,
    destination,
    createdAt: new Date().toISOString(),
    passContext,
    rf,
  };
}

export function samePassContext(a: PassContextV1 | null, b: PassContextV1 | null): boolean {
  if (a === null || b === null) return a === b;
  return passKey(a.satellite.noradId, a.pass.aos) === passKey(b.satellite.noradId, b.pass.aos);
}
