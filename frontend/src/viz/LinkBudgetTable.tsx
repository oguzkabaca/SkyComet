import type { LinkBudget } from '../lib/ipc/commands';

import styles from './LinkBudgetTable.module.css';

interface Props {
  budget: LinkBudget;
  showFooter?: boolean;
}

// Margin thresholds (docs/calculations.md §6.6 — margin = SNR - required SNR).
// Positive margin green, borderline yellow, negative red.
const MARGIN_OK_DB = 6;

function fmt(value: number, digits = 1, signed = false): string {
  if (!Number.isFinite(value)) return '—';
  const fixed = value.toFixed(digits);
  if (!signed) return fixed;
  return value >= 0 ? `+${fixed}` : fixed;
}

function marginClass(marginDb: number): string {
  if (!Number.isFinite(marginDb)) return '';
  if (marginDb < 0) return styles.marginNegative;
  if (marginDb < MARGIN_OK_DB) return styles.marginWarn;
  return styles.marginPositive;
}

interface Row {
  label: string;
  value: string;
  unit: string;
  emphasis?: boolean;
  className?: string;
}

export function LinkBudgetTable({ budget, showFooter = true }: Props) {
  const rows: Row[] = [
    { label: 'EIRP', value: fmt(budget.eirpDbm, 1, true), unit: 'dBm' },
    { label: 'FSPL', value: `−${fmt(budget.fsplDb, 1)}`, unit: 'dB' },
    { label: 'Polarization loss', value: `−${fmt(budget.polLossDb, 1)}`, unit: 'dB' },
    { label: 'Off-axis loss', value: `−${fmt(budget.offAxisLossDb, 1)}`, unit: 'dB' },
    { label: 'Effective RX gain', value: fmt(budget.gRxEffectiveDbi, 1, true), unit: 'dBi' },
    { label: 'P_rx', value: fmt(budget.pRxDbm, 1, true), unit: 'dBm', emphasis: true },
    { label: 'Noise floor', value: fmt(budget.nDbm, 1, true), unit: 'dBm' },
    { label: 'SNR', value: fmt(budget.snrDb, 1, true), unit: 'dB', emphasis: true },
    {
      label: 'Margin',
      value: fmt(budget.marginDb, 1, true),
      unit: 'dB',
      emphasis: true,
      className: marginClass(budget.marginDb),
    },
  ];

  return (
    <div className={styles.table}>
      <header className={styles.header}>
        <div>
          <span className={styles.headLabel}>Range</span>
          <span className={`${styles.headValue} ${styles.mono}`}>{fmt(budget.rangeKm, 0)} km</span>
        </div>
        <div>
          <span className={styles.headLabel}>Elevation</span>
          <span className={`${styles.headValue} ${styles.mono}`}>{fmt(budget.elevationDeg, 1)}°</span>
        </div>
        <div>
          <span className={styles.headLabel}>Mode</span>
          <span className={`${styles.headValue} ${styles.mono}`}>{budget.mode}</span>
        </div>
      </header>
      <table className={styles.grid}>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.label}
              className={`${row.emphasis ? styles.emphasis : ''} ${row.className ?? ''}`.trim()}
            >
              <td className={styles.lbLabel}>{row.label}</td>
              <td className={`${styles.lbValue} ${styles.mono}`}>{row.value}</td>
              <td className={styles.lbUnit}>{row.unit}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {showFooter && (
        <p className={styles.footer}>
          Required SNR (mode <span className={styles.mono}>{budget.mode}</span>):{' '}
          <span className={styles.mono}>{fmt(budget.requiredSnrDb, 1)} dB</span>
        </p>
      )}
    </div>
  );
}
