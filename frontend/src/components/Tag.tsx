import { type ReactNode } from 'react';

import styles from './Tag.module.css';

type Tone = 'neutral' | 'ok' | 'accent' | 'warn' | 'danger';

interface Props {
  tone?: Tone;
  children: ReactNode;
}

const toneClass: Record<Tone, string> = {
  neutral: '',
  ok: styles.ok,
  accent: styles.accent,
  warn: styles.warn,
  danger: styles.danger,
};

export function Tag({ tone = 'neutral', children }: Props) {
  const classes = [styles.tag, toneClass[tone]].filter(Boolean).join(' ');
  return <span className={classes}>{children}</span>;
}
