import { useState, useCallback, useRef, useEffect } from 'react';
import { useQueryService } from './useQueryService';
import type { LogicalSource } from './useQueryService';

const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';
const MQTT_SINK_URL = import.meta.env.VITE_NES_MQTT_SINK_URL ?? '';
const NES_SINK_TOPIC = 'nebulastream/telemetry';

const RETRY_DELAY = 10_000;

export type NesStreamStatus = 'discovering' | 'ready' | 'submitting' | 'running' | 'error';

export interface NesStreamState {
  status: NesStreamStatus;
  source: string | null;
  sources: LogicalSource[];
  error: string | null;
  queryId: string | null;
  dsl: string | null;
  start: () => void;
  stop: () => void;
  refresh: () => void;
}

interface ExecuteQueryResponse {
  queryId: number;
}

// builds the DSL string for scanning a source into mqtt
export function buildScanDsl(sourceName: string): string {
  return `Query::from("${sourceName}").sink(MQTTSinkDescriptor::create("${MQTT_SINK_URL}", "${NES_SINK_TOPIC}", "", 1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));`;
}

// TODO: this state machine is getting messy, maybe refactor later
// discovering -> ready -> submitting -> running (or error at any point)
export function useNesStream(): NesStreamState {
  const { fetchSources, stopQuery } = useQueryService();

  const [status, setStatus] = useState<NesStreamStatus>('discovering');
  const [source, setSource] = useState<string | null>(null);
  const [sources, setSources] = useState<LogicalSource[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [queryId, setQueryId] = useState<string | null>(null);
  const [dsl, setDsl] = useState<string | null>(null);

  const qRef = useRef<string | null>(null);

  // cycle counter to force re-discovery
  const [cycle, setCycle] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    async function discover(): Promise<void> {
      if (cancelled) return;
      setStatus('discovering');
      setError(null);

      try {
        const srcs = await fetchSources();
        if (cancelled) return;

        setSources(srcs);

        if (srcs.length === 0) {
          setStatus('error');
          setError('no sources found - is the worker registered?');
          // retry after delay
          retryTimer = setTimeout(() => {
            if (!cancelled) void discover();
          }, RETRY_DELAY);
          return;
        }

        // pick the telemetry source if available, otherwise just use first one
        const selected = srcs.find((s) => s.name.startsWith('telemetry_')) ?? srcs[0];
        setSource(selected.name);
        setDsl(buildScanDsl(selected.name));
        setStatus('ready');
      } catch (err) {
        if (cancelled) return;
        setStatus('error');
        setError(err instanceof Error ? err.message : 'could not connect to NES coordinator');
        retryTimer = setTimeout(() => {
          if (!cancelled) void discover();
        }, RETRY_DELAY);
      }
    }

    void discover();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [fetchSources, cycle]);

  // not sure if this unmount cleanup is right
  useEffect(() => {
    return () => {
      if (qRef.current) {
        stopQuery(qRef.current).catch(() => {});
        qRef.current = null;
      }
    };
  }, [stopQuery]);

  const start = useCallback(async () => {
    if (!source || !dsl) return;
    setStatus('submitting');
    setError(null);

    // console.log("debug submitting query", dsl)
    const res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userQuery: dsl, placement: 'BottomUp' }),
    });

    if (!res.ok) {
      setStatus('error');
      setError(`Query submission failed: ${res.status}`);
      return;
    }

    const data = (await res.json()) as ExecuteQueryResponse;
    const id = String(data.queryId);

    setQueryId(id);
    qRef.current = id;
    setStatus('running');
  }, [source, dsl]);

  const stop = useCallback(async () => {
    if (!qRef.current) return;
    try {
      await stopQuery(qRef.current);
    } catch {
      // works for now
    }
    qRef.current = null;
    setQueryId(null);
    setStatus('ready');
  }, [stopQuery]);

  const refresh = useCallback(() => {
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    (async () => {
      if (qRef.current) {
        try {
          await stopQuery(qRef.current);
        } catch {
          // best effort
        }
        qRef.current = null;
      }
    })();

    setQueryId(null);
    setDsl(null);
    setSource(null);
    setCycle((n) => n + 1);
  }, [stopQuery]);

  return {
    status,
    source,
    sources,
    error,
    queryId,
    dsl,
    start: () => void start(),
    stop: () => void stop(),
    refresh,
  };
}
