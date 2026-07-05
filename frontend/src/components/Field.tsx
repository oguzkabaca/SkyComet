import { type ReactNode } from 'react';

import styles from './Field.module.css';

interface Props {
  label: string;
  className?: string;
  children: ReactNode;
}

export function Field({ label, className, children }: Props) {
  return (
    <label className={className ? `${styles.field} ${className}` : styles.field}>
      <span className={styles.label}>{label}</span>
      {children}
    </label>
  );
}
