import { useState, useEffect, useCallback } from 'react';
import styles from './CoordinatorStatus.module.css';

// nES coordinator runs on ECS Fargate behind an ALB. The REST API is the
const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';

type ConnStatus = 'checking' | 'online' | 'offline';

interface LogicalSource { name: string; schema: string }

interface TopologyNode {
  id: number;
  ip_address: string;
  available_resources: number;
  nodeType: string;
  location: string | null;
}

interface TopologyEdge { source: number; target: number }

interface TopologyResponse {
  nodes: TopologyNode[];
  edges: TopologyEdge[];
}

// query lifecycle in NES: REGISTERED -> OPTIMIZING -> RUNNING -> STOPPED.
interface RegisteredQuery {
  queryId: number;
  queryString: string;
  queryStatus: string;
  queryPlan: string;
  queryMetaData: string;
}

export function CoordinatorStatus(): React.JSX.Element | null {
  const [open, setOpen] = useState(false);
  const [connSt, setConnSt] = useState<ConnStatus>('checking');
  const [topo, setTopo] = useState<TopologyResponse | null>(null);
  const [srcs, setSrcs] = useState<LogicalSource[]>([]);
  const [qs, setQs] = useState<RegisteredQuery[]>([]);
  const [loading, setLoading] = useState(false);
  const [expandedSrcs, setExpandedSrcs] = useState<Set<string>>(new Set());
  const [stoppingIds, setStoppingIds] = useState<Set<number>>(new Set());
  const [removingWkrs, setRemovingWkrs] = useState<Set<number>>(new Set());

  // returns 502 so we retry once after 1.5 s to ride out rolling updates.
  const refresh = useCallback(async () => {
    setLoading(true);
    setConnSt('checking');

    // --- connectivity (with 502 retry for ECS rolling deploys) ---
    let isOnline = false;
    for (let att = 0; att < 2; att++) {
      try {
        const r = await fetch(`${API_BASE}/v1/nes/connectivity/check`, {
          signal: AbortSignal.timeout(5000),
        });
        if (r.ok) { isOnline = true; break; }
        // aLB gives 502 while Fargate drains old task; wait and retry once
        if (r.status === 502 && att === 0) {
          await new Promise(ok => setTimeout(ok, 1500));
          continue;
        }
      } catch {
        if (att === 0) { await new Promise(ok => setTimeout(ok, 1500)); continue; }
      }
    }

    let topoData: TopologyResponse | null = null;
    try {
      const r = await fetch(`${API_BASE}/v1/nes/topology`);
      if (r.ok) topoData = (await r.json()) as TopologyResponse;
    } catch { /* coordinator might be unreachable, that's fine */ }

    let srcList: LogicalSource[] = [];
    try {
      const r = await fetch(`${API_BASE}/v1/nes/sourceCatalog/allLogicalSource`);
      if (r.ok) {
        const raw = (await r.json()) as Record<string, string>[];
        if (Array.isArray(raw)) {
          srcList = raw.map(entry => {
            const [nm, sch] = Object.entries(entry)[0] ?? ['', ''];
            return { name: nm, schema: sch };
          });
        }
      }
    } catch { /* swallow */ }

    // --- registered queries (all states) ---
    let queryList: RegisteredQuery[] = [];
    try {
      const r = await fetch(`${API_BASE}/v1/nes/queryCatalog/allRegisteredQueries`);
      if (r.ok) {
        const d = (await r.json()) as RegisteredQuery[];
        queryList = Array.isArray(d) ? d : [];
      }
    } catch { /* swallow - queries are non-critical here */ }

    setConnSt(isOnline ? 'online' : 'offline');
    setTopo(topoData);
    setSrcs(srcList);
    setQs(queryList);
    setLoading(false);
  }, []);

  const stopQuery = useCallback(async (qid: number) => {
    setStoppingIds(prev => new Set(prev).add(qid));
    try {
      const r = await fetch(
        `${API_BASE}/v1/nes/query/stop-query?queryId=${qid}`,
        { method: 'DELETE' },
      );
      if (!r.ok) console.warn(`stop query ${qid}: status ${r.status}`);
    } catch (e) {
      // TODO: surface this in a toast instead of console
      console.error(`stop query ${qid} failed`, e);
    } finally {
      setStoppingIds(prev => { const s = new Set(prev); s.delete(qid); return s; });
      void refresh();
    }
  }, [refresh]);

  // the coordinator still references stale physical sources.
  const removeWorker = useCallback(async (wid: number) => {
    setRemovingWkrs(prev => new Set(prev).add(wid));
    try {
      const r = await fetch(
        `${API_BASE}/v1/nes/sourceCatalog/removeAllPhysicalSourcesByWorker?workerId=${wid}`,
        { method: 'DELETE' },
      );
      if (!r.ok) console.warn(`remove worker ${wid}: ${r.status}`);
    } catch (e) {
      console.error(`remove worker ${wid}:`, e);
    } finally {
      setRemovingWkrs(prev => { const s = new Set(prev); s.delete(wid); return s; });
      void refresh();
    }
  }, [refresh]);

  useEffect(() => {
    const handler = () => { setOpen(true); void refresh(); };
    window.addEventListener('menu:coordinator-status', handler);
    return () => window.removeEventListener('menu:coordinator-status', handler);
  }, [refresh]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open]);

  const nodes = topo?.nodes ?? [];
  let coordId = -1;
  if (topo) {
    const srcIds = new Set(topo.edges.map(e => e.source));
    const cNode = topo.nodes.find(n => !srcIds.has(n.id));
    coordId = cNode?.id ?? Math.min(...topo.nodes.map(n => n.id));
  }
  const coordNode = nodes.find(n => n.id === coordId) ?? null;
  const wkrs = nodes.filter(n => n.id !== coordId);

  if (!open) return null;

  const parseFields = (schema: string): Array<{ name: string; type: string }> =>
    schema.split(/\s+/).filter(s => s.includes(':')).map(s => {
      const [n, ...tp] = s.split(':');
      return { name: n, type: tp.join(':') };
    });

  return (
    <div className={styles.overlay} onClick={() => setOpen(false)}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div className={styles.header}>
          <h2 className={styles.title}>NES Coordinator</h2>
          <div className={styles.headerActions}>
            <button
              type="button"
              className={styles.refreshBtn}
              onClick={() => { void refresh(); }}
              disabled={loading}
              title="Refresh"
            >
              {loading ? 'Refreshing...' : 'Refresh'}
            </button>
            <button type="button" className={styles.closeBtn} onClick={() => setOpen(false)} aria-label="Close">
              &times;
            </button>
          </div>
        </div>

        <div className={styles.body}>
          {/* ---- Connectivity ---- */}
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>Connectivity</h3>
            <div className={styles.statusRow}>
              <span className={styles.dot} data-status={connSt} />
              <span className={styles.statusLabel}>
                {connSt === 'checking' ? 'Checking...' : connSt === 'online' ? 'Online' : 'Offline'}
              </span>
              <span className={styles.endpoint}>{API_BASE}</span>
            </div>
          </section>

          {/* ---- Topology tree (coordinator at root, workers as leaves) ---- */}
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>
              Topology
              {nodes.length > 0 && (
                <span className={styles.count}>
                  {nodes.length} node{nodes.length !== 1 ? 's' : ''} ({wkrs.length} worker{wkrs.length !== 1 ? 's' : ''})
                </span>
              )}
            </h3>
            {connSt === 'offline' && <p className={styles.muted}>Coordinator unreachable</p>}
            {connSt === 'online' && nodes.length === 0 && <p className={styles.muted}>No topology data</p>}
            {coordNode && (
              <div className={styles.topoTree}>
                <div className={styles.topoRoot}>
                  <div className={styles.topoCard} data-role="coord">
                    <span className={styles.topoRole}>Coordinator</span>
                    <span className={styles.topoIp}>{coordNode.ip_address}</span>
                    <span className={styles.topoMeta}>
                      #{coordNode.id} &middot; {coordNode.available_resources.toLocaleString()} slots
                    </span>
                  </div>
                </div>
                {wkrs.length > 0 && <div className={styles.topoStem} />}
                {wkrs.length > 0 && (
                  <div className={styles.topoWorkers}>
                    {wkrs.length > 1 && <div className={styles.topoRail} />}
                    {wkrs.map(w => {
                      const busy = removingWkrs.has(w.id);
                      return (
                        <div key={w.id} className={styles.topoWorkerCol}>
                          <div className={styles.topoBranch} />
                          <div className={styles.topoCard} data-role="worker">
                            <span className={styles.topoRole}>Worker</span>
                            <span className={styles.topoIp}>{w.ip_address}</span>
                            <span className={styles.topoMeta}>
                              #{w.id} &middot; {w.available_resources.toLocaleString()} slots
                            </span>
                            <button
                              type="button"
                              className={styles.removeWorkerBtn}
                              onClick={() => { void removeWorker(w.id); }}
                              disabled={busy}
                              title={`Remove worker #${w.id} and its physical sources`}
                              aria-label={`Remove worker ${w.id}`}
                            >
                              {busy ? 'Removing...' : 'Remove'}
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            )}
          </section>

          {/* ---- Logical sources registered in the NES source catalog ---- */}
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>
              Logical Sources
              {srcs.length > 0 && <span className={styles.count}>{srcs.length}</span>}
            </h3>
            {connSt === 'offline' && <p className={styles.muted}>Coordinator unreachable</p>}
            {connSt === 'online' && srcs.length === 0 && <p className={styles.muted}>No logical sources registered</p>}
            {srcs.length > 0 && (
              <div className={styles.sourceList}>
                {srcs.map(src => {
                  const flds = parseFields(src.schema);
                  const expanded = expandedSrcs.has(src.name);
                  return (
                    <button
                      key={src.name}
                      type="button"
                      className={styles.sourceItem}
                      onClick={() => {
                        setExpandedSrcs(prev => {
                          const nxt = new Set(prev);
                          if (nxt.has(src.name)) nxt.delete(src.name); else nxt.add(src.name);
                          return nxt;
                        });
                      }}
                    >
                      <div className={styles.sourceHeader}>
                        <span className={styles.sourceChevron} data-open={expanded ? 'true' : 'false'}>&#9656;</span>
                        <span className={styles.sourceName}>{src.name}</span>
                        <span className={styles.sourceFieldCount}>{flds.length} field{flds.length !== 1 ? 's' : ''}</span>
                      </div>
                      {expanded && (
                        <div className={styles.fieldList}>
                          {flds.map(f => (
                            <span key={f.name} className={styles.field}>
                              <span className={styles.fieldName}>{f.name}</span>
                              <span className={styles.fieldType}>{f.type}</span>
                            </span>
                          ))}
                        </div>
                      )}
                    </button>
                  );
                })}
              </div>
            )}
          </section>

          {/* ---- Queries: REGISTERED -> OPTIMIZING -> RUNNING -> STOPPED ---- */}
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>
              Queries
              {qs.length > 0 && <span className={styles.count}>{qs.length}</span>}
            </h3>
            {connSt === 'offline' && <p className={styles.muted}>Coordinator unreachable</p>}
            {connSt === 'online' && qs.length === 0 && <p className={styles.muted}>No registered queries</p>}
            {qs.length > 0 && (
              <div className={styles.queryList}>
                {qs.map(q => {
                  // should also be stoppable, the coordinator sometimes
                  const active = q.queryStatus === 'RUNNING' || q.queryStatus === 'OPTIMIZING';
                  const stopping = stoppingIds.has(q.queryId);
                  return (
                    <div key={q.queryId} className={styles.queryItem}>
                      <div className={styles.queryHeader}>
                        <span className={styles.queryId}>Query #{q.queryId}</span>
                        <span className={styles.queryStatus} data-status={q.queryStatus.toLowerCase()}>
                          {q.queryStatus}
                        </span>
                        {active && (
                          <button
                            type="button"
                            className={styles.stopBtn}
                            onClick={() => { void stopQuery(q.queryId); }}
                            disabled={stopping}
                            title={`Stop query #${q.queryId}`}
                            aria-label={`Stop query ${q.queryId}`}
                          >
                            {stopping ? 'Stopping...' : 'Stop'}
                          </button>
                        )}
                      </div>
                      <code className={styles.queryString}>{q.queryString}</code>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
