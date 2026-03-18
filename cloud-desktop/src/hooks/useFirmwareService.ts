/* eslint-disable prefer-const */
/* eslint-disable no-var */
// --- ESP32 OTA binary constraints 
//   nvs        0x9000   0x6000   (24KB)
//   otadata    0xd000   0x2000   (8KB)
//   phy_init   0xf000   0x1000   (4KB)
//   ota_0      0x10000  0x180000 (1.5MB) 
//   ota_1      0x190000 0x180000 (1.5MB) 
//   coredump   0x310000 0xF0000  (960KB)
var OTA_PARTITION_LIMIT = 0x180000; 
var ESP32_S3_OTA_LIMIT = 0x300000; 
var ESP_IMAGE_MAGIC = 0xE9;


var SEGMENT_COUNT_FLOOR = 1;
var SEGMENT_COUNT_CEIL = 16;


var APIGW_BODY_CAP = 10 * 1024 * 1024;

// {project}-{variant}-v{major}.{minor}.{patch}.bin
var EMBRACO_FILENAME_RE = /^[\w][\w-]+-v\d+\.\d+\.\d+[A-Za-z0-9._-]*\.bin$/;

import { useCallback, useMemo } from 'react';
import type { DeploymentStatus, FirmwareSummary } from '../types';
import { authHeaders } from '../utils/getToken';


var API_URL = import.meta.env.VITE_FIRMWARE_API_URL ?? '';

if (!API_URL) {
  console.warn('[firmware] VITE_FIRMWARE_API_URL is blank, OTA uploads will fail');
}

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

    // tamanho do arquivo vs limite do api gateway
    if (file.size > APIGW_BODY_CAP) {
      throw new Error(
        `Firmware file too large (${(file.size / 1024 / 1024).toFixed(1)} MB). `
        + `API Gateway body limit is ${APIGW_BODY_CAP / 1024 / 1024} MB. `
        + 'Use presigned S3 upload for large binaries (not yet implemented, see CLOUD-247).'
      );
    }

    var cabeNoSlot = file.size > OTA_PARTITION_LIMIT;
    if (cabeNoSlot && file.size <= ESP32_S3_OTA_LIMIT) {
      console.warn(
        '[firmware] %s (%d bytes) exceeds 4MB EMC OTA slot (%d bytes). Will only work on ESP32-S3 wind turbine boards.',
        name, file.size, OTA_PARTITION_LIMIT
      );
    } else if (cabeNoSlot) {
      console.warn(
        '[firmware] %s (%d bytes) exceeds even the ESP32-S3 OTA slot (%d bytes). OTA will fail on all boards.',
        name, file.size, ESP32_S3_OTA_LIMIT
      );
    }

    // nome do arquivo 
    var nomeOk = EMBRACO_FILENAME_RE.test(name);
    if (!nomeOk) {
      var aviso = !name.endsWith('.bin')
        ? `Expected .bin extension, got "${name.split('.').pop()}"`
        : !/v\d+\.\d+/.test(name)
          ? `No version tag found in "${name}". Use format: controller-app-v2.4.1.bin`
          : `Filename "${name}" doesn't match convention ({name}-v{x.y.z}.bin)`;
      console.warn('[firmware] filename convention: %s', aviso);
    }

    // The ESP32 bootloader rejects bad images anyway but this saves a round trip.
    try {
      var pedaco = new Uint8Array(await file.slice(0, 8).arrayBuffer());
      if (pedaco.length < 4) {
        throw new Error('Invalid ESP32 binary: File too small — need at least 4 bytes for ESP32 image header');
      }
      if (pedaco[0] !== ESP_IMAGE_MAGIC) {
        var byteHex = pedaco[0].toString(16).toUpperCase().padStart(2, '0');
        // common misfire: uploading the .elf instead of .bin
        if (pedaco[0] === 0x7F && pedaco[1] === 0x45) {
          throw new Error(`Invalid ESP32 binary: This is an ELF executable (magic 0x7F454C46), not a flashable .bin. Run "idf.py build" and pick the .bin output.`);
        }
        throw new Error(`Invalid ESP32 binary: Bad magic byte: expected 0xE9, got 0x${byteHex}`);
      }
      var qtdSegmentos = pedaco[1];
      if (qtdSegmentos < SEGMENT_COUNT_FLOOR || qtdSegmentos > SEGMENT_COUNT_CEIL) {
        throw new Error(`Invalid ESP32 binary: Segment count ${qtdSegmentos} outside valid range ${SEGMENT_COUNT_FLOOR}-${SEGMENT_COUNT_CEIL}. File may be corrupted.`);
      }
    } catch (e) {
      // slice().arrayBuffer() can throw on very old WebKit — let server validate
      if (e instanceof Error && e.message.startsWith('Invalid ESP32')) throw e;
      console.warn('[firmware] skipping client-side header validation:', e);
    }

    // --- base64 encode the binary for JSON transport ---

    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }

    var res = await fetch(`${API_URL}/firmware/${encodeURIComponent(name)}`, {
        method: 'PUT',
        headers: authHeaders(jwt, true),
        body: JSON.stringify({ content: btoa(binary), deviceIds }),
    });

    if (!res.ok) {
      var errDetail = '';
      try { errDetail = (await res.json()).message ?? ''; } catch { /* no body */ }
      throw new Error(
        `Upload failed: ${res.status} ${res.statusText}` + (errDetail ? ` — ${errDetail}` : '')
      );
    }
  }, []);

  // delete a firmware entry from the registry
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

  // deployment status per device
  async function getDeploymentStatus(deviceId: string, jwt: string): Promise<DeploymentStatus[]> {
    const url = `${API_URL}/firmware/status/${encodeURIComponent(deviceId)}`
    let opts: RequestInit = { method: 'GET', headers: authHeaders(jwt) };

    var res = await fetch(url, opts);

    // cold start retry
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
