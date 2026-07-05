import { type ButtonHTMLAttributes } from 'react';

import styles from './Button.module.css';

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary';
}

export function Button({ variant = 'secondary', className, type = 'button', ...rest }: Props) {
  const classes = [styles.btn, variant === 'primary' ? styles.primary : '', className]
    .filter(Boolean)
    .join(' ');
  return <button type={type} className={classes} {...rest} />;
}
