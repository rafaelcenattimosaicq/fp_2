import { useCallback, useMemo } from 'react';
import type { DeploymentStatus, FirmwareSummary } from '../types';
import { authHeaders } from '../utils/getToken';

// firmware api - lambda behind api gateway
const API_URL = import.meta.env.VITE_FIRMWARE_API_URL ?? '';
// TODO: handle case where API_URL is empty string better
const MAX_SIZE = 10 * 1024 * 1024; // 10MB

export function useFirmwareService() {

  const listFirmware = useCallback((jwt: string): Promise<FirmwareSummary[]> => {
    return fetch(`${API_URL}/firmware`, {
      method: 'GET',
      headers: authHeaders(jwt),
    }).then((res) => {
        if (!res.ok) throw new Error(`${res.status}`);
        return res.json() as Promise<FirmwareSummary[]>;
    });
  }, []);

  const uploadFirmware = useCallback(
    async (name: string, file: File, deviceIds: string[], jwt: string): Promise<void> => {
    if (file.size > MAX_SIZE) {
      throw new Error(`Firmware file too large (${(file.size / 1024 / 1024).toFixed(1)} MB). Maximum is 10 MB.`);
    }

    // TODO: there's probably a better way to do base64 than manual char conversion
    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = '';
    for (const b of bytes) binary += String.fromCharCode(b);

    var res = await fetch(`${API_URL}/firmware/${encodeURIComponent(name)}`, {
        method: 'PUT',
        headers: authHeaders(jwt, true),
        body: JSON.stringify({ content: btoa(binary), deviceIds }),
    });

    if (!res.ok) throw new Error(res.statusText);
  }, []);

  // delete a firmware entry
  function deleteFirmware(name: string, jwt: string): Promise<void> {
      return fetch(`${API_URL}/firmware/${encodeURIComponent(name)}`, {
        method: 'DELETE',
        headers: authHeaders(jwt),
      }).then((x) => {
        if (!x.ok) {
            throw new Error(`Failed to delete firmware "${name}": ${x.status}`);
        }
    });
  }

  // get deployment status, retries on 504 (lambda cold start)
  async function getDeploymentStatus(deviceId: string, jwt: string): Promise<DeploymentStatus[]> {
    const url = `${API_URL}/firmware/status/${encodeURIComponent(deviceId)}`
    const opts: RequestInit = { method: 'GET', headers: authHeaders(jwt) };

    let res = await fetch(url, opts);

    if (res.status == 504) {
      await new Promise((r) => setTimeout(r, 1500));
      res = await fetch(url, opts);
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);

    return (await res.json()) as DeploymentStatus[];
  }

  return useMemo(
    () => ({ listFirmware, uploadFirmware, deleteFirmware, getDeploymentStatus }),
    [listFirmware, uploadFirmware],
  );
}
