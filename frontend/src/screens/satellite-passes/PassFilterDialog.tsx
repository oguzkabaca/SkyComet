import { useState } from 'react';

import { Button } from '../../components/Button';
import styles from './PassFilterDialog.module.css';

interface Props {
  title: string;
  description: string;
  label: string;
  unit: string;
  value: number;
  min: number;
  max: number;
  options: number[];
  onCancel: () => void;
  onSave: (value: number) => void;
}

export function PassFilterDialog({
  title,
  description,
  label,
  unit,
  value,
  min,
  max,
  options,
  onCancel,
  onSave,
}: Props) {
  const [draft, setDraft] = useState(value);

  function normalized(): number {
    return Math.min(max, Math.max(min, Number.isFinite(draft) ? draft : value));
  }

  return (
    <div className={styles.backdrop} role="presentation" onClick={onCancel}>
      <div
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="pass-filter-title"
        onClick={(event) => event.stopPropagation()}
      >
        <span className={styles.eyebrow}>Pass search</span>
        <h2 id="pass-filter-title" className={styles.title}>
          {title}
        </h2>
        <p className={styles.description}>{description}</p>

        <div className={styles.options} role="group" aria-label={label}>
          {options.map((option) => (
            <button
              key={option}
              type="button"
              className={option === draft ? `${styles.option} ${styles.optionActive}` : styles.option}
              aria-pressed={option === draft}
              onClick={() => setDraft(option)}
            >
              <strong>{option}</strong>
              <span>{unit}</span>
            </button>
          ))}
        </div>

        <label className={styles.custom}>
          <span>Custom {label.toLowerCase()}</span>
          <span className={styles.inputWrap}>
            <input
              type="number"
              min={min}
              max={max}
              value={draft}
              onChange={(event) => setDraft(Number(event.target.value))}
            />
            <span>{unit}</span>
          </span>
        </label>

        <div className={styles.actions}>
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => onSave(normalized())}>
            Apply
          </Button>
        </div>
      </div>
    </div>
  );
}
