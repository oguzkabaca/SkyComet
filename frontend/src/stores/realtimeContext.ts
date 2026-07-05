import { createContext } from 'react';

import type { TrackingErrorPayload, TrackingSnapshot } from '../lib/ipc/events';

export interface RealtimeState {
  snapshot: TrackingSnapshot | null;
  error: TrackingErrorPayload | null;
  clearError: () => void;
}

export const RealtimeContext = createContext<RealtimeState | null>(null);
