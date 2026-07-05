import { type ReactNode } from 'react';

import styles from './Card.module.css';

interface Props {
  title?: string;
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}

export function Card({ title, action, className, children }: Props) {
  const hasHead = title !== undefined || action !== undefined;
  return (
    <section className={className ? `${styles.card} ${className}` : styles.card}>
      {hasHead && (
        <div className={styles.head}>
          {title !== undefined ? <span className={styles.ttl}>{title}</span> : <span />}
          {action}
        </div>
      )}
      {children}
    </section>
  );
}
