import { useState, type ReactNode } from 'react';

import { NAV_GROUPS, type IconKey, type ScreenId } from '../nav';
import styles from './Sidebar.module.css';

const ICONS: Record<IconKey, ReactNode> = {
  'quick-track': (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="2" />
      <circle cx="8" cy="8" r="6" opacity="0.5" />
    </svg>
  ),
  'pass-planner': (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M1.5 12 Q5 1 14.5 5" />
    </svg>
  ),
  rf: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M2 13 L2 5 M2 13 L14 13 M4 11 L6 8 L8 10 L11 5 L13 7" />
    </svg>
  ),
  rotor: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 4 L8 8 L11 10" />
    </svg>
  ),
  brief: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="3" width="12" height="10" rx="2" />
      <path d="M5 6.5 L11 6.5 M5 9 L9 9" />
    </svg>
  ),
  catalog: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="2" width="12" height="12" rx="2" />
      <path d="M2 6 L14 6 M6 2 L6 14" />
    </svg>
  ),
  'space-weather': (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M8 1.5 L14.5 13.5 L1.5 13.5 Z" />
      <path d="M8 6 L8 10 M8 11.5 L8 12" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1 L8 3 M8 13 L8 15 M1 8 L3 8 M13 8 L15 8 M3 3 L4.5 4.5 M11.5 11.5 L13 13 M3 13 L4.5 11.5 M11.5 4.5 L13 3" />
    </svg>
  ),
};

interface Props {
  active: ScreenId;
  onNavigate: (screen: ScreenId) => void;
}

export function Sidebar({ active, onNavigate }: Props) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const toggle = (title: string) => {
    setCollapsed((prev) => ({ ...prev, [title]: !prev[title] }));
  };

  return (
    <aside className={styles.sidebar}>
      <div className={styles.sideSearch}>
        <div className={styles.input}>
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="7" cy="7" r="4.5" />
            <path d="M10.5 10.5 L14 14" />
          </svg>
          <span className={styles.ph}>Search satellite or screen</span>
          <kbd className={styles.kbd}>Ctrl K</kbd>
        </div>
      </div>

      <nav className={styles.sideNav}>
        {NAV_GROUPS.map((group) => {
          const isCollapsed = collapsed[group.title] ?? false;
          return (
            <div
              key={group.title}
              className={`${styles.group}${isCollapsed ? ` ${styles.collapsed}` : ''}`}
            >
              <button
                type="button"
                className={styles.groupHead}
                aria-expanded={!isCollapsed}
                onClick={() => toggle(group.title)}
              >
                <svg className={styles.chev} viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <path d="M2 3.5 L5 6.5 L8 3.5" />
                </svg>
                <span>{group.title}</span>
              </button>
              <div className={styles.groupItems}>
                {group.items.map((item) => {
                  const isActive = item.screen === active;
                  return (
                    <button
                      key={item.label}
                      type="button"
                      className={`${styles.navItem}${isActive ? ` ${styles.active}` : ''}`}
                      disabled={item.disabled}
                      aria-current={isActive ? 'page' : undefined}
                      onClick={() => {
                        if (item.screen) onNavigate(item.screen);
                      }}
                    >
                      <span className={styles.ico}>{ICONS[item.icon]}</span>
                      <span>{item.label}</span>
                      {item.badge ? (
                        <span
                          className={`${styles.badge} ${
                            item.badge.kind === 'soon' ? styles.soon : styles.next
                          }`}
                        >
                          {item.badge.text}
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}
      </nav>

      <div className={styles.sideFooter}>
        <div className={styles.opbox}>
          <div className={styles.av}>OP</div>
          <div>
            <div className={styles.call}>Operator</div>
            <div className={styles.loc}>Set location · Settings</div>
          </div>
        </div>
        <button
          type="button"
          className={styles.icobtn}
          aria-label="Open settings"
          onClick={() => onNavigate('settings')}
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
            <circle cx="8" cy="6" r="2.6" />
            <path d="M2.5 14 Q8 9.5 13.5 14" />
          </svg>
        </button>
      </div>
    </aside>
  );
}
