import { createContext, useContext, useState, useCallback, useReducer, useMemo, useEffect } from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { useQueryService, validateQuery } from '../hooks/useQueryService';
import type { LogicalSource } from '../hooks/useQueryService';
import type { Query, QueryRequest } from '../types';
import { DEMO_SOURCES, generateQueryResults } from '../demo/demoData';

const DEMO_MODE = import.meta.env.VITE_DEMO_MODE === 'true';

const MAX_STREAMING_ROWS = 500;
const SOURCE_POLL_MS = 15_000;
const PENDING_TIMEOUT_MS = 30_000;

// fields that come from the NES coordinator and aren't actual sensor data
const METADATA_FIELDS = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'join_key', 'start', 'end']);

function filterDeviceSources(sources: LogicalSource[]): LogicalSource[] {
  return sources.filter((s) => {
    if(s.name === 'default_logical') return false;
    return s.fields.some((f) => !METADATA_FIELDS.has(f));
  });
}

function stripFieldPrefix(obj: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const [key, val] of Object.entries(obj)) {
    const cleaned = key.includes('$') ? key.substring(key.indexOf('$') + 1) : key;
    if ((cleaned in result) && METADATA_FIELDS.has(cleaned)) continue;
    result[cleaned] = val;
  }
  return result;
}

type QueriesAction =
  | { type: 'ADD'; query: Query }
  | { type: 'REMOVE'; id: string }
  | { type: 'APPEND_RESULTS'; queryId: string; rows: Record<string, unknown>[] }
  | { type: 'MARK_COMPLETED'; id: string; results: Record<string, unknown>[] }
  | { type: 'MARK_FAILED'; id: string; error: string };

function queriesReducer(state: Query[], action: QueriesAction): Query[] {
  switch (action.type) {
    case 'ADD':
      return [action.query, ...state];

    case 'REMOVE':
      return state.filter((q) => q.id !== action.id);

    case 'APPEND_RESULTS': {
      return state.map((q) => {
        if (q.id !== action.queryId) return q;
        const combined = [...q.results, ...action.rows];
        // cap streaming buffer so we don't blow up memory on long-running queries
        return { ...q, status: 'running' as const, results: combined.slice(-MAX_STREAMING_ROWS) };
      });
    }

    case 'MARK_COMPLETED':
      return state.map((q) =>
        q.id == action.id ? { ...q, status: 'completed' as const, results: action.results } : q
      );

    case 'MARK_FAILED': {
      return state.map((q) => {
        if(q.id === action.id && q.status === 'pending'){
          return { ...q, status: 'failed' as const, error: action.error };
        }
        return q;
      });
    }

    default:
      return state;
  }
}

interface QueryContextValue {
  queries: Query[];
  sources: LogicalSource[];
  submitQuery: (request: QueryRequest) => Promise<void>;
  removeQuery: (id: string) => Promise<void>;
  loadingSources: boolean;
  selectedDevices: string[];
  setSelectedDevices: (devices: string[]) => void;
}

const QueryContext = createContext<QueryContextValue | null>(null);

interface QueryProviderProps {
  children: ReactNode;
}

