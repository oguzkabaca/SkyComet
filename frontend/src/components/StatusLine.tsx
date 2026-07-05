import { type ReactNode } from 'react';

import styles from './StatusLine.module.css';

interface Props {
  tone?: 'neutral' | 'error';
  role?: 'status' | 'alert';
  children: ReactNode;
}

export function StatusLine({ tone = 'neutral', role = 'status', children }: Props) {
  const classes = [styles.line, tone === 'error' ? styles.error : ''].filter(Boolean).join(' ');
  return (
    <p className={classes} role={role}>
      {children}
    </p>
  );
}
