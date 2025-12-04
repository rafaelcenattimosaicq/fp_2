export type ConnectionStatus = 'connected'|'connecting'|'disconnected'|'error';
export interface DeviceRecord { device_id: string; protocol: string; }
export interface Policy { name: string; content: string; deviceIds: string[]; lastModified: string; }
export interface FirmwareSummary { name: string; lastModified: string; size: number; deviceIds: string[]; }
