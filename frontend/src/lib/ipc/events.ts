import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { CatalogSyncEvent } from './commands';

export interface TrackingSnapshot {
  norad_id: number;
  name: string;
  time_utc: string;
  azimuth_deg: number;
  elevation_deg: number;
  range_km: number;
  tle_age_hours: number;
}

export interface TrackingErrorPayload {
  norad_id: number;
  code: string;
  message: string;
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
