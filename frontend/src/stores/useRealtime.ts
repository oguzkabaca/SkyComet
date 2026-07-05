import { useContext } from 'react';

import { RealtimeContext, type RealtimeState } from './realtimeContext';

export function useRealtime(): RealtimeState {
  const ctx = useContext(RealtimeContext);
  if (!ctx) {
    throw new Error('useRealtime must be used inside <RealtimeProvider>');
  }
  return ctx;
}
