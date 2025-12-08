import { createContext, useContext, useState, useCallback, useEffect } from 'react';
import type { ReactNode } from 'react';
import { useFirmwareService } from '../hooks/useFirmwareService';
import { getToken } from '../utils/getToken';
import type { FirmwareSummary } from '../types';

export interface FirmwareContextValue {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  firmware: FirmwareSummary[];
  error: string | null;
  loadFirmware: () => Promise<void>;
  upload: (name: string, file: File, deviceIds: string[]) => Promise<void>;
  remove: (name: string) => Promise<void>;
}

const FirmwareContext = createContext<FirmwareContextValue | null>(null);

// max firmware binary size we allow through the UI (50MB)
// the lambda has its own limit but this prevents wasting time on huge uploads
const MAX_UPLOAD_SIZE = 50 * 1024 * 1024;

export function FirmwareProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const svc = useFirmwareService();

  const [status, setStatus] = useState<FirmwareContextValue['status']>('idle');
  const [firmware, setFirmware] = useState<FirmwareSummary[]>([]);
  const [error,setError] = useState<string | null>(null);

  const loadFirmware = useCallback(async () => {
      setStatus('loading');
      setError(null);
    try {
      const token = await getToken();
      setFirmware(await svc.listFirmware(token));
      setStatus('loaded');
    } catch(e) {
        if (e instanceof Error) {
          setError(e.message)
        } else {
          setError('Failed to load firmware')
        }
        setStatus('error');
    }
  }, [svc]);

  // handles upload + refresh
  const upload = useCallback(async (name: string, file: File, deviceIds: string[]) => {
    if(status === 'loading') return;

    // quick sanity check before we even hit the API
    if (file.size > MAX_UPLOAD_SIZE) {
      setError(`File too large (${(file.size / 1024 / 1024).toFixed(1)}MB). Max is 50MB.`);
      return;
    }

    setError(null);
    setStatus('loading');

    try {
        const token = await getToken();
        await svc.uploadFirmware(name, file, deviceIds, token);
        const refreshed = await svc.listFirmware(token);
        setFirmware(refreshed);
        setStatus('loaded');
    } catch(e) {
      setError(e instanceof Error ? e.message : 'could not upload firmware');
      setStatus('error');
      throw e;
    }
  }, [svc, status]);

  const remove = useCallback(
    async (name: string) => {
      setError(null);

      try {
        const token = await getToken()
        await svc.deleteFirmware(name, token);
        setFirmware(await svc.listFirmware(token));
      } catch (e) {
        if(e instanceof Error){
          setError(e.message);
        } else{
          setError('delete firmware');
        }
      }
    },
    [svc],
  );

  return (
    <FirmwareContext.Provider value={{
      status: status,
      firmware,
      error,
      loadFirmware,
      upload: upload,
      remove,
    }}>
      {children}
    </FirmwareContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useFirmware(): FirmwareContextValue {
  const ctx = useContext(FirmwareContext);
  if (ctx === null) {
    throw new Error('useFirmware must be called inside <FirmwareProvider>');
  }
  return ctx;
}
