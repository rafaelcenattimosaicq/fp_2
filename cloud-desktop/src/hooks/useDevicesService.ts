import { useMemo } from 'react';
import type { DeviceRecord } from '../types';
import { authHeaders } from '../utils/getToken';

const API_BASE = (import.meta.env.VITE_DEVICES_API_URL as string | undefined) ?? '';

export const isDevicesApiConfigured = API_BASE.length > 0;

/**
 * Low-level device CRUD. Returns plain functions rather than loading/error
 * state because every caller manages its own lifecycle differently (the
 * device list polls, the detail page fetches once, the dialog fires and
 * awaits). Callers pass the Cognito token explicitly so we don't couple
 * this to the auth context.
 *
 * Functions are stable across renders via useMemo so that polling callers
 * (DeviceList) don't thrash their useEffect deps.
 */
export function useDevicesService() {
  return useMemo(() => buildServiceFns(), []);
}

/** Construct the service object once - no hooks needed inside here. */
function buildServiceFns() {
  /**
   * Fetch every registered device. Lambda returns [] for no devices,
   * not 404, so we don't treat empty arrays as errors.
   */
  function listDevices(authToken: string): Promise<DeviceRecord[]> {
    return fetch(`${API_BASE}/devices`, {
      method: 'GET',
      headers: authHeaders(authToken),
    }).then((res) => {
      if (!res.ok) throw new Error(`could not list devices: ${res.status}`);
      return res.json() as Promise<DeviceRecord[]>;
    });
  }

  /**
   * Fetch a single device by ID. We used to go through the list endpoint
   * plus a client-side filter, but that broke once we passed ~300 devices
   * (Lambda 6 MB response limit). The per-device endpoint also returns
   * the descriptor inline which saves a second round-trip.
   */
  async function getDevice(deviceId: string, authToken: string): Promise<DeviceRecord> {
    const res = await fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'GET',
      headers: authHeaders(authToken),
    });

    if (res.ok) return (await res.json()) as DeviceRecord;

    // includes the DynamoDB condition-check failure reason.
    const detail = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status}${detail ? `: ${detail}` : ''}`);
  }

  /**
   * Create or update a device record. Icon is optional because older
   * devices registered before we added the icon field.
   */
  const saveDevice = (
    deviceId: string,
    protocol: string,
    authToken: string,
    icon?: string,
  ): Promise<void> => {
    const body: Record<string, string> = { protocol };
    if (icon) body.icon = icon;

    return fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'PUT',
      headers: authHeaders(authToken, true),
      body: JSON.stringify(body),
    }).then((res) => {
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    });
  };

  /** Remove a device and its associated policy binding. */
  async function deleteDevice(deviceId: string, authToken: string): Promise<void> {
    const res = await fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'DELETE',
      headers: authHeaders(authToken),
    });
    if (!res.ok) throw new Error(res.statusText);
  }

  return { listDevices, getDevice, saveDevice, deleteDevice } as const;
}
