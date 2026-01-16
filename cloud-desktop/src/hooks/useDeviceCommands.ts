import { useEffect, useState,useCallback, useRef } from 'react';
import { useMqtt } from '../contexts/MqttContext';

// handles sending modbus write commands over mqtt and waiting for ack response
// TODO: maybe add retry logic if ack never comes back
export interface WriteAck {
  request_id: string;
  success: boolean;
  error?: string;
}

export interface WriteCommandState {
  pending: boolean;
  lastAck: WriteAck | null;
}

export function useDeviceCommands(gatewayId: string): {
  sendWriteCommand: (deviceId: string, registerId: string, value: number) => void;
  commandState: WriteCommandState;
} {
  const { publish, subscribe } = useMqtt();
  const [commandState, setCommandState] = useState<WriteCommandState>({ pending: false, lastAck: null });
  var timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  function clearTimer() {
    if(timerRef.current !== null){
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }

  useEffect(() => {
    return () => { clearTimer(); };
  }, []);

  // listen for ack messages
  useEffect(() => {
    var unsub = subscribe('commands/' + gatewayId + '/write/ack', (_topic, raw) => {
        try {
          var data = JSON.parse(raw) as WriteAck;
          // console.log("debug ack", data)
          clearTimer();
          setCommandState({pending: false, lastAck: data});
        } catch {
          // bad json, just ignore
        }
      }
    );
    return unsub;
  }, [gatewayId, subscribe]);

  // send a write command to the gateway
  const sendWriteCommand = useCallback((deviceId: string, registerId: string, value: number): void => {
    var reqId = Date.now() + '-' + Math.random().toString(36).slice(2,8);

    publish(`commands/${gatewayId}/write`, JSON.stringify({
      request_id: reqId,
      device_id: deviceId,
      register_id: registerId,
      value: value,
    }));
    setCommandState({pending: true,lastAck: null});

    clearTimer();

    timerRef.current = setTimeout(() => {
      setCommandState({ pending: false, lastAck: {
          request_id: reqId, success: false, error: '(timeout)',
      }});
    }, 8000);
  }, [gatewayId, publish]);

  return { sendWriteCommand, commandState };
}
