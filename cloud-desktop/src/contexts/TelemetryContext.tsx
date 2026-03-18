/* eslint-disable prefer-const */
/* eslint-disable no-var */
// TelemetryContext 

import {
  createContext, useContext, useEffect,
  useState, useCallback,
} from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { parseTelemetry } from '../utils/parseTelemetry';
import type { TelemetryPoint, Device } from '../types';


var JANELA_MS = 5 * 60 * 1000;


const LIMIAR_OFFLINE = 10_000;


var LIMIAR_STALE = 30_000;


var COMPRESSOR_REGISTERS = ['STATUS_ID_COMP_SPEED', 'STATUS_ID_COMP_POWER', 'STATUS_ID_TEMP_CABINET', 'STATUS_ID_TEMP_DISCHARGE', 'STATUS_ID_TEMP_SUCTION', 'STATUS_ID_TEMP_CONDENSER'];
var WIND_TURBINE_REGISTERS = ['STATUS_ID_RPM', 'STATUS_ID_POWER', 'STATUS_ID_WIND_SPEED', 'STATUS_ID_ROTOR_TEMP'];

// the gateway publishes all Modbus/wind-turbine readings on this single topic
var TOPICO_GW = 'controller_app/events';

let _deviceTypes: Record<string, 'compressor' | 'wind_turbine' | 'unknown'> = {};

// --- shared buffer ---

let _bufTelemetria = new Map<string, TelemetryPoint[]>();

// --- React context and provider ---

interface TelProviderProps { children: ReactNode }

const Ctx = createContext<{
  devices: Device[];
  buffers: Map<string, TelemetryPoint[]>;
  selectedDeviceId: string | null;
  selectDevice: (id: string | null) => void;
} | null>(null);

export function TelemetryProvider({ children }: TelProviderProps): React.JSX.Element {
  const { subscribe } = useMqtt();
  const [devices, setDevices] = useState<Device[]>([]);
  const [buffers, setBuffers] = useState<Map<string, TelemetryPoint[]>>(() => new Map());
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);

  useEffect(() => {
    // reset on hot-reload so stale Modbus readings don't bleed through
    _bufTelemetria = new Map<string, TelemetryPoint[]>();
    _deviceTypes = {};

    const unsub = subscribe(TOPICO_GW, (_topic, payload) => {
      const pt = parseTelemetry(payload);
      if (!pt) return;

      var devId = pt.deviceId;
      var agora = Date.now();
      const existing = _bufTelemetria.get(devId) ?? [];


      if (!_deviceTypes[devId]) {
        let valKeys = Object.keys(pt.values);
        let isCompressor = valKeys.some(k => COMPRESSOR_REGISTERS.indexOf(k) !== -1);
        let isTurbine = valKeys.some(k => WIND_TURBINE_REGISTERS.indexOf(k) !== -1);
        if (isCompressor) {
          _deviceTypes[devId] = 'compressor';
          console.debug('[tel] detected compressor device:', devId, '(has STATUS_ID_COMP_* registers)');
        } else if (isTurbine) {
          _deviceTypes[devId] = 'wind_turbine';
          console.debug('[tel] detected wind turbine device:', devId);
        } else {
          _deviceTypes[devId] = 'unknown';
        }
      }


      var tsUltimo = existing.length > 0 ? existing[existing.length - 1].timestamp : 0;
      if (tsUltimo > 0 && (agora - tsUltimo) > LIMIAR_STALE) {
        console.warn('[tel] device', devId, 'was offline for', Math.round((agora - tsUltimo) / 1000), 's — possible RS485 disconnect or gateway restart');
      }

      if (pt.timestamp === tsUltimo) return;


      if (pt.timestamp > agora + 60_000) {
        console.warn('[tel] rejecting future timestamp from', devId, '— ESP32 RTC drift?');
        return;
      }
      if (pt.timestamp < agora - JANELA_MS * 2) {
        //  old data point, ignore it
        return;
      }

      existing.push(pt);

      // trim to rolling window
      const corte = agora - JANELA_MS;
      var trimmed = existing.filter((p) => p.timestamp >= corte);
      _bufTelemetria.set(devId, trimmed);

      var lista: Device[] = [];
      for (const [id, pontos] of _bufTelemetria) {
        var ultimo = pontos.length > 0 ? pontos[pontos.length - 1] : null;
        var lastSeen = ultimo?.timestamp ?? 0;
        var isOnline = agora - lastSeen < LIMIAR_OFFLINE;

        var isStale = !isOnline && (agora - lastSeen < LIMIAR_STALE);
        lista.push({
          id: id,
          name: id,  // FIXME: resolve friendly name from NES device registry
          lastSeen: lastSeen,
          online: isOnline || isStale,
          lastPoint: ultimo,
          latitude: ultimo?.latitude,
          longitude: ultimo?.longitude,
        });
      }

      setBuffers(new Map(_bufTelemetria));
      setDevices(lista);
    });

    return () => {
      unsub();
      _bufTelemetria = new Map();
    };
  }, [subscribe]);

  const selectDevice = useCallback((id: string | null) => {
    setSelectedDeviceId(id);
  }, []);

  return (
    <Ctx.Provider value={{ devices, buffers, selectedDeviceId, selectDevice }}>
      {children}
    </Ctx.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export const useTelemetry = (): {
  devices: Device[];
  buffers: Map<string, TelemetryPoint[]>;
  selectedDeviceId: string | null;
  selectDevice: (id: string | null) => void;
} => {
  const ctx = useContext(Ctx);
  if (!ctx) {
    throw new Error('useTelemetry called outside TelemetryProvider — check App.tsx wrapper order');
  }
  return ctx;
};
