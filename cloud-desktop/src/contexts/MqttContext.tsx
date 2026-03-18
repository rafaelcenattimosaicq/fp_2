/* eslint-disable no-var */
// MqttContext

import { createContext, useContext, useEffect, useRef, useState, useCallback } from 'react';
import type { ReactNode } from 'react';
import mqtt from 'mqtt';
import type { MqttClient } from 'mqtt';
import type { ConnectionStatus } from '../types';

// mosquitto broker
var BROKER_WS = import.meta.env.VITE_MQTT_WS_URL ?? 'ws://localhost:9001';
const MODO_DEMO = import.meta.env.VITE_DEMO_MODE === 'true';

// broker protection
var MSG_SIZE_LIMIT = 1024 * 1024;


var _msgCount = 0;
var _msgCountWindowStart = Date.now();
var _msgsPerSecond = 0;


let _reconnectAttempt = 0;
var BASE_RECONNECT_MS = 2000;
var MAX_RECONNECT_MS = 30_000;

function _calcReconnectDelay(): number {
  // exponential backoff: 2
  var delay = Math.min(BASE_RECONNECT_MS * Math.pow(2, _reconnectAttempt), MAX_RECONNECT_MS);
  var jitter = delay * 0.25 * (Math.random() * 2 - 1);
  _reconnectAttempt++;
  return Math.floor(delay + jitter);
}

// --- types and React context ---

type MsgHandler = (topic: string, payload: string) => void;

interface MqttCtxValue {
  status: ConnectionStatus;
  subscribe: (topic: string, handler: MsgHandler) => () => void;
  publish: (topic: string, message: string) => void;
  injectMessage: (topic: string, payload: string) => void;
}

const Ctx = createContext<MqttCtxValue | null>(null);

interface MqttProviderProps { children: ReactNode; brokerUrl?: string }

export function MqttProvider({ children, brokerUrl }: MqttProviderProps): React.JSX.Element {
  const [status, setStatus] = useState<ConnectionStatus>(
    MODO_DEMO ? 'connected' : 'connecting',
  );
  const clientRef = useRef<MqttClient | null>(null);
  const _assinaturas = useRef<Map<string, Set<MsgHandler>>>(new Map());

  const fanout = useCallback((topic: string, mensagem: string) => {
    _msgCount++;
    var agora = Date.now();
    var elapsed = agora - _msgCountWindowStart;
    if (elapsed > 5000) {
      _msgsPerSecond = _msgCount / (elapsed / 1000);
      if (_msgsPerSecond > 100) {
        console.warn('[mqtt] high throughput:', _msgsPerSecond.toFixed(1), 'msg/s — check for wildcard subscriptions');
      }
      _msgCount = 0;
      _msgCountWindowStart = agora;
    }

    for (const [pat, handlers] of _assinaturas.current) {
      let matched = false;
      if (pat === topic) { matched = true; }
      else if (pat.endsWith('#')) {
        var prefixo = pat.slice(0, -1);
        if (topic.startsWith(prefixo)) matched = true;
      } else {
        var patParts = pat.split('/');
        var topicParts = topic.split('/');
        if (patParts.length === topicParts.length) {
          matched = true;
          for (let idx = 0; idx < patParts.length; idx++) {
            var seg = patParts[idx];
            if (seg === '+') continue;
            if (seg !== topicParts[idx]) { matched = false; break; }
          }
        }
      }

      if (!matched) continue;
      handlers.forEach((fn) => fn(topic, mensagem));
    }
  }, []);

  useEffect(() => {
    if (MODO_DEMO) return;

    var wsUrl = brokerUrl ?? BROKER_WS;

    _reconnectAttempt = 0;

    var reconnectMs = _calcReconnectDelay();
    const cl = mqtt.connect(wsUrl, {
      reconnectPeriod: reconnectMs,
    });
    clientRef.current = cl;

    cl.on('connect', () => {
      setStatus('connected');
      _reconnectAttempt = 0; // reset backoff on successful connect
      const topicos = Array.from(_assinaturas.current.keys());
      if (topicos.length > 0) {
        cl.subscribe(topicos);
        console.debug('[mqtt] re-subscribed', topicos.length, 'topics after reconnect');
      }
    });

    cl.on('reconnect', () => {
      var nextDelay = _calcReconnectDelay();
      if ((cl as unknown as Record<string, unknown>).options) {
        ((cl as unknown as Record<string, Record<string, unknown>>).options).reconnectPeriod = nextDelay;
      }
      setStatus('connecting');
    });

    cl.on('close', () => {

      setStatus('disconnected');
    });


    cl.on('message', (t: string, buf: Buffer) => {
      if (buf.length > MSG_SIZE_LIMIT) {
        console.error('[mqtt] dropping oversized message on', t, '— size:', buf.length, 'bytes (limit:', MSG_SIZE_LIMIT, ')');
        return;
      }
      fanout(t, buf.toString());
    });

    return () => {
      cl.end();
      clientRef.current = null;
    };
  }, [brokerUrl, fanout]);

  const subscribe = useCallback((topic: string, cb: MsgHandler) => {
    if (!_assinaturas.current.has(topic)) {
      _assinaturas.current.set(topic, new Set<MsgHandler>());
      // subscribe on the broker if we have a live connection
      if (clientRef.current) clientRef.current.subscribe(topic);
    }
    _assinaturas.current.get(topic)!.add(cb);

    return () => {
      const conjunto = _assinaturas.current.get(topic);
      if (conjunto) conjunto.delete(cb);
      if (conjunto?.size == 0) {
        _assinaturas.current.delete(topic);
        if (clientRef.current) clientRef.current.unsubscribe(topic);
      }
    };
  }, []);

  const publish = useCallback((topic: string, msg: string) => {
    if (!clientRef.current?.connected) {
      console.warn(
        `[mqtt] publish to "${topic}" dropped — broker not reachable`
        + ` (check mosquitto WS listener or nginx proxy)`,
      );
      return;
    }
    
    if (msg.length > MSG_SIZE_LIMIT) {
      console.error('[mqtt] refusing to publish oversized message to', topic, '— size:', msg.length);
      return;
    }

    clientRef.current!.publish(topic, msg);
  }, []);


  const injectMessage = useCallback(
    (topic: string, msg: string) => { fanout(topic, msg); },
    [fanout],
  );

  return (
    <Ctx.Provider value={{ status, subscribe, publish, injectMessage }}>
      {children}
    </Ctx.Provider>
  );
}

export function useMqtt(): MqttCtxValue {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error('useMqtt called outside MqttProvider — wrap your app tree');
  return ctx!;
}
