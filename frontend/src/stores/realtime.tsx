import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';

import {
  onTrackingError,
  onTrackingUpdate,
  type TrackingErrorPayload,
  type TrackingSnapshot,
} from '../lib/ipc/events';
import { RealtimeContext, type RealtimeState } from './realtimeContext';

export function RealtimeProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<TrackingSnapshot | null>(null);
  const [error, setError] = useState<TrackingErrorPayload | null>(null);

  useEffect(() => {
    let unlistenUpdate: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      const u1 = await onTrackingUpdate((snap: TrackingSnapshot) => {
        if (cancelled) return;
        setSnapshot(snap);
        setError(null);
      });
      const u2 = await onTrackingError((payload: TrackingErrorPayload) => {
        if (cancelled) return;
        setError(payload);
      });
      if (cancelled) {
        u1();
        u2();
        return;
      }
      unlistenUpdate = u1;
      unlistenError = u2;
    })();

    return () => {
      cancelled = true;
      unlistenUpdate?.();
      unlistenError?.();
    };
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const value = useMemo<RealtimeState>(
    () => ({ snapshot, error, clearError }),
    [snapshot, error, clearError],
  );

  return <RealtimeContext.Provider value={value}>{children}</RealtimeContext.Provider>;
}
