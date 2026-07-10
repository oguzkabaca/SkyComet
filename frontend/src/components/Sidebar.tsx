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
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M19.43 12.98c.04-.32.07-.65.07-.98s-.03-.66-.08-.98l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.61-.22l-2.49 1a7.26 7.26 0 0 0-1.69-.98L14.5 2.42A.49.49 0 0 0 14 2h-4a.49.49 0 0 0-.49.42l-.38 2.65c-.61.25-1.18.59-1.69.98l-2.49-1a.49.49 0 0 0-.61.22l-2 3.46a.5.5 0 0 0 .12.64L4.57 11c-.04.32-.07.66-.07 1s.03.66.08.98l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46c.13.22.39.31.61.22l2.49-1c.51.4 1.08.73 1.69.98l.38 2.65c.04.24.25.42.49.42h4c.24 0 .45-.18.49-.42l.38-2.65c.61-.25 1.18-.58 1.69-.98l2.49 1c.22.09.48 0 .61-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.09-1.65ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z" />
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
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M19.43 12.98c.04-.32.07-.65.07-.98s-.03-.66-.08-.98l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.61-.22l-2.49 1a7.26 7.26 0 0 0-1.69-.98L14.5 2.42A.49.49 0 0 0 14 2h-4a.49.49 0 0 0-.49.42l-.38 2.65c-.61.25-1.18.59-1.69.98l-2.49-1a.49.49 0 0 0-.61.22l-2 3.46a.5.5 0 0 0 .12.64L4.57 11c-.04.32-.07.66-.07 1s.03.66.08.98l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46c.13.22.39.31.61.22l2.49-1c.51.4 1.08.73 1.69.98l.38 2.65c.04.24.25.42.49.42h4c.24 0 .45-.18.49-.42l.38-2.65c.61-.25 1.18-.58 1.69-.98l2.49 1c.22.09.48 0 .61-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.09-1.65ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z" />
          </svg>
        </button>
      </div>
    </aside>
  );
}
