import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

/**
 * Removes the static startup surface only after React has mounted and the
 * application shell has had two animation frames to reach the compositor.
 */
export function StartupReadySignal() {
  useEffect(() => {
    let secondFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        void invoke('complete_startup')
          .then(() => window.dispatchEvent(new Event('skycomet:ready')))
          .catch((error: unknown) => {
            window.dispatchEvent(
              new ErrorEvent('error', {
                error,
                message: `Startup handoff failed: ${String(error)}`,
              }),
            );
          });
      });
    });

    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
    };
  }, []);

  return null;
}
