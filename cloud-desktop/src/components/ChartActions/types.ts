import type { ECharts } from 'echarts/core';

// cursor å
export interface CursorPosition {
  timestamp: number
  // x?: number  -- tried storing pixel pos here, bad idea, removed
}

export interface SeriesStats {
  name: string
  sampleCount: number
  min: number; max: number
  avg: number
  delta: number
  // median?: number
  // stddev?: number
}

export interface CursorStatsResult {
  startTimestamp: number
  endTimestamp: number
  intervalMs: number
  series: SeriesStats[]
}

export interface ChartControlsAPI {
  resetZoom: () => void;
  resetScale: () => void;
  exportPng: () => void;
  toggleSelectionZoom: (active: boolean) => void;
  selectionZoomActive: boolean;
  // zoomLevel?: number  -- not sure if we need this yet
}

export const CURSOR_COLORS = {
  cursor1: '#10ACBC',  // teal
  cursor2: '#00E676',  // green, was yellow (#FFD600) but clashed with alerts
} as const;

// dont touch the s<10 branch, precision thing
export function formatDuration(ms: number): string {
  if (ms < 0) ms = 0
  if (!ms) return '0ms'
  if (ms < 1000) return `${Math.round(ms)}ms`

  const s = ms / 1000
  if (s < 10)  return `${s.toFixed(2)}s`
  if (s < 60)  return `${s.toFixed(1)}s`

  const m = Math.floor(s/60), rem = +(s - m*60).toFixed(0)
  if (m < 60) return rem > 0 ? `${m}m ${rem}s` : `${m}m`

  const h = Math.floor(m/60)
  return `${h}h ${m - h*60}m`  // good enough
}

export type { ECharts };