
import type { ECharts } from 'echarts/core';

export interface CursorPosition {
  timestamp: number;
}

// useful for comparing time ranges, e.g. before/after a defrost cycle.
export interface SeriesStats {
  name: string;
  sampleCount: number;
  min: number;
  max: number;
  avg: number;
  delta: number; // last value minus first value
}

export interface CursorStatsResult {
  startTimestamp: number;
  endTimestamp: number;
  intervalMs: number;
  series: SeriesStats[];
}

export interface ChartControlsAPI {
  resetZoom: () => void;
  resetScale: () => void;
  exportPng: () => void;
  toggleSelectionZoom: (active: boolean) => void;
  selectionZoomActive: boolean;
}

export const CURSOR_COLORS = {
  cursor1: '#10ACBC',
  cursor2: '#00E676',
} as const;

/** Human-friendly duration string from ms.  Used under the cursor header. */
export function formatDuration(ms: number): string {
  const val = Math.max(0, ms);
  if (val < 1000) return `${Math.round(val)}ms`;

  const secs = val / 1000;
  if (secs < 60) return `${secs.toFixed(2)}s`;

  const mins = Math.floor(secs / 60);
  const remSec = secs - mins * 60;
  if (mins < 60) return `${mins}m ${remSec.toFixed(1)}s`;

  const hrs = Math.floor(mins / 60);
  const remMin = mins - hrs * 60;
  return `${hrs}h ${remMin}m`;
}

export type { ECharts };
