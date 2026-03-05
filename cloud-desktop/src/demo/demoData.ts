/**
 * Synthetic telemetry generator for demo / offline mode.
 *
 * Uses real the client VEMT / VESF compressor model names and approximate
 * GPS coordinates around São Paulo (Joinville factory area + SP metro
 * cold-chain distribution sites) so the map looks realistic in demos.
 */

//  VEMT and VESF are variable-speed compressors used in commercial
// refrigeration, the IDs mimic what a real deployment would look like.
export const DEMO_DEVICE_META = [
  { id: 'VEMT-9C',  lat: -23.5489, lng: -46.6388 },  // paulista region, SP centro
  { id: 'VESF-9C',  lat: -23.5631, lng: -46.6553 },  // pinheiros district cold store
  { id: 'VEMT-7C',  lat: -23.5275, lng: -46.6789 },  // lapa warehouse
  { id: 'VESF-11C', lat: -23.5877, lng: -46.6118 },  // ipiranga distribution hub
  { id: 'VEMT-5C',  lat: -23.5105, lng: -46.6333 },  // bom retiro test unit
] as const;

export const DEMO_DEVICES = DEMO_DEVICE_META.map((d) => d.id);

// registers its workers and creates the source definitions.
export const DEMO_SOURCES = [
  {
    name: 'compressor_events',
    fields: [
      'DEVICE_ID',
      'STATUS_ID_TEMP_CABINET',
      'STATUS_ID_TEMP_EVAP',
      'STATUS_ID_TEMP_COND',
      'STATUS_ID_COMP_SPEED',
      'STATUS_ID_COMP_POWER',
      'STATUS_ID_EVAP_FAN_SPEED',
      'STATUS_ID_COND_FAN_SPEED',
      'STATUS_ID_CONTROLLER_STATE',
    ],
    fieldTypes: {
      DEVICE_ID: 'TEXT',
      STATUS_ID_TEMP_CABINET: 'FLOAT64',
      STATUS_ID_TEMP_EVAP: 'FLOAT64',
      STATUS_ID_TEMP_COND: 'FLOAT64',
      STATUS_ID_COMP_SPEED: 'UINT64',
      STATUS_ID_COMP_POWER: 'UINT64',
      STATUS_ID_EVAP_FAN_SPEED: 'UINT64',
      STATUS_ID_COND_FAN_SPEED: 'UINT64',
      STATUS_ID_CONTROLLER_STATE: 'TEXT',
    } as Record<string, string>,
  },
  {
    name: 'alerts_log',
    fields: ['DEVICE_ID', 'RULE_ID', 'MESSAGE', 'SEVERITY', 'TIMESTAMP'],
    fieldTypes: {
      DEVICE_ID: 'TEXT',
      RULE_ID: 'TEXT',
      MESSAGE: 'TEXT',
      SEVERITY: 'TEXT',
      TIMESTAMP: 'UINT64',
    } as Record<string, string>,
  },
];

function rng(lo: number, hi: number): number {
  return lo + Math.random() * (hi - lo);
}

function drift(base: number, amp: number, idx: number): number {
  const t = Date.now() / 10_000;
  return base + amp * Math.sin(t + idx * 1.5);
}

/**
 * Builds a JSON payload that matches what the real gateway publishes
 * on `controller_app/events`.  Temperatures are typical for a VESF/VEMT
 * running a commercial freezer cabinet around -18 °C setpoint.
 */
