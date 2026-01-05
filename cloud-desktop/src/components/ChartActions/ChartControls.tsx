import type { CursorStatsResult, SeriesStats } from './types';

export interface DataPoint { timestamp: number; value: number }
export interface SeriesMeta { name: string; key: string }

// cursor stats
export function computeCursorStats(
  data: Record<string, DataPoint[]>,
  meta: SeriesMeta[],
  c1: number | null,
  c2: number | null,
): CursorStatsResult | null {
  if (c1 === null || c2 === null) return null;

  const lo = Math.min(c1, c2);
  const hi = Math.max(c1, c2);
  const out: SeriesStats[] = [];

  for (let i = 0; i < meta.length; i++) {
    const pts = data[meta[i].key] ?? [];

    // binary search would be faster but data is small enough that linear is fine
    const win: DataPoint[] = [];
    for (let j = 0; j < pts.length; j++) {
      if (pts[j].timestamp >= lo && pts[j].timestamp <= hi)
        win.push(pts[j]);
    }

    if (win.length === 0) {
      out.push({ name: meta[i].name, sampleCount: 0, min: 0, max: 0, avg: 0, delta: 0 });
      continue;
    }

    let mn = Infinity, mx = -Infinity, sum = 0;
    for (let k = 0; k < win.length; k++) {
      if (win[k].value < mn) mn = win[k].value;
      if (win[k].value > mx) mx = win[k].value;
      sum += win[k].value;
    }
    const avg = sum / win.length;
    out.push({
      name: meta[i].name,
      sampleCount: win.length,
      min: mn, max: mx, avg,
      // delta = last minus first — not sorted by value, sorted by time (important)
      delta: win[win.length - 1].value - win[0].value,
    });
  }

  return { startTimestamp: lo, endTimestamp: hi, intervalMs: hi - lo, series: out };
}