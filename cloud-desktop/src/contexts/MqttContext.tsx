import { createContext, useContext, useEffect, useRef, useReducer, useCallback } from 'react';
import type { ReactNode } from 'react';
import mqtt from 'mqtt';
import type { MqttClient } from 'mqtt';
import type { ConnectionStatus } from '../types';

const BROKER_URL = import.meta.env.VITE_MQTT_WS_URL ?? 'ws://localhost:9001';
const DEMO_MODE = import.meta.env.VITE_DEMO_MODE === 'true';

type MessageHandler = (topic: string, payload: string) => void;

type StatusAction =
  | { kind: 'connected' }
  | { kind: 'reconnecting' }
  | { kind: 'closed' }
  | { kind: 'errored' };

function statusReducer(_prev: ConnectionStatus, action: StatusAction): ConnectionStatus {
  switch (action.kind) {
    case 'connected':
      return 'connected';
    case 'reconnecting':
      return 'connecting';
    case 'closed': return 'disconnected';
    case 'errored': return 'error';
  }
}

interface MqttContextValue {
  status: ConnectionStatus;
  subscribe: (topic: string, handler: MessageHandler) => () => void;
  publish: (topic: string, message: string) => void;
  injectMessage: (topic: string, payload: string) => void;
}

const MqttContext = createContext<MqttContextValue | null>(null);
interface MqttProviderProps {
  children: ReactNode;
  brokerUrl?: string;
}

export function MqttProvider({ children, brokerUrl }: MqttProviderProps): React.JSX.Element {
  const initialStatus: ConnectionStatus = DEMO_MODE ? 'connected' : 'connecting';

  const [status, dispatch] = useReducer(statusReducer, initialStatus);
  const clientRef = useRef<MqttClient | null>(null);
  const handlersRef = useRef<Map<string, Set<MessageHandler>>>(new Map());

  const routeMessage = useCallback((topic: string, msg: string) => {
    for (const [pattern, handlers] of handlersRef.current) {
      if (topicMatchesPattern(pattern, topic)) {
        handlers.forEach((fn) => fn(topic, msg));
      }
    }
  }, []);

  // connect to broker and wire up event handlers
  // also re-subscribe existing topics on reconnect
  useEffect(() => {
    if ((DEMO_MODE)) return;
    const client = mqtt.connect(brokerUrl ?? BROKER_URL, { reconnectPeriod: 5000 });
    clientRef.current = client;

    client.on('connect', () => {
      dispatch({ kind: 'connected' });
      // re-subscribe topics that were registered before connection was ready
      const topics = Array.from(handlersRef.current.keys());
      if (topics.length > 0) client.subscribe(topics);
    });
    client.on('reconnect', () => dispatch({ kind: 'reconnecting' }));
    client.on('close', () => {
      dispatch({ kind: 'closed' });
    });
    client.on('error', () => dispatch({ kind: 'errored' }));
    client.on('message', (t: string, buf: Buffer) => {
      routeMessage(t, buf.toString());
    });

    return () => {
      client.end();
      clientRef.current = null;
    };
  }, [brokerUrl, routeMessage]);

  const subscribe = useCallback((topic: string, cb: MessageHandler) => {
    if (!handlersRef.current.has(topic)) {
      handlersRef.current.set(topic, new Set<MessageHandler>());
      if (clientRef.current) clientRef.current.subscribe(topic);
    }
    handlersRef.current.get(topic)!.add(cb);

    return () => {
      const set = handlersRef.current.get(topic);
      if (set) set.delete(cb);
      if (set?.size == 0) {
        handlersRef.current.delete(topic);
        if (clientRef.current) {
          clientRef.current.unsubscribe(topic);
        }
      }
    };
  }, []);
  const publish = useCallback((topic: string, msg: string) => {
    if (!clientRef.current?.connected) {
      console.warn(`[mqtt] publish to "${topic}" dropped -- client not connected`);
      return;
    }
    clientRef.current!.publish(topic, msg);
  }, []);

  const injectMessage = useCallback(
    (topic: string, msg: string) => { routeMessage(topic, msg); },
    [routeMessage],
  );
  return (
    <MqttContext.Provider value={{ status, subscribe, publish, injectMessage }}>
      {children}
    </MqttContext.Provider>
  );
}

export function useMqtt(): MqttContextValue {
  const ctx = useContext(MqttContext);
  if (!ctx) throw new Error('useMqtt must be used within a MqttProvider');
  return ctx!;
}

// basic MQTT topic matching - supports + (single level) and # (multi level) wildcards
function topicMatchesPattern(pattern: string, topic: string): boolean {
  if (pattern === topic) return true;

  if (pattern.endsWith('#')) {
    const prefix = pattern.slice(0, pattern.length - 1);
    return topic.startsWith(prefix);
  }
  const patternParts = pattern.split('/');
  const topicParts = topic.split('/');
  if (patternParts.length !== topicParts.length) return false;
  return patternParts.every(
    (seg, i) => seg === '+' || seg === topicParts[i],
  );
}
