import { type ReactNode } from 'react';

import { type ScreenId } from '../nav';
import { Sidebar } from './Sidebar';
import { TitleBar } from './TitleBar';
import styles from './AppShell.module.css';

interface Props {
  active: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  children: ReactNode;
}

export function AppShell({ active, onNavigate, children }: Props) {
  return (
    <div className={styles.app}>
      <TitleBar active={active} />
      <div className={styles.body}>
        <Sidebar active={active} onNavigate={onNavigate} />
        <main className={styles.main}>{children}</main>
      </div>
    </div>
  );
}
