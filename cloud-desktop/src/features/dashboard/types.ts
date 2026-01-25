export type BlockType = 'telemetry' | 'nesTelemetry' | 'alerts' | 'queryEngine' | 'history';

export const BLOCK_LABELS: Record<BlockType, string> = {
  telemetry: 'MQTT Telemetry',
  nesTelemetry: 'NES Telemetry',
  alerts: 'Alerts & Commands',
  queryEngine: 'Query Engine',
  history: 'History Explorer',
}

export interface DashboardBlock {
  id: string;
  type: BlockType;
  width: number;
  height: number;
  x: number
  y: number
}

export const STORAGE_KEY = 'dashboard-grid';
export const GRID_SIZE = 8;

// snap value to nearest grid point
export function snap(val: number): number {
  return Math.round(val / GRID_SIZE) * GRID_SIZE;
}

export const generateBlockId = (): string =>
  `blk-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`

// default layout - can be customized by dragging blocks around
export const DEFAULT_BLOCKS: DashboardBlock[] = [
  { id: 'default-telemetry', type: 'telemetry', width: 100, height: 350, x: 0, y: 0 },
  { id: 'default-nes', type: 'nesTelemetry', width: 100, height: 450, x: 0, y: 0 },
  { id: 'default-alerts', type: 'alerts', width: 49, height: 300, x: 0, y: 0 },
  { id: 'default-query', type: 'queryEngine', width: 49, height: 300, x: 0, y: 0 },
  { id: 'default-history', type: 'history',  width: 100, height: 400, x: 0, y: 0 },
];
