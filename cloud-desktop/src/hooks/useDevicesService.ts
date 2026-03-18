/* eslint-disable prefer-const */
/* eslint-disable no-var */
// Each device row in DynamoDB stores its protocol (modbus-rtu, ble, etc.)
import { useMemo } from 'react';
import type { DeviceRecord } from '../types';
import { authHeaders } from '../utils/getToken';

// modbus address range we actually use 


const API_BASE = (import.meta.env.VITE_DEVICES_API_URL as string | undefined) ?? '';

export const isDevicesApiConfigured = API_BASE.length > 0;

if (!isDevicesApiConfigured) {
  console.warn('[devices] VITE_DEVICES_API_URL not set — device CRUD will fail');
}

export function useDevicesService() {
  return useMemo(() => buildServiceFns(), []);
}

function buildServiceFns() {
  function listDevices(authToken: string): Promise<DeviceRecord[]> {
    return fetch(`${API_BASE}/devices`, {
      method: 'GET',
      headers: authHeaders(authToken),
    }).then((res) => {
      if (!res.ok) throw new Error(`could not list devices: ${res.status}`);
      return res.json() as Promise<DeviceRecord[]>;
    });
  }


  async function getDevice(deviceId: string, authToken: string): Promise<DeviceRecord> {
    let res = await fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'GET',
      headers: authHeaders(authToken),
    });

    if (res.ok) return (await res.json()) as DeviceRecord;

    var detail = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status}${detail ? `: ${detail}` : ''}`);
  }

  const saveDevice = (
    deviceId: string,
    protocol: string,
    authToken: string,
    icon?: string,
  ): Promise<void> => {
    var body: Record<string, string> = { protocol };
    if (icon) body.icon = icon;

    return fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'PUT',
      headers: authHeaders(authToken, true),
      body: JSON.stringify(body),
    }).then((res) => {
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    });
  };

  // remove device + associated IoT policy binding
  async function deleteDevice(deviceId: string, authToken: string): Promise<void> {
    const res = await fetch(`${API_BASE}/devices/${deviceId}`, {
      method: 'DELETE',
      headers: authHeaders(authToken),
    });
    if (!res.ok) throw new Error(res.statusText);
  }

  return { listDevices, getDevice, saveDevice, deleteDevice } as const;
}
