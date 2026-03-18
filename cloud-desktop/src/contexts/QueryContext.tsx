/* eslint-disable no-var */


import { createContext, useContext, useState, useCallback, useMemo, useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { useMqtt } from './MqttContext';
import { useQueryService, validateQuery } from '../hooks/useQueryService';
import type { LogicalSource } from '../hooks/useQueryService';
import type { Query, QueryRequest } from '../types';


var REGISTER_RANGES: Record<string, [number, number]> = {
  STATUS_ID_MOTOR_RPM: [0, 5000],
  STATUS_ID_TEMP_CABINET: [-20, 80],     // NTC 10K, celsius
  STATUS_ID_COMP_SPEED: [0, 4500],       // inverter RPM
  STATUS_ID_MOTOR_SPEED: [0, 255],       // PWM duty cycle
  PARAM_MOTOR_RPM: [0, 5000],
  PARAM_MOTOR_CURRENT: [0, 30],          // amps, hall sensor
  PARAM_EVAP_TEMP: [-40, 50],            // celsius
};



function checkThresholdRange(field: string, raw: string) {
  var range = REGISTER_RANGES[field];
  if(!range) return;
  var num = parseFloat(raw);
  if(Number.isNaN(num)) return;
  if (num < range[0] || num > range[1]) {
    console.warn(`[query] threshold ${num} for ${field} is outside expected range [${range[0]}..${range[1]}] — results will probably be empty`);
  }
}

// NES coordinator 
var NES_OVERHEAD = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'join_key', 'start', 'end']);

var DEMO_SOURCES: LogicalSource[] = [];
function _fakeDemoResults(): string { return '[]'; }
var DEMO = import.meta.env.VITE_DEMO_MODE === 'true';

var MAX_BUF = 500;
const POLL_MS = 15_000;

var QUERY_TIMEOUT = 30_000;

const STALE_CUTOFF = 90_000;

interface QueryContextValue {
  queries: Query[];
  sources: LogicalSource[];
  submitQuery: (request: QueryRequest) => Promise<void>;
  removeQuery: (id: string) => Promise<void>;
  renameQuery: (id: string, name: string) => void;
  loadingSources: boolean;
  selectedDevices: string[];
  setSelectedDevices: (devices: string[]) => void;
}

var Ctx = createContext<QueryContextValue | null>(null);

