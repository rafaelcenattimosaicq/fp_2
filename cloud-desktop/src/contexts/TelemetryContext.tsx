import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
} from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { parseTelemetry } from '../utils/parseTelemetry';
import type { TelemetryPoint, Device } from '../types';

const BUFFER_MS = 5 * 60 * 1000;

// if no data in 10s, device is offline
const OFFLINE_MS = 10_000;

const GATEWAY_TOPIC = 'controller_app/events';

interface TelemetryProviderProps {
  children: ReactNode;
}

// shared buffer outside component bc useState initializer runs twice in strict mode
// and the cleanup from the first render would wipe the data
// react makes this annoying
let sharedBuffer = new Map<string, TelemetryPoint[]>();

const TelemetryContext = createContext<{
  devices: Device[];
  buffers: Map<string, TelemetryPoint[]>;
  selectedDeviceId: string | null;
  selectDevice: (id: string | null) => void;
} | null>(null);

export function TelemetryProvider({
  children,
}: TelemetryProviderProps): React.JSX.Element {
  const { subscribe } = useMqtt();
  const [devices, setDevices] = useState<Device[]>([]);
  const [buffers, setBuffers] = useState<Map<string, TelemetryPoint[]>>(
    () => new Map(),
  );
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);

  useEffect(() => {
    // reset on hot reload so old session doesnt bleed through
    sharedBuffer = new Map<string, TelemetryPoint[]>();

    const unsubscribe = subscribe(GATEWAY_TOPIC, (_topic, payload) => {
      const point = parseTelemetry(payload);
      if (!point) return;

      const { deviceId } = point;
      const now = Date.now();

      const existing = sharedBuffer.get(deviceId) ?? [];

      // skip duplicate timestamps (broker retry when ACK lost)
      const lastTs = existing.length > 0 ? existing[existing.length - 1].timestamp : 0;
      if (point.timestamp === lastTs) return;

      existing.push(point);

      // trim old points outside buffer window
      const cutoff = now - BUFFER_MS;
      const trimmed = existing.filter((p) => p.timestamp >= cutoff);
      sharedBuffer.set(deviceId, trimmed);

      setBuffers(new Map(sharedBuffer));

      // console.log("debug telemetry", deviceId, trimmed.length)
      setDevices(
        Array.from(sharedBuffer.entries()).map(([id, buf]) => {
          const lastPoint = buf[buf.length - 1] ?? null;
          return {
            id,
            name: id,
            lastSeen: lastPoint?.timestamp ?? 0,
            online: now - (lastPoint?.timestamp ?? 0) < OFFLINE_MS,
            lastPoint,
            latitude: lastPoint?.latitude,
            longitude: lastPoint?.longitude,
          };
        }),
      );
    });

    return () => {
      unsubscribe();
      sharedBuffer = new Map();
    };
  }, [subscribe]);

  const selectDevice = useCallback((id: string | null) => {
    setSelectedDeviceId(id);
  }, []);

  return (
    <TelemetryContext.Provider value={{
      devices,
      buffers,
      selectedDeviceId,
      selectDevice,
    }}>
      {children}
    </TelemetryContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export const useTelemetry = (): {
  devices: Device[];
  buffers: Map<string, TelemetryPoint[]>;
  selectedDeviceId: string | null;
  selectDevice: (id: string | null) => void;
} => {
  const ctx = useContext(TelemetryContext);
  if (!ctx) {
    throw new Error('useTelemetry must be used within a TelemetryProvider');
  }
  return ctx;
};