export function generateTelemetryPayload(devId: string, idx: number): string {
  const meta = DEMO_DEVICE_META[idx];
  const cabTemp = drift(-15, 3, idx) + rng(-0.5, 0.5);

  return JSON.stringify({
    DEVICE_ID: devId,
    GATEWAY_ID: 'GW-SP-001',   // são Paulo demo gateway
    STATUS_ID_TEMP_CABINET: +cabTemp.toFixed(2),
    STATUS_ID_TEMP_EVAP:    +(drift(-22, 2, idx) + rng(-0.3, 0.3)).toFixed(2),
    STATUS_ID_TEMP_COND:    +(drift(45, 5, idx) + rng(-0.5, 0.5)).toFixed(2),
    STATUS_ID_TEMP_AMB:     +(drift(25, 2, idx) + rng(-0.2, 0.2)).toFixed(2),
    STATUS_ID_TEMP_DOOR:    +(drift(5, 2, idx) + rng(-0.3, 0.3)).toFixed(2),
    STATUS_ID_TEMP_AUX:     +(drift(10, 3, idx) + rng(-0.2, 0.2)).toFixed(2),
    STATUS_ID_TEMP_EVAP_2:  +(drift(-20, 2, idx) + rng(-0.4, 0.4)).toFixed(2),
    STATUS_ID_TEMP_DISPLAY: +(drift(-14, 1, idx) + rng(-0.1, 0.1)).toFixed(2),
    STATUS_ID_TEMP_PRODUCT: +(drift(-16, 2, idx) + rng(-0.3, 0.3)).toFixed(2),
    // compressor
    STATUS_ID_COMP_SPEED:     Math.round(drift(3000, 500, idx) + rng(-50, 50)),
    STATUS_ID_COMP_SET_SPEED: Math.round(drift(3200, 200, idx)),
    STATUS_ID_COMP_POWER:     Math.round(drift(150, 30, idx) + rng(-5, 5)),
    STATUS_ID_COMP_OP_STATUS: 1,
    STATUS_ID_EVAP_FAN_SPEED: Math.round(drift(1200, 200, idx) + rng(-20, 20)),
    STATUS_ID_COND_FAN_SPEED: Math.round(drift(1500, 300, idx) + rng(-30, 30)),
    STATUS_ID_CONTROLLER_STATE: cabTemp < -18 ? 'Cooling' : cabTemp > -12 ? 'Defrost' : 'Running',
    LATITUDE:  meta?.lat ?? 0,
    LONGITUDE: meta?.lng ?? 0,
    timestamp: Date.now(),
  });
}

// alert templates based on real conditions we've seen in the client field trials
const ALERT_MSGS = [
  { message: 'Cabinet temp above -12 °C threshold',        severity: 'warning'  },
  { message: 'Compressor current exceeds rated limit',     severity: 'warning'  },
  { message: 'Suction pressure dropped below safe range',  severity: 'critical' },
  { message: 'Supply voltage fluctuation detected',        severity: 'info'     },
  { message: 'Automatic defrost cycle started',            severity: 'info'     },
  { message: 'Refrigerant charge level below 20 %',       severity: 'critical' },
] as const;

export function generateAlertPayload(): string {
  const dev = DEMO_DEVICES[Math.floor(Math.random() * DEMO_DEVICES.length)];
  const a   = ALERT_MSGS[Math.floor(Math.random() * ALERT_MSGS.length)];
  return JSON.stringify({
    device_id: dev,
    rule_id: `rule-${Math.floor(rng(1, 10))}`,
    message: a.message,
    severity: a.severity,
  });
}

const CMD_LIST = [
  'reduce_speed',
  'increase_cooling',
  'start_defrost',
  'reset_alarm',
  'power_save_mode',
];

export function generateCommandPayload(): string {
  const dev = DEMO_DEVICES[Math.floor(Math.random() * DEMO_DEVICES.length)];
  const cmd = CMD_LIST[Math.floor(Math.random() * CMD_LIST.length)];
  return JSON.stringify({
    device_id: dev,
    rule_id: `rule-${Math.floor(rng(1, 10))}`,
    command: cmd,
  });
}

/** Fake aggregated query result for the query-engine demo tab. */
export function generateQueryResults(): string {
  const rows = DEMO_DEVICES.slice(0, 3).map((id) => ({
    DEVICE_ID: id,
    AVG_TEMPERATURE: +rng(-18, -12).toFixed(1),
    AVG_PRESSURE: +rng(2.5, 4.5).toFixed(2),
    AVG_SPEED: Math.round(rng(2500, 3500)),
  }));
  return JSON.stringify(rows);
}
