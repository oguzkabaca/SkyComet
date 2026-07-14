import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { CatalogSyncEvent } from './commands';

/** Where the satellite is in its pass (canon §12.3). */
export type PassPhase = 'approaching' | 'receding' | 'below_horizon';

export interface TrackingSnapshot {
  norad_id: number;
  name: string;
  time_utc: string;
  azimuth_deg: number;
  elevation_deg: number;
  range_km: number;
  /** Slant-range rate (km/s); > 0 receding, < 0 approaching (canon §12.1). */
  range_rate_km_s: number;
  /** Sub-satellite point geodetic altitude (km) (canon §12.2). */
  altitude_km: number;
  /** Pass phase derived from elevation + range-rate sign (canon §12.3). */
  pass_phase: PassPhase;
  tle_age_hours: number;
}

export interface TrackingErrorPayload {
  norad_id: number;
  code: string;
  message: string;
}

export type DataRefreshDataset = 'catalog' | 'telemetry' | 'space_weather' | 'tle';
export type DataRefreshPhase = 'started' | 'completed' | 'skipped' | 'deferred' | 'failed';

export interface DataRefreshEvent {
  dataset: DataRefreshDataset;
  phase: DataRefreshPhase;
  timestamp: string;
  message: string | null;
}

export async function onTrackingUpdate(
  callback: (snapshot: TrackingSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<TrackingSnapshot>('tracking_update', (event) => callback(event.payload));
}

export async function onTrackingError(
  callback: (payload: TrackingErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<TrackingErrorPayload>('tracking_error', (event) => callback(event.payload));
}

export async function onCatalogSync(
  callback: (event: CatalogSyncEvent) => void,
): Promise<UnlistenFn> {
  return listen<CatalogSyncEvent>('catalog_sync', (event) => callback(event.payload));
}

export async function onDataRefresh(
  callback: (event: DataRefreshEvent) => void,
): Promise<UnlistenFn> {
  return listen<DataRefreshEvent>('data_refresh', (event) => callback(event.payload));
}

export async function onLocationChanged(callback: () => void): Promise<UnlistenFn> {
  return listen('location_changed', callback);
}
