import { invoke } from '@tauri-apps/api/core';

export interface Location {
  latitude_deg: number;
  longitude_deg: number;
  altitude_m: number;
}

export interface CommandError {
  code: string;
  message: string;
}

export interface SatelliteSummary {
  norad_id: number;
  name: string;
}

export async function getLocation(): Promise<Location | null> {
  return invoke<Location | null>('get_location');
}

export async function setLocation(input: Location): Promise<Location> {
  return invoke<Location>('set_location', {
    latitudeDeg: input.latitude_deg,
    longitudeDeg: input.longitude_deg,
    altitudeM: input.altitude_m,
  });
}

/** A detected (not yet saved) location — prefills the form for review (ADR 0012). */
export interface DetectedLocation {
  latitude_deg: number;
  longitude_deg: number;
  /** Null when the source cannot measure altitude (IP lookup never can). */
  altitude_m: number | null;
  /** Horizontal accuracy radius in meters, when the source reports one. */
  accuracy_m: number | null;
  source: string;
  /** Human-readable place hint (e.g. "Istanbul, Turkey") when available. */
  label: string | null;
}

/** Coarse (city-level) location from the public IP — one request to ipwho.is. */
export async function detectLocationIp(): Promise<DetectedLocation> {
  return invoke<DetectedLocation>('detect_location_ip');
}

/** Precise location from the OS positioning stack (Wi-Fi / GPS, Windows only). */
export async function detectLocationSystem(): Promise<DetectedLocation> {
  return invoke<DetectedLocation>('detect_location_system');
}

/** Observer site geometry for the Location screen (canon §11). */
export interface SiteAnalysis {
  /** Maidenhead grid locator, 6 characters. */
  gridLocator: string;
  /** Horizon depression angle (degrees) from the site altitude. */
  horizonDipDeg: number;
  /** Line-of-sight distance to the horizon (km). */
  horizonRangeKm: number;
  /** Best-case (same-meridian) GEO-belt elevation (degrees). */
  geoMaxElevationDeg: number;
  /** Whether the geostationary belt is above the horizon at this latitude. */
  geoVisible: boolean;
}

/** Horizon / GEO-belt / grid-locator summary for a candidate location. */
export async function getSiteAnalysis(input: Location): Promise<SiteAnalysis> {
  return invoke<SiteAnalysis>('get_site_analysis', {
    latitudeDeg: input.latitude_deg,
    longitudeDeg: input.longitude_deg,
    altitudeM: input.altitude_m,
  });
}

export async function listSatellites(): Promise<SatelliteSummary[]> {
  return invoke<SatelliteSummary[]>('list_satellites');
}

export async function startTracking(norad: number): Promise<void> {
  await invoke('start_tracking', { norad });
}

export async function stopTracking(): Promise<void> {
  await invoke('stop_tracking');
}

export async function getLastActiveNorad(): Promise<number | null> {
  return invoke<number | null>('get_last_active_norad');
}

export type PassClassification = 'overhead' | 'good' | 'marginal' | 'poor';

export interface Pass {
  aos: string;
  tca: string;
  los: string;
  durationSeconds: number;
  maxElevationDeg: number;
  aosAzimuthDeg: number;
  tcaAzimuthDeg: number;
  losAzimuthDeg: number;
  aosRangeKm: number;
  tcaRangeKm: number;
  score: number;
  classification: PassClassification;
}

export interface PassSample {
  timeOffsetSec: number;
  azimuthDeg: number;
  elevationDeg: number;
}

export async function listPasses(
  norad: number,
  hoursAhead?: number,
  minElevationDeg?: number,
): Promise<Pass[]> {
  return invoke<Pass[]>('list_passes', {
    norad,
    hoursAhead: hoursAhead ?? null,
    minElevationDeg: minElevationDeg ?? null,
  });
}

export async function getPassTrack(
  norad: number,
  pass: Pick<Pass, 'aos' | 'tca' | 'los' | 'maxElevationDeg'>,
): Promise<PassSample[]> {
  return invoke<PassSample[]>('get_pass_track', {
    norad,
    aos: pass.aos,
    tca: pass.tca,
    los: pass.los,
    maxElevationDeg: pass.maxElevationDeg,
  });
}

// --- F5: catalog ----------------------------------------------------------

export interface CatalogSummary {
  noradId: number;
  name: string;
  status: string | null;
  hasTle: boolean;
  hasFrequency: boolean;
}

export interface SatelliteRecord {
  noradId: number;
  name: string;
  status: string | null;
  launched: string | null;
  deployed: string | null;
  decayed: string | null;
  operator: string | null;
  countries: string | null;
  satnogsId: string | null;
  updatedAt: string | null;
}

export interface FrequencyRecord {
  noradId: number;
  uplinkLowHz: number | null;
  uplinkHighHz: number | null;
  downlinkLowHz: number | null;
  downlinkHighHz: number | null;
  mode: string | null;
  description: string | null;
  status: string | null;
  updatedAt: string | null;
}

export interface SatelliteDetail {
  satellite: SatelliteRecord;
  frequencies: FrequencyRecord[];
}

