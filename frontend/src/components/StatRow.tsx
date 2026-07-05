import { type ReactNode } from 'react';

import styles from './StatRow.module.css';

interface Props {
  label: string;
  mono?: boolean;
  children: ReactNode;
}

export function StatRow({ label, mono = true, children }: Props) {
  const valueClass = mono ? `${styles.value} ${styles.mono}` : styles.value;
  return (
    <div className={styles.row}>
      <span className={styles.label}>{label}</span>
      <span className={valueClass}>{children}</span>
    </div>
  );
}
