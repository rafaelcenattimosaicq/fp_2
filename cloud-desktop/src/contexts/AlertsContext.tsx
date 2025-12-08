import { createContext, useContext, useEffect, useReducer, useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { useQueryService } from '../hooks/useQueryService';
import { nesOperator, nesValue } from '../utils/nesHelpers';
import type { Alert, AlertRule, Command, FeedItem } from '../types';

const MAX_FEED_SIZE = 100;
const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';
const BROKER_URL = import.meta.env.VITE_NES_MQTT_SINK_URL ?? '';

// dispatch avoids the subtle ordering bugs we hit when two separate
type AlertsState = {
  feed: FeedItem[];
  rules: AlertRule[];
};

type AlertsAction =
  | { type: 'PUSH_FEED'; item: FeedItem }
  | { type: 'CLEAR_FEED' }
  | { type: 'ADD_RULE'; rule: AlertRule }
  | { type: 'REMOVE_RULE'; id: string }
  | { type: 'PUSH_FEED_AND_ADD_RULE'; item: FeedItem; rule: AlertRule };

function alertsReducer(prev: AlertsState, action: AlertsAction): AlertsState {
  switch (action.type) {
    case 'PUSH_FEED':
      return { ...prev, feed: [action.item, ...prev.feed].slice(0, MAX_FEED_SIZE) };
    case 'CLEAR_FEED':
      return { ...prev, feed: [] };
    case 'ADD_RULE':
      return { ...prev, rules: [...prev.rules, action.rule] };
    case 'REMOVE_RULE':
      return { ...prev, rules: prev.rules.filter((r) => r.id !== action.id) };
    case 'PUSH_FEED_AND_ADD_RULE':
      return {
        feed: [action.item, ...prev.feed].slice(0, MAX_FEED_SIZE),
        rules: [...prev.rules, action.rule],
      };
    default:
      return prev;
  }
}

let feedCounter = 0;
function nextFeedId(): string {
  feedCounter += 1;
  return `feed-${feedCounter}`;
}

const AlertsContext = createContext<{
  feed: FeedItem[];
  clearFeed: () => void;
  rules: AlertRule[];
  addRule: (source: string, field: string, operator: string, threshold: string) => Promise<void>;
  removeRule: (id: string) => Promise<void>;
} | null>(null);

interface AlertsProviderProps {
  children: ReactNode;
}

/**
 * Manages the live alert feed and alert rules. Rules are deployed as
 * NES filter queries whose output is routed to an MQTT sink, so alerts
 * arrive in real time through the same broker the telemetry uses.
 */
export function AlertsProvider({ children }: AlertsProviderProps): React.JSX.Element {
  const { subscribe } = useMqtt();
  const { stopQuery: apiStop } = useQueryService();
  const [state, dispatch] = useReducer(alertsReducer, { feed: [], rules: [] });

  useEffect(() => {
    const unsubAlerts = subscribe('alerts/#', (_topic, payload) => {
      try {
        const raw = JSON.parse(payload) as Record<string, unknown>;
        const alert: Alert = {
          id: nextFeedId(),
          deviceId: String(raw.device_id ?? 'unknown'),
          ruleId: String(raw.rule_id ?? ''),
          message: String(raw.message ?? ''),
          severity: (raw.severity as Alert['severity']) ?? 'info',
          timestamp: Date.now(),
        };
        dispatch({ type: 'PUSH_FEED', item: { type: 'alert', data: alert } });
      } catch {
      }
    });

    const unsubCommands = subscribe('commands/#', (_topic, payload) => {
      try {
        const raw = JSON.parse(payload) as Record<string, unknown>;
        const command: Command = {
          id: nextFeedId(),
          deviceId: String(raw.device_id ?? 'unknown'),
          ruleId: String(raw.rule_id ?? ''),
          command: String(raw.command ?? ''),
          timestamp: Date.now(),
        };
        dispatch({ type: 'PUSH_FEED', item: { type: 'command', data: command } });
      } catch {
      }
    });

    return () => {
      unsubAlerts();
      unsubCommands();
    };
  }, [subscribe]);

  useEffect(() => {
    const unsub = subscribe('nebulastream/alerts/#', (topic, payload) => {
      const segments = topic.split('/');
      const ruleId = segments[segments.length - 1];

      const matchingRule = state.rules.find((r) => r.id === ruleId);
      if (!matchingRule) return;

      try {
        const raw = JSON.parse(payload) as Record<string, unknown> | Record<string, unknown>[];
        const rows = Array.isArray(raw) ? raw : [raw];

        for (const row of rows) {
          const cleanRow: Record<string, unknown> = {};
          for (const [key, value] of Object.entries(row)) {
            const cleaned = key.includes('$') ? key.substring(key.indexOf('$') + 1) : key;
            cleanRow[cleaned] = value;
          }

          const fieldValue = cleanRow[matchingRule.field] ?? '?';
          const deviceId = String(cleanRow.DEVICE_ID ?? 'unknown');

          const alert: Alert = {
            id: nextFeedId(),
            deviceId,
            ruleId,
            message: `${matchingRule.field} ${matchingRule.operator} ${matchingRule.threshold} (value: ${fieldValue})`,
            severity: 'warning',
            timestamp: Date.now(),
          };
          dispatch({ type: 'PUSH_FEED', item: { type: 'alert', data: alert } });
        }
      } catch {
      }
    });

    return unsub;
  }, [subscribe, state.rules]);

  /**
   * Deploy an alert rule as a NES filter query. The coordinator returns 502
   * during ECS rolling deployments; a single retry with a 2s backoff is
   * usually enough because the replacement task comes up fast.
   */
  const addRule = useCallback(async (
    source: string,
    field: string,
    operator: string,
    threshold: string,
  ) => {
    const ruleId = crypto.randomUUID();
    const op = nesOperator(operator);
    const val = nesValue(threshold);

    let dsl = `Query::from("${source}").filter(Attribute("${field}") ${op} ${val})`;

    if (BROKER_URL) {
      const sinkTopic = `nebulastream/alerts/${ruleId}`;
      dsl += `.sink(MQTTSinkDescriptor::create("${BROKER_URL}", "${sinkTopic}", "", 1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));`;
    } else {
      dsl += '.sink(PrintSinkDescriptor::create());';
    }

    let res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userQuery: dsl, placement: 'BottomUp' }),
    });

    // retry once on 502, ECS deploy in progress
    if (res.status === 502) {
      await new Promise((resolve) => setTimeout(resolve, 2000));
      res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userQuery: dsl, placement: 'BottomUp' }),
      });
    }

    let coordinatorQueryId: number | null = null;
    if (res.ok) {
      const body = await res.json() as { queryId: number };
      coordinatorQueryId = body.queryId;
    }

    const rule: AlertRule = {
      id: ruleId,
      coordinatorQueryId,
      source,
      field,
      operator,
      threshold,
      active: res.ok,
    };

    if (res.ok) {
      dispatch({ type: 'ADD_RULE', rule });
    } else {
      const errorAlert: Alert = {
        id: nextFeedId(),
        deviceId: source,
        ruleId,
        message: `Failed to create alert rule: ${res.status}`,
        severity: 'critical',
        timestamp: Date.now(),
      };
      dispatch({
        type: 'PUSH_FEED_AND_ADD_RULE',
        item: { type: 'alert', data: errorAlert },
        rule,
      });
    }
  }, []);

  const removeRule = useCallback(async (id: string) => {
    const target = state.rules.find((r) => r.id === id);
    if (target?.coordinatorQueryId != null && target.active) {
      try {
        await apiStop(String(target.coordinatorQueryId));
      } catch {
      }
    }
    dispatch({ type: 'REMOVE_RULE', id });
  }, [state.rules, apiStop]);

  const clearFeed = useCallback(() => dispatch({ type: 'CLEAR_FEED' }), []);

  const exposed = useMemo(
    () => ({ feed: state.feed, clearFeed, rules: state.rules, addRule, removeRule }),
    [state.feed, state.rules, clearFeed, addRule, removeRule],
  );

  return <AlertsContext.Provider value={exposed}>{children}</AlertsContext.Provider>;
}

export function useAlerts() {
  const alertsCtx = useContext(AlertsContext);
  if (!alertsCtx) {
    throw new Error('useAlerts requires an <AlertsProvider> ancestor');
  }
  return alertsCtx;
}
