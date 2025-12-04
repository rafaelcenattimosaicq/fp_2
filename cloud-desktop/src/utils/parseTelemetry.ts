import type { TelemetryPoint } from '../types';

// takes raw json string from mqtt and turns it into our TelemetryPoint format
export function parseTelemetry(json: string): TelemetryPoint | null {
  try {
    const data = JSON.parse(json) as Record<string, unknown>;

    // get device id from either field (backend sends DEVICE_ID, some devices send device_id)
    let deviceId: string;
    if (data.DEVICE_ID != undefined) {
      deviceId = String(data.DEVICE_ID);
    } else if (data.device_id != undefined) {
      deviceId = String(data.device_id)
    } else {
      deviceId = 'unknown';
    }

    let timestamp: number;
    if (typeof data.timestamp === 'number') {
      timestamp = data.timestamp as number;
    }
    else {
      timestamp = Date.now();
    }

    const skipKeys = new Set(['timestamp', 'LATITUDE', 'LONGITUDE', 'DEVICE_ID', 'GATEWAY_ID']);
    // const skipKeys = new Set(['timestamp', 'LATITUDE', 'LONGITUDE', 'DEVICE_ID', 'GATEWAY_ID', 'STATUS_ID_CONTROLLER_STATE']);
    const values: Record<string, number> = {};

    for (const [k, v] of Object.entries(data)) {
      if (typeof v !== 'number') continue;
      if (skipKeys.has(k)) {
        continue
      }
      values[k] = v as number;
    }


    let state: string | undefined;
    if (typeof data.STATUS_ID_CONTROLLER_STATE === 'string') {
      state = data.STATUS_ID_CONTROLLER_STATE as string;
    } else if (typeof data.STATE === 'string') {
      state = String(data.STATE);
    } else {
      state = undefined;
    }

    var lat: number | undefined = typeof data.LATITUDE === 'number' ? data.LATITUDE as number : undefined;
    var lng: number | undefined = typeof data.LONGITUDE === 'number' ? data.LONGITUDE as number : undefined;

    const result: TelemetryPoint = {
      deviceId: deviceId,
      timestamp: timestamp,
      values: values,
      state: state,
      latitude: lat,
      longitude: lng,
    };
    return result;

  } catch {
    return null;
  }
}
