/* eslint-disable no-empty */
/* eslint-disable react-hooks/preserve-manual-memoization */
import { createContext, useContext, useEffect, useReducer, useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { useQueryService } from '../hooks/useQueryService';
import { nesOperator, nesValue } from '../utils/nesHelpers';
import type { Alert, AlertRule, Command, FeedItem } from '../types';

const MAX_FEED = 100;
const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';
const BROKER_URL = import.meta.env.VITE_NES_MQTT_SINK_URL ?? '';

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
      return { ...prev, feed: [action.item, ...prev.feed].slice(0, MAX_FEED) };
    case 'CLEAR_FEED':
      return { ...prev, feed: [] };
    case 'ADD_RULE':
      return { ...prev, rules: [...prev.rules, action.rule] };
    case 'REMOVE_RULE':
      return { ...prev, rules: prev.rules.filter((r) => r.id !== action.id) };
    case 'PUSH_FEED_AND_ADD_RULE':
      return {
        feed: [action.item, ...prev.feed].slice(0, MAX_FEED),
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

interface ActionOpts {
  actionRegisterId?: string;
  actionValue?: number;
  actionGatewayId?: string;
}

const AlertsContext = createContext<{
  feed: FeedItem[];
  clearFeed: () => void;
  rules: AlertRule[];
  addRule: (source: string, field: string, operator: string, threshold: string, action?: ActionOpts) => Promise<void>;
  removeRule: (id: string) => Promise<void>;
} | null>(null);

interface AlertsProviderProps {
  children: ReactNode;
}

// manages live alert feed + alert rules
// rules are deployed as NES filter queries that output to MQTT
export function AlertsProvider({ children }: AlertsProviderProps): React.JSX.Element {
  const { subscribe, publish } = useMqtt();
  const { stopQuery: apiStop } = useQueryService();
  const [state, dispatch] = useReducer(alertsReducer, { feed: [], rules: [] });

  // subscribe to gateway alert/command topics
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

    return () => {
      unsubAlerts();
    };
  }, [subscribe]);

  // listen for NES alert query results coming through mqtt
  useEffect(() => {
    const unsub = subscribe('nebulastream/alerts/#', (topic, payload) => {
      const parts = topic.split('/');
      const ruleId = parts[parts.length - 1];

      const matchingRule = state.rules.find((r) => r.id === ruleId);
      if (!matchingRule) return;

      try {
        const raw = JSON.parse(payload) as Record<string, unknown> | Record<string, unknown>[];
        const rows = Array.isArray(raw) ? raw : [raw];

        for (const row of rows) {
          // strip NES field prefixes (source$field -> field)
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

          // auto-action: send write command to gateway if rule has an action configured
          if (matchingRule.actionRegisterId && matchingRule.actionValue !== undefined && matchingRule.actionGatewayId) {
            const writeCmd = JSON.stringify({
              request_id: `auto-${Date.now()}`,
              device_id: deviceId,
              register_id: matchingRule.actionRegisterId,
              value: matchingRule.actionValue,
            });
            publish(`commands/${matchingRule.actionGatewayId}/write`, writeCmd);

            const command: Command = {
              id: nextFeedId(),
              deviceId,
              ruleId,
              command: `Auto: ${matchingRule.actionRegisterId} = ${matchingRule.actionValue}`,
              timestamp: Date.now(),
            };
            dispatch({ type: 'PUSH_FEED', item: { type: 'command', data: command } });
          }
        }
      } catch {
      }
    });

    return unsub;
  }, [subscribe, publish, state.rules]);

  // deploy alert rule as NES filter query
  // retries once on 502 bc ECS rolling deploy
  const addRule = useCallback(async (
    source: string,
    field: string,
    operator: string,
    threshold: string,
    action?: ActionOpts,
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

    // console.log("debug alert dsl", dsl)
    let res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userQuery: dsl, placement: 'BottomUp' }),
    });

    // retry once on 502
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
      actionRegisterId: action?.actionRegisterId,
      actionValue: action?.actionValue,
      actionGatewayId: action?.actionGatewayId,
    };

    if (res.ok) {
      dispatch({ type: 'ADD_RULE', rule });

      // send auto-action rule config to gateway for local execution
      if (action?.actionRegisterId && action.actionValue !== undefined && action.actionGatewayId) {
        const ruleConfig = JSON.stringify({
          action: 'add',
          rule: {
            rule_id: ruleId,
            register_id: action.actionRegisterId,
            value: action.actionValue,
          },
        });
        publish(`commands/${action.actionGatewayId}/rule`, ruleConfig);
      }
    } else {
      const errAlert: Alert = {
        id: nextFeedId(),
        deviceId: source,
        ruleId,
        message: `Failed to create alert rule: ${res.status}`,
        severity: 'critical',
        timestamp: Date.now(),
      };
      dispatch({
        type: 'PUSH_FEED_AND_ADD_RULE',
        item: { type: 'alert', data: errAlert },
        rule,
      });
    }
  }, []);

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
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
  const ctx = useContext(AlertsContext);
  if (!ctx) {
    throw new Error('useAlerts requires an <AlertsProvider> ancestor');
  }
  return ctx;
}