export function QueryProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const { subscribe } = useMqtt();
  const { fetchSources: buscarFontes, submitQuery: apiSubmit, stopQuery: apiStop } = useQueryService();

  const [queries, setQueries] = useState<Query[]>([]);
  const [sources, setSources] = useState<LogicalSource[]>([]);
  const [loadingSources, setLoadingSources] = useState(true);
  const [selectedDevices, mySetDevices] = useState<string[]>([]);


  var qRef = useRef(queries);
  qRef.current = queries;

  const setSelectedDevices = useCallback((a: string[]) => { mySetDevices(a); }, []);

  // periodic sweep for zombie queries stuck in pending
  useEffect(() => {
    var t = setInterval(() => {
      var now = Date.now();
      setQueries(prev => {
        var changed = false;
        var next = prev.map(q => {
          if(q.status !== 'pending') return q;
          var age = now - q.createdAt;
          if(age < STALE_CUTOFF) return q;
          changed = true;
          if (import.meta.env.DEV) console.debug('[query] stale query %s (%ds)', q.id, Math.round(age/1000));
          return { ...q, status: 'failed' as const, error: `Stuck in pending for ${Math.round(age/1000)}s — coordinator lost it` };
        });
        return changed ? next : prev;
      });
    }, 20_000);
    return () => clearInterval(t);
  }, []);

  // poll coordinator for logical sources (one per connected gateway)
  useEffect(() => {
    var alive = true;
    var iv: ReturnType<typeof setInterval> | undefined;

    async function doTheFetch() {
      try {
        var raw = await buscarFontes();
        if(!alive) return;
        var good = raw.filter(s => {
          if(s.name === 'default_logical') return false;
          for(var i = 0; i < s.fields.length; i++){
            if(!NES_OVERHEAD.has(s.fields[i])) return true;
          }
          return false;
        });
        setSources(good);
      } catch(err) {
        if(DEMO) setSources(DEMO_SOURCES);
        else console.warn('[query] coordinator unreachable, probably restarting', err);
      } finally {
        if(alive) setLoadingSources(false);
      }
    }

    void doTheFetch();
    if(!DEMO) iv = setInterval(() => void doTheFetch(), POLL_MS);
    return () => { alive = false; if(iv) clearInterval(iv); };
  }, [buscarFontes]);

  // NES pushes query results to nebulastream/results/<resultId>
  useEffect(() => {
    return subscribe('nebulastream/results/#', function onResult(topic, payload) {
      var qid = topic.split('/').pop()!;
      var rows: Record<string, unknown>[];
      try {
        var p = JSON.parse(payload);
        var arr = Array.isArray(p) ? p : [p];
        // strip "sourceName$field" prefixes inline
        rows = arr.map(obj => {
          var out: Record<string, unknown> = {};
          for(var [k, v] of Object.entries(obj)) {
            var clean = k.includes('$') ? k.substring(k.indexOf('$') + 1) : k;
            if((clean in out) && NES_OVERHEAD.has(clean)) continue;
            out[clean] = v;
          }
          return out;
        });
      } catch {
        return; // partial JSON from MQTTSink buffer flush, skip
      }

      setQueries(prev => {
        var i = prev.findIndex(q => q.id === qid);
        if(i === -1) return prev;
        var t = prev[i];
        var buf = [...t.results, ...rows].slice(-MAX_BUF);
        var next = [...prev];
        next[i] = { ...t, status: 'running' as const, results: buf };
        return next;
      });
    });
  }, [subscribe]);

  var stopAndRemove = useCallback(async (id: string) => {
    var q = qRef.current.find(x => x.id === id);
    if(q?.coordinatorQueryId != null && (q.status === 'running' || q.status == 'pending')) {
      try { await apiStop(String(q.coordinatorQueryId)); }
      catch(e) { console.warn('[query] stop failed (query probably already gone):', e); }
    }
    setQueries(prev => prev.filter(x => x.id !== id));
  }, [apiStop]);

  var fireQuery = useCallback(async (req: QueryRequest) => {
    if(DEMO) {
      var did = `demo-q-${Date.now()}`;
      setQueries(prev => [{ id: did, coordinatorQueryId: null, request: req, status: 'running' as const, results: [], error: null, createdAt: Date.now() }, ...prev]);
      setTimeout(() => {
        setQueries(prev => prev.map(q => q.id === did ? { ...q, status: 'completed' as const, results: JSON.parse(_fakeDemoResults()) } : q));
      }, 1500);
      return;
    }

 

    for(var f of req.filters) checkThresholdRange(f.field, f.value);

    var src = sources.find(s => s.name === req.source);
    var validation = validateQuery(req, src?.fieldTypes ?? {});
    if(!validation.valid) {
      setQueries(prev => [{ id: `err-${Date.now()}`, coordinatorQueryId: null, request: req, status: 'failed' as const, results: [], error: validation.errors.join(' '), createdAt: Date.now() }, ...prev]);
      return;
    }

    try {
      var resp = await apiSubmit(req);
    } catch(err) {
      var msg = err instanceof Error ? err.message : String(err);
      console.error('[query] coordinator rejected:', msg, req);
      setQueries(prev => [{ id: `err-${Date.now()}`, coordinatorQueryId: null, request: req, status: 'failed' as const, results: [], error: `Coordinator error: ${msg}`, createdAt: Date.now() }, ...prev]);
      return;
    }

    setQueries(prev => [{ id: resp.resultId, coordinatorQueryId: resp.coordinatorQueryId, request: req, status: 'pending' as const, results: [], error: null, createdAt: Date.now() }, ...prev]);

    setTimeout(() => {
      setQueries(prev => prev.map(q => {
        if(q.id !== resp.resultId || q.status !== 'pending') return q;
        return { ...q, status: 'failed' as const, error: 'NES query timed out — coordinator did not respond' };
      }));
    }, QUERY_TIMEOUT);
  }, [apiSubmit, sources]);

  const renameQuery = useCallback((id: string, name: string) => {
    setQueries((prev) => prev.map((q) =>
      q.id === id ? { ...q, request: { ...q.request, name } } : q
    ));
  }, []);

  var val = useMemo(() => ({
    queries, sources, submitQuery: fireQuery, removeQuery: stopAndRemove, renameQuery,
    loadingSources, selectedDevices, setSelectedDevices,
  }), [queries, sources, fireQuery, stopAndRemove, renameQuery, loadingSources, selectedDevices, setSelectedDevices]);

  return <Ctx.Provider value={val}>{children}</Ctx.Provider>;
}

export function useQuery(): QueryContextValue {
  var c = useContext(Ctx);
  if(c === null) throw new Error('useQuery requires QueryProvider');
  return c;
}