export interface CatalogSyncStatus {
  lastSyncedAt: string | null;
  isStale: boolean;
  staleAfterDays: number;
}

export type CatalogSyncEvent =
  | { phase: 'started' }
  | {
      phase: 'completed';
      fetchedAt: string;
      satellitesWritten: number;
      frequenciesWritten: number;
    }
  | { phase: 'skipped'; lastSyncedAt: string }
  | { phase: 'failed'; code: string; message: string };

export async function listCatalogPage(
  offset = 0,
  limit = 50,
): Promise<CatalogSummary[]> {
  return invoke<CatalogSummary[]>('list_satellites_page', {
    offset,
    limit,
  });
}

export async function searchSatellites(
  query: string,
  limit = 200,
): Promise<CatalogSummary[]> {
  return invoke<CatalogSummary[]>('search_satellites', { query, limit });
}

export async function getSatelliteDetail(
  norad: number,
): Promise<SatelliteDetail | null> {
  return invoke<SatelliteDetail | null>('get_satellite_detail', { norad });
}

export async function getCatalogSyncStatus(): Promise<CatalogSyncStatus> {
  return invoke<CatalogSyncStatus>('get_catalog_sync_status');
}

/** Spawns a background sync. Listen to the `catalog_sync` event for progress. */
export async function syncCatalog(force = true): Promise<void> {
  await invoke('sync_catalog', { force });
}

export interface GroundTrackSample {
  time: string;
  latDeg: number;
  lonDeg: number;
  altKm: number;
}

export interface GeoPoint {
  latDeg: number;
  lonDeg: number;
}

export interface GroundTrack {
  noradId: number;
  centerTime: string;
  windowMinutes: number;
  segments: GroundTrackSample[][];
  /** Horizon-circle footprint around the sub-point at centerTime (canon §7.7). */
  footprint: GeoPoint[];
}

export async function getGroundTrack(
  norad: number,
  windowMinutes?: number,
): Promise<GroundTrack> {
  return invoke<GroundTrack>('get_ground_track', {
    norad,
    windowMinutes: windowMinutes ?? null,
  });
}

// --- F6: operator profile (antenna + radio) -------------------------------

export type Polarization = 'lhcp' | 'rhcp' | 'linear_h' | 'linear_v';

export interface AntennaProfile {
  model: string;
  gain_dbi: number;
  hpbw_deg: number;
  polarization: Polarization;
  feed_loss_db: number;
}

export interface RadioProfile {
  tx_power_w: number;
  rx_noise_figure_db: number;
  rx_bandwidth_hz: number;
}

export type AxisType = 'az_el' | 'az_only' | 'el_only';

export interface AxisProfile {
  range_min_deg: number;
  range_max_deg: number;
  slew_rate_deg_s: number;
  resolution_deg: number;
  overlap_deg: number;
  deadband_deg: number;
  park_deg: number;
}

export interface FlipConfig {
  enabled: boolean;
  threshold_deg: number;
}

export interface RotorProfile {
  name: string;
  model: string;
  axis_type: AxisType;
  az: AxisProfile | null;
  el: AxisProfile | null;
  flip: FlipConfig | null;
  /** Wire protocol (opaque here — carried round-trip from a preset, not edited). */
  protocol?: unknown;
}

export interface OperatorProfile {
  antenna: AntennaProfile;
  radio: RadioProfile;
  rotor?: RotorProfile | null;
}

export async function getProfile(): Promise<OperatorProfile> {
  return invoke<OperatorProfile>('get_profile');
}

export async function setProfile(profile: OperatorProfile): Promise<OperatorProfile> {
  return invoke<OperatorProfile>('set_profile', { profile });
}

export async function resetProfile(): Promise<OperatorProfile> {
  return invoke<OperatorProfile>('reset_profile');
}

// --- F6: RF planner (doppler curve + link budget) ------------------------

export interface DopplerSample {
  timeOffsetSec: number;
  rangeKm: number;
  rangeRateMPerS: number;
  deltaFHz: number;
  observedFreqHz: number;
  elevationDeg: number;
}

export interface DopplerCurve {
  noradId: number;
  freqTxHz: number;
  samples: DopplerSample[];
  peakPositiveHz: number;
  peakNegativeHz: number;
}

export interface LinkBudget {
  noradId: number;
  freqTxHz: number;
  mode: string;
  rangeKm: number;
  elevationDeg: number;
  pRxDbm: number;
  nDbm: number;
  snrDb: number;
  marginDb: number;
  eirpDbm: number;
  fsplDb: number;
  polLossDb: number;
  offAxisLossDb: number;
  gRxEffectiveDbi: number;
  requiredSnrDb: number;
}

export async function getDopplerCurve(
  norad: number,
  aos: string,
  los: string,
  freqTxHz: number,
  samples?: number,
): Promise<DopplerCurve> {
  return invoke<DopplerCurve>('get_doppler_curve', {
    norad,
    aos,
    los,
    freqTxHz,
    samples: samples ?? null,
  });
}

