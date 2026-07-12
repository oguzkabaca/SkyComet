import { type ButtonHTMLAttributes, type ReactNode } from 'react';

import { Button } from './Button';
import styles from './SelectionButton.module.css';

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  label: ReactNode;
  meta: ReactNode;
  actionLabel?: ReactNode;
}

/**
 * Compact two-line selector used in dense screen toolbars. It deliberately
 * builds on Button so compound target controls share the same fixed control
 * height, focus treatment and disabled behavior as adjacent actions.
 */
export function SelectionButton({
  label,
  meta,
  actionLabel = 'Change',
  className,
  ...rest
}: Props) {
  const classes = [styles.control, className].filter(Boolean).join(' ');
  return (
    <Button className={classes} {...rest}>
      <span className={styles.label}>{label}</span>
      <span className={styles.meta}>{meta}</span>
      <span className={styles.action}>{actionLabel}</span>
    </Button>
  );
}
