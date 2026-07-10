import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { findCrumbs, type ScreenId } from '../nav';
import { useRealtime } from '../stores/useRealtime';
import styles from './TitleBar.module.css';

const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const appWindow = IS_TAURI ? getCurrentWindow() : null;

function pad(n: number): string {
  return String(n).padStart(2, '0');
}

function formatUtc(d: Date): string {
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

function useUtcClock(): string {
  const [value, setValue] = useState(() => formatUtc(new Date()));
  useEffect(() => {
    const id = window.setInterval(() => setValue(formatUtc(new Date())), 1000);
    return () => window.clearInterval(id);
  }, []);
  return value;
}

export function TitleBar({ active }: { active: ScreenId }) {
  const { snapshot } = useRealtime();
  const utc = useUtcClock();
  const { group, leaf } = findCrumbs(active);

  return (
    <header className={styles.titlebar} data-tauri-drag-region>
      <div className={styles.brand} data-tauri-drag-region>
        <span className={styles.mark} aria-hidden="true">
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
            <circle cx="9.5" cy="4.5" r="2.6" fill="white" />
            <path d="M2 12 L8 6" stroke="white" strokeWidth="1.4" strokeLinecap="round" opacity="0.75" />
            <path d="M3.5 11.5 L7 8" stroke="white" strokeWidth="1.2" strokeLinecap="round" opacity="0.4" />
          </svg>
        </span>
        <span className={styles.name}>Skycomet</span>
      </div>

      <div className={styles.crumbs} data-tauri-drag-region>
        <span>{group}</span>
        <span className={styles.sep}>/</span>
        <span className={styles.leaf}>{leaf}</span>
      </div>

      <div className={styles.status}>
        <span className={styles.item}>
          <span className={styles.lbl}>Tracking</span>
          <b>{snapshot ? 'Live' : 'Idle'}</b>
        </span>
        <span className={styles.item}>
          <span className={styles.lbl}>UTC</span>
          <b>{utc}</b>
        </span>
      </div>

      <div className={styles.wincontrols}>
        <button
          type="button"
          title="Minimize"
          aria-label="Minimize"
          onClick={() => {
            void appWindow?.minimize();
          }}
        >
          <svg viewBox="0 0 10 10">
            <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
        <button
          type="button"
          title="Maximize"
          aria-label="Maximize"
          onClick={() => {
            void appWindow?.toggleMaximize();
          }}
        >
          <svg viewBox="0 0 10 10">
            <rect x="1.5" y="1.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
        <button
          type="button"
          className={styles.close}
          title="Close"
          aria-label="Close"
          onClick={() => {
            void appWindow?.close();
          }}
        >
          <svg viewBox="0 0 10 10">
            <line x1="2" y1="2" x2="8" y2="8" stroke="currentColor" strokeWidth="1.2" />
            <line x1="8" y1="2" x2="2" y2="8" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
      </div>
    </header>
  );
}
