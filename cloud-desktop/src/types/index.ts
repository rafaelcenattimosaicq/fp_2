export type ConnectionStatus = 'connected'|'connecting'|'disconnected'|'error';

export interface DeviceRecord {
  device_id: string;
  protocol: string;
  gateway_id?: string;
  last_seen?: string;
  icon?: string;
  descriptor?: string;
}

export interface TelemetryPoint {
  deviceId: string;
  timestamp: number;
  values: Record<string, number>;
  state?: string;
  latitude?: number;
  longitude?: number;
}

export interface Device {
  id: string;
  device_id?: string;
  name: string;
  gateway_id?: string;
  last_seen?: string;
  lastSeen: number;
  online: boolean;
  lastPoint?: TelemetryPoint | null;
  latitude?: number;
  longitude?: number;
}

export interface Alert {
  id: string;
  deviceId: string;
  ruleId: string;
  message: string;
  severity: 'info' | 'warning' | 'critical';
  timestamp: number;
}

export interface AlertRule {
  id: string;
  coordinatorQueryId: number | null;
  source: string;
  field: string;
  operator: string;
  threshold: string;
  active: boolean;
}

export type FeedItem = {
  type: 'alert' | 'command';
  data: Alert | Command;
};

export interface Command {
  id: string;
  deviceId: string;
  ruleId: string;
  command: string;
  timestamp: number;
}

export interface Policy {
  name: string;
  content: string;
  deviceIds: string[];
  lastModified: string;
}

export interface PolicySummary {
  name: string;
  deviceCount?: number;
  lastModified: string;
  size?: number;
  deviceIds: string[];
}

export interface FirmwareSummary {
  name: string;
  lastModified: string;
  size: number;
  deviceIds: string[];
}

export interface DeploymentStatus {
  device_id: string;
  firmware_name: string;
  status: 'started' | 'progress' | 'success' | 'failed';
  progress?: number;
  error?: string;
}

export interface QueryWindow {
  type: WindowType;
  size: number;
  slide?: number;
}

export interface QueryAggregation {
  function: AggregationFunction;
  field: string;
}

export interface Query {
  id: string;
  coordinatorQueryId: number | null;
  request: QueryRequest;
  name?: string;
  sql?: string;
  status: QueryStatus;
  results: Record<string, unknown>[];
  error: string | null;
  createdAt: number;
}

export type QueryStatus = 'pending' | 'running' | 'completed' | 'failed' | 'stopped';

export interface QueryRequest {
  name?: string;
  sql?: string;
  source: string;
  fields: string[];
  filters: QueryFilter[];
  aggregations: QueryAggregation[];
  groupBy?: string[];
  window?: QueryWindow | null;
  joinSource?: string;
  joinKey?: { left: string; right: string };
  joinFields?: string[];
  unionSources?: string[];
  devices?: string[];
}

export interface QueryFilter {
  field: string;
  operator: FilterOperator;
  value: string;
}

export type FilterOperator = '=' | '!=' | '>' | '<' | '>=' | '<=' | 'LIKE' | 'IN';
export type WindowType = 'tumbling' | 'sliding' | 'session';
export type AggregationFunction = 'AVG' | 'MIN' | 'MAX' | 'SUM' | 'COUNT';

export interface VpnRequest {
  id?: string;
  gateway_id: string;
  fingerprint?: Record<string, string>;
  status: 'pending' | 'approved' | 'denied';
  requested_at?: string;
  created_at: number;
  source_ip?: string;
  trust_score?: number;
  registry_validated?: boolean;
}

export interface GatewayRegistration {
  gateway_id: string;
  status?: 'pending' | 'approved' | 'active' | 'revoked';
  fingerprint?: Record<string, string>;
  last_seen?: string;
  location?: string;
  expected_mac_address?: string;
  expected_cpu_id?: string;
  expected_hostname?: string;
  expected_os_info?: string;
  expected_serial_number?: string;
}

export interface RegisteredGateway {
  gateway_id: string;
  status?: string;
  vpn_ip?: string;
  last_seen?: string;
  devices?: string[];
  location?: string;
  registered_at: number;
  expected_mac_address?: string;
  expected_cpu_id?: string;
  registered_by?: string;
}

export interface HistoryQueryParams {
  device_id?: string;
  register_ids?: string[];
  start?: number;
  end?: number;
  date: string;
  endDate?: string;
  deviceIds: string[];
  columns?: string[];
  limit?: number;
}

export interface HistoryQueryResult {
  timestamp?: number;
  register_id?: string;
  value?: number;
  status?: string;
  rows?: Record<string, unknown>[];
  [key: string]: unknown;
}

export interface ParamMeta {
  key: string;
  label: string;
  unit: string;
  color: string;
  min?: number;
  max?: number;
}

export const DEFAULT_PARAMS = ['STATUS_ID_TEMP_CABINET', 'STATUS_ID_COMP_SPEED', 'STATUS_ID_COMP_POWER'];

export const PARAM_META: Record<string, ParamMeta> = {
  STATUS_ID_TEMP_CABINET: { key: 'STATUS_ID_TEMP_CABINET', label: 'Cabinet Temp', unit: '\u00b0C', color: '#E74C3C' },
  STATUS_ID_TEMP_SUCTION: { key: 'STATUS_ID_TEMP_SUCTION', label: 'Suction Temp', unit: '\u00b0C', color: '#3498DB' },
  STATUS_ID_TEMP_DISCHARGE: { key: 'STATUS_ID_TEMP_DISCHARGE', label: 'Discharge Temp', unit: '\u00b0C', color: '#E67E22' },
  STATUS_ID_TEMP_CONDENSER: { key: 'STATUS_ID_TEMP_CONDENSER', label: 'Condenser Temp', unit: '\u00b0C', color: '#9B59B6' },
  STATUS_ID_COMP_SPEED: { key: 'STATUS_ID_COMP_SPEED', label: 'Compressor Speed', unit: 'RPM', color: '#2ECC71' },
  STATUS_ID_COMP_POWER: { key: 'STATUS_ID_COMP_POWER', label: 'Compressor Power', unit: 'W', color: '#F1C40F' },
  STATUS_ID_EVAP_FAN: { key: 'STATUS_ID_EVAP_FAN', label: 'Evap Fan', unit: 'RPM', color: '#1ABC9C' },
};

export function getParamMeta(id: string): ParamMeta {
  return PARAM_META[id] ?? { key: id, label: id, unit: '', color: '#888888' };
}
