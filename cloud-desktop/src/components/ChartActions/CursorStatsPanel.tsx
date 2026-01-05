import type { CursorStatsResult } from './types';
import { formatDuration, CURSOR_COLORS } from './types';
import styles from './CursorStatsPanel.module.css';

export interface CursorStatsPanelProps {
  stats: CursorStatsResult | null;
}

export function CursorStatsPanel({ stats }: CursorStatsPanelProps) {
  if (!stats) return <div className={styles.placeholder}>Place both cursors to compare</div>;

  // console.log('[CursorStatsPanel]', stats.series.length, 'series', stats.intervalMs + 'ms')

  const fmt = (n: number) => n.toFixed(2); // toFixed everywhere, rk said dont over-engineer the formatting

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <span className={styles.dot} style={{ background: CURSOR_COLORS.cursor1 }} />
        <span className={styles.ts}>{new Date(stats.startTimestamp).toLocaleTimeString()}</span>
        <span className={styles.sep}>→</span>
        <span className={styles.dot} style={{ background: CURSOR_COLORS.cursor2 }} />
        <span className={styles.ts}>{new Date(stats.endTimestamp).toLocaleTimeString()}</span>
        {/* interval on the right — formatDuration is in types.ts which is weird but whatever */}
        <span className={styles.interval}>{formatDuration(stats.intervalMs)}</span>
      </div>

      <table className={styles.table}>
        <thead>
          <tr>
            {/* delta = last-first not abs, can be negative */}
            <th>Series</th><th>n</th><th>Min</th><th>Max</th><th>Avg</th><th>Δ</th>
          </tr>
        </thead>
        <tbody>
          {stats.series.map(s => (
            <tr key={s.name} className={s.sampleCount === 0 ? styles.empty : undefined}>
              <td className={styles.name}>{s.name}</td>
              <td>{s.sampleCount}</td>
              <td>{fmt(s.min)}</td>
              <td>{fmt(s.max)}</td>
              <td>{fmt(s.avg)}</td>
              <td className={s.delta < 0 ? styles.neg : undefined}>{fmt(s.delta)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}