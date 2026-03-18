/* eslint-disable no-var */
// FirmwareContext 

import { createContext, useContext, useState, useCallback } from 'react';
import type { ReactNode } from 'react';
import { useFirmwareService } from '../hooks/useFirmwareService';
import { getToken } from '../utils/getToken';
import type { FirmwareSummary } from '../types';

// ESP32 image header layout
//   offset 0: magic byte (0xE9)
//   offset 1: segment count
//   offset 2: SPI mode (0=QIO, 1=QOUT, 2=DIO, 3=DOUT)
//   offset 3: SPI speed + flash size (high nibble = speed, low = size)
//   offset 4-7: entry point address (little-endian u32)

var ESP32_MAGIC = 0xE9;
var MAX_SEGMENTS = 16;


var OTA_SLOT_SIZE = 0x180000; // 1572864

// API Gateway body limit i

var UPLOAD_CAP = 50 * 1024 * 1024;


async function validateEsp32Binary(file: File): Promise<string | null> {
  if (file.size < 8) return 'File is too small to be a valid ESP32 binary (< 8 bytes)';

  var buf: ArrayBuffer;
  try {

    buf = await file.slice(0, 8).arrayBuffer();
  } catch(e) {
    console.warn('[fw] could not read file header:', e);
    return null; // skip validation, server will catch it
  }

  var header = new Uint8Array(buf);
  var magic = header[0];
  if (magic !== ESP32_MAGIC) {
    return `Not an ESP32 binary: expected 0xE9 at offset 0, got 0x${magic.toString(16).toUpperCase().padStart(2, '0')}. `
      + 'Did you upload the .elf or .map by mistake? You need the .bin from the build output.';
  }

  var segCount = header[1];
  if (segCount === 0 || segCount > MAX_SEGMENTS) {
    return `Suspicious segment count (${segCount}) in ESP32 image header. Valid range is 1-${MAX_SEGMENTS}. Binary might be corrupted.`;
  }

  // check the entry point isn't 0x00000000 (common in truncated files)
  var entryPoint = header[4] | (header[5] << 8) | (header[6] << 16) | (header[7] << 24);
  if (entryPoint === 0) {
    return 'Entry point is 0x00000000 — binary appears truncated or corrupted. Re-run idf.py build and upload the fresh .bin.';
  }


  if (file.size > OTA_SLOT_SIZE) {
    console.warn('[fw] binary %s (%d bytes) exceeds standard OTA slot size (%d). Will work on ESP32-S3 but not on regular ESP32.', file.name, file.size, OTA_SLOT_SIZE);
  }

  return null; // all good
}

function parseVersionTag(name: string): string | null {
  var m = name.match(/v?(\d+\.\d+(?:\.\d+)?)/i);
  return m ? m[1] : null;
}

// --- context + provider ---

export interface FirmwareContextValue {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  firmware: FirmwareSummary[];
  error: string | null;
  loadFirmware: () => Promise<void>;
  upload: (name: string, file: File, deviceIds: string[]) => Promise<void>;
  remove: (name: string) => Promise<void>;
}

var Ctx = createContext<FirmwareContextValue | null>(null);

export function FirmwareProvider({ children }: { children: ReactNode }): React.JSX.Element {
  var svc = useFirmwareService();
  const [status, setStatus] = useState<FirmwareContextValue['status']>('idle');
  const [lista, setLista] = useState<FirmwareSummary[]>([]);
  const [error, setError] = useState<string | null>(null);

  const carregarFirmwares = useCallback(async () => {
    setStatus('loading'); setError(null);
    try {
      var tok = await getToken();
      setLista(await svc.listFirmware(tok));
      setStatus('loaded');
    } catch(e) {
      var msg = e instanceof Error ? e.message : 'failed to list firmware from S3';
      console.warn('[fw] carregarFirmwares:', msg);
      setError(msg);
      setStatus('error');
    }
  }, [svc]);

  var enviarBinario = useCallback(async (name: string, file: File, deviceIds: string[]) => {
    if(status === 'loading') return;

    if (file.size > UPLOAD_CAP) {
      setError(`Binary too large (${(file.size / 1024 / 1024).toFixed(1)}MB). Limit is 50MB.`);
      return;
    }

    var headerErr = await validateEsp32Binary(file);
    if (headerErr) { setError(headerErr); return; }

    if (!parseVersionTag(name)) {
      console.warn('[fw] no version in filename "%s" — use controller-app-v{x.y.z}.bin', name);
    }

    setError(null); setStatus('loading');
    try {
      var tok = await getToken();
      await svc.uploadFirmware(name, file, deviceIds, tok);
      setLista(await svc.listFirmware(tok));
      setStatus('loaded');
    } catch(e) {
      var errMsg = e instanceof Error ? e.message : 'upload failed';

      setError(errMsg);
      setStatus('error');
      throw e;
    }
  }, [svc, status]);

  const removerFirmware = useCallback(async (name: string) => {
    setError(null);
    try {
      var tok = await getToken();
      await svc.deleteFirmware(name, tok);
      setLista(await svc.listFirmware(tok));
    } catch (e) {
      if(e instanceof Error){
        console.warn('[fw] delete:', e.message);
        setError(e.message);
      } else {
        setError('delete firmware failed — S3 object may be orphaned');
      }
    }
  }, [svc]);

  return (
    <Ctx.Provider value={{
      status: status, firmware: lista, error,
      loadFirmware: carregarFirmwares,
      upload: enviarBinario,
      remove: removerFirmware,
    }}>
      {children}
    </Ctx.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useFirmware(): FirmwareContextValue {
  var v = useContext(Ctx);
  if (v === null) throw new Error('useFirmware must be inside <FirmwareProvider>');
  return v;
}