export function QueryProvider({ children }: QueryProviderProps): React.JSX.Element {
  const { subscribe } = useMqtt();
  const { fetchSources: buscarFontes, submitQuery: apiSubmit, stopQuery: apiStop } = useQueryService();

  const [queries, dispatchQueries] = useReducer(queriesReducer, []);
  const [sources, setSources] = useState<LogicalSource[]>([]);
  const [loadingSources, setLoadingSources] = useState(true);
  const [selectedDevices, mySetDevices] = useState<string[]>([]);

  const setSelectedDevices = useCallback((arr: string[]) => {
    mySetDevices(arr);
  }, []);

  // fetch available logical sources from NES coordinator + set up polling
  useEffect(() => {
    let interval: ReturnType<typeof setInterval> | undefined;

    const doTheFetch = async () => {
      try {
        const res = await buscarFontes();
        setSources(filterDeviceSources(res));
      } catch {
        if(DEMO_MODE) setSources(DEMO_SOURCES);
      } finally {
        setLoadingSources(false);
      }
    };

    void doTheFetch();

    if (!DEMO_MODE) {
      interval = setInterval(() => void doTheFetch(), SOURCE_POLL_MS);
    }

    return () => { if(interval) clearInterval(interval); };
  }, [buscarFontes]);

  // mqtt subscription for streaming query results from the gateway
  useEffect(() => {
    return subscribe('nebulastream/results/#', (topic, payload) => {
      const parts = topic.split('/');
      const queryId = parts[parts.length - 1];

      try {
        const parsed = JSON.parse(payload) as Record<string, unknown> | Record<string, unknown>[];
        const rows = (Array.isArray(parsed) ? parsed : [parsed]).map(stripFieldPrefix);
        dispatchQueries({ type: 'APPEND_RESULTS', queryId, rows });
      } catch {
        // malformed json from broker, just skip it
      }
    });
  }, [subscribe]);

  const handleRemove = useCallback(
    async (id: string) => {
      const found = queries.find((q) => q.id === id);

      if(found?.coordinatorQueryId != null && (found?.status === 'running' || found?.status == 'pending')) {
        try {
          await apiStop(String(found.coordinatorQueryId));
        } catch {
        }
      }

      dispatchQueries({ type: 'REMOVE', id });
    },
    [queries, apiStop],
  );

  const handleSubmit = useCallback(
    async (req: QueryRequest) => {
      if (DEMO_MODE) {
        const demoId = `demo-q-${Date.now()}`;

        const q: Query = {
          id: demoId, coordinatorQueryId: null,
          request: req, status: 'running',
          results: [], error: null,
          createdAt: Date.now(),
        };
        dispatchQueries({ type: 'ADD', query: q });

        setTimeout(() => {
          dispatchQueries({
            type: 'MARK_COMPLETED', id: demoId,
            results: JSON.parse(generateQueryResults()) as Record<string, unknown>[],
          });
        }, 1500);
        return;
      }

      const sourceInfo = sources.find((s) => (s.name === req.source));
      const fieldTypes = sourceInfo?.fieldTypes ?? {};

      // idk if validating on the frontend is even necessary since the coordinator
      // also validates, but it gives a nicer error message
      const validation = validateQuery(req, fieldTypes);
      if (!validation.valid) {
        const errId = `err-${Date.now()}`;
        dispatchQueries({ type: 'ADD', query: {
          id: errId,
          coordinatorQueryId: null,
          request: req,
          status: 'failed',
          results: [],
          error: validation.errors.join(' '),
          createdAt: Date.now(),
        }});
        return;
      }

      const resp = await apiSubmit(req);

      dispatchQueries({ type: 'ADD', query: {
        id: resp.resultId,
        coordinatorQueryId: resp.coordinatorQueryId,
        request: req,
        status: 'pending',
        results: [],
        error: null,
        createdAt: Date.now(),
      }});

      // if we dont hear back from the coordinator in time, mark it as failed
      setTimeout(() => {
        dispatchQueries({ type: 'MARK_FAILED', id: resp.resultId, error: 'Query timed out waiting for results' });
      }, PENDING_TIMEOUT_MS);
    },
    [apiSubmit, sources],
  );

  const value = useMemo(
    () => ({
      queries, sources,
      submitQuery: handleSubmit,
      removeQuery: handleRemove,
      loadingSources, selectedDevices, setSelectedDevices,
    }),
    [queries, sources, handleSubmit, handleRemove, loadingSources, selectedDevices, setSelectedDevices],
  );

  return <QueryContext.Provider value={value}>{children}</QueryContext.Provider>;
}

export function useQuery(): QueryContextValue {
  const ctx = useContext(QueryContext);
  if(ctx === null) throw new Error('useQuery requires QueryProvider');
  return ctx;
}
