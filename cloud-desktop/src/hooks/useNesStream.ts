/* eslint-disable prefer-const */
/* eslint-disable no-var */
// NES coordinator telemetry stream lifecycle 

import { useState, useCallback, useRef, useEffect } from 'react';
import { useQueryService } from './useQueryService';
import type { LogicalSource } from './useQueryService';

var API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';
let MQTT_SINK_URL = import.meta.env.VITE_NES_MQTT_SINK_URL ?? '';
const NES_SINK_TOPIC = 'nebulastream/telemetry';

// ECS cold starts can take 30-60s
var TEMPO_RETRY_DISCOVERY = 10_000;

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

// builds the scan DSL 
export function buildScanDsl(sourceName: string): string {
  return `Query::from("${sourceName}").sink(MQTTSinkDescriptor::create("${MQTT_SINK_URL}", "${NES_SINK_TOPIC}", "", 1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));`;
}

// state machine: discovering -> ready -> submitting -> running --------------------------
//                                                   \-> error
// any state can go to error, refresh() resets to discovering
export function useNesStream(): NesStreamState {
  const { fetchSources, stopQuery } = useQueryService();

  const [status, setStatus] = useState<NesStreamStatus>('discovering');
  const [source, setSource] = useState<string | null>(null);
  const [sources, setSources] = useState<LogicalSource[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [queryId, setQueryId] = useState<string | null>(null);
  const [dsl, setDsl] = useState<string | null>(null);

  var activeQueryRef = useRef<string | null>(null);

  const [cicloDescoberta, setCicloDescoberta] = useState(0); // discovery cycle counter

  useEffect(() => {
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    async function discover(): Promise<void> {
      if (cancelled) return;
      setStatus('discovering');
      setError(null);

      try {
        var srcs = await fetchSources();
        if (cancelled) return;

        setSources(srcs);

        if (srcs.length === 0) {
          setStatus('error');
          setError('no sources found - is the worker registered?');
          retryTimer = setTimeout(() => {
            if (!cancelled) void discover();
          }, TEMPO_RETRY_DISCOVERY);
          return;
        }

        let fonteEscolhida = srcs.find((s) => s.name.startsWith('telemetry_')) ?? srcs[0];
        setSource(fonteEscolhida.name);
        setDsl(buildScanDsl(fonteEscolhida.name));
        setStatus('ready');
      } catch (err) {
        if (cancelled) return;
        // FIXME: ECS cold start can cause ECONNREFUSED here
        setStatus('error');
        setError(err instanceof Error ? err.message : 'could not connect to NES coordinator');
        retryTimer = setTimeout(() => {
          if (!cancelled) void discover();
        }, TEMPO_RETRY_DISCOVERY);
      }
    }

    void discover();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [fetchSources, cicloDescoberta]);

  useEffect(() => {
    return () => {
      if (activeQueryRef.current) {
        stopQuery(activeQueryRef.current).catch(() => {
        });
        activeQueryRef.current = null;
      }
    };
  }, [stopQuery]);

  const start = useCallback(async () => {
    if (!source || !dsl) return;
    setStatus('submitting');
    setError(null);

    try {
      var res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userQuery: dsl, placement: 'BottomUp' }),
      });

      if (!res.ok) throw new Error(`Query submission failed: ${res.status}`);

      let data = (await res.json()) as ExecuteQueryResponse;
      var id = String(data.queryId);

      setQueryId(id);
      activeQueryRef.current = id;
      setStatus('running');
    } catch (err) {
      setStatus('error');
      setError(err instanceof Error ? err.message : 'Failed to submit query');
    }
  }, [source, dsl]);

  const stop = useCallback(async () => {
    if (!activeQueryRef.current) return;
    try {
      await stopQuery(activeQueryRef.current);
    } catch {
      // best effort
    }
    activeQueryRef.current = null;
    setQueryId(null);
    setStatus('ready');
  }, [stopQuery]);

  const refresh = useCallback(() => {
    void (async () => {
      if (activeQueryRef.current) {
        try {
          await stopQuery(activeQueryRef.current);
        } catch {
          /* nao importa se falhar aqui */
        }
        activeQueryRef.current = null;
      }
    })();

    setQueryId(null);
    setDsl(null);
    setSource(null);
    setCicloDescoberta((n) => n + 1);
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
