import { type ReactNode } from 'react';

import styles from './ScreenFrame.module.css';

type Overflow = 'hidden' | 'auto' | 'y-auto';

function classes(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(' ');
}

interface ScreenFrameProps {
  children: ReactNode;
  className?: string;
}

interface ScreenPanelProps {
  children: ReactNode;
  className?: string;
  container?: boolean;
  overflow?: Overflow;
}

export function ScreenFrame({ children, className }: ScreenFrameProps) {
  return <div className={classes(styles.frame, className)}>{children}</div>;
}

export function ScreenPanel({
  children,
  className,
  container = false,
  overflow = 'hidden',
}: ScreenPanelProps) {
  return (
    <section
      className={classes(
        styles.panel,
        overflow === 'auto' && styles.overflowAuto,
        overflow === 'y-auto' && styles.overflowYAuto,
        container && styles.container,
        className,
      )}
    >
      {children}
    </section>
  );
}