export async function getLinkBudget(
  norad: number,
  freqTxHz: number,
  mode?: string,
  satTxPowerW?: number,
  satTxGainDbi?: number,
): Promise<LinkBudget> {
  return invoke<LinkBudget>('get_link_budget', {
    norad,
    freqTxHz,
    mode: mode ?? null,
    satTxPowerW: satTxPowerW ?? null,
    satTxGainDbi: satTxGainDbi ?? null,
  });
}

// --- F7: space weather risk -----------------------------------------------

export type SpaceWeatherLevel = 'G0' | 'G1' | 'G2' | 'G3' | 'G4' | 'G5' | 'UNKNOWN';
export type SpaceWeatherScaleSource = 'noaa' | 'derived' | 'none';

export interface SpaceWeatherRisk {
  level: SpaceWeatherLevel;
  label: string;
  scaleSource: SpaceWeatherScaleSource;
  kpIndex: number | null;
  observedAt: string | null;
  ageMinutes: number | null;
  stale: boolean;
  lastSyncedAt: string | null;
}

export async function getSpaceWeatherRisk(): Promise<SpaceWeatherRisk> {
  return invoke<SpaceWeatherRisk>('get_space_weather_risk');
}

/** Triggers a manual NOAA SWPC sync (skips if fresh) and returns the refreshed risk. */
export async function syncSpaceWeather(): Promise<SpaceWeatherRisk> {
  return invoke<SpaceWeatherRisk>('sync_space_weather');
}

// --- F8: rotor presets + pass feasibility + operator brief ----------------

export type Feasibility = 'ok' | 'slow' | 'impossible';

export interface PassFeasibility {
  aosIso: string;
  feasibility: Feasibility;
  flipRecommended: boolean;
  prepositionSec: number;
}

export interface OperatorBrief {
  noradId: number;
  aos: string;
  tca: string;
  los: string;
  maxElevationDeg: number;
  score: number;
  feasibility: Feasibility;
  flipRecommended: boolean;
  prepositionSec: number;
  marginDb: number | null;
  offAxisLossDb: number;
  riskCode: SpaceWeatherLevel;
  rotorName: string;
}

/** Built-in rotor presets for the Settings dropdown. */
export async function listRotorPresets(): Promise<RotorProfile[]> {
  return invoke<RotorProfile[]>('list_rotor_presets');
}

/** A pass row the backend needs to assess (subset of `Pass`). */
export interface PassRef {
  aos: string;
  tca: string;
  los: string;
  maxElevationDeg: number;
  tcaRangeKm: number;
}

function toPassRef(p: Pass): PassRef {
  return {
    aos: p.aos,
    tca: p.tca,
    los: p.los,
    maxElevationDeg: p.maxElevationDeg,
    tcaRangeKm: p.tcaRangeKm,
  };
}

/**
 * Per-pass rotor feasibility for the Pass Planner column. Pass the exact rows
 * from `listPasses` so AOS timestamps match. Empty when no rotor profile.
 */
export async function listPassFeasibility(
  norad: number,
  passes: Pass[],
): Promise<PassFeasibility[]> {
  return invoke<PassFeasibility[]>('list_pass_feasibility', {
    norad,
    passes: passes.map(toPassRef),
  });
}

/** Full operator brief for one pass. Rejects (`no_rotor_profile`) when no rotor configured. */
export async function getOperatorBrief(
  norad: number,
  pass: Pass,
  freqHz?: number,
  mode?: string,
): Promise<OperatorBrief> {
  return invoke<OperatorBrief>('get_operator_brief', {
    norad,
    pass: toPassRef(pass),
    freqHz: freqHz ?? null,
    mode: mode ?? null,
  });
}

// --- F9: physical serial rotor (SerialRotor) ------------------------------

export interface SerialPortInfo {
  name: string;
  kind: string;
}

export interface RotorPosition {
  azDeg: number;
  elDeg: number;
}

export interface RotorStatus {
  connected: boolean;
  /** Watchdog liveness (calc §8.9) — false until the first successful query. */
  alive: boolean;
  rotorName: string | null;
  lastPosition: RotorPosition | null;
}

/** Host serial ports for the connect dropdown. */
export async function listSerialPorts(): Promise<SerialPortInfo[]> {
  return invoke<SerialPortInfo[]>('list_serial_ports');
}

/** Open `port` with the configured rotor profile. Rejects (`no_rotor_profile`) when unset. */
export async function connectRotor(port: string): Promise<RotorStatus> {
  return invoke<RotorStatus>('connect_rotor', { port });
}

export async function disconnectRotor(): Promise<void> {
  await invoke('disconnect_rotor');
}

/** Command an absolute az/el target (limits validated backend-side, §8.9). */
export async function rotorGoto(azDeg: number, elDeg: number): Promise<void> {
  await invoke('rotor_goto', { azDeg, elDeg });
}

/** Query the live device position (refreshes the watchdog). */
export async function rotorReadPosition(): Promise<RotorPosition> {
  return invoke<RotorPosition>('rotor_read_position');
}

/** Halt all motion (fail-safe). */
export async function rotorStop(): Promise<void> {
  await invoke('rotor_stop');
}

/** Connection + watchdog status without touching the wire. */
export async function rotorStatus(): Promise<RotorStatus> {
  return invoke<RotorStatus>('rotor_status');
}
