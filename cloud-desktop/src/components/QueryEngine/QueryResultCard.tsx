import { useState, useMemo } from 'react';
import type { Query, QueryStatus } from '../../types';
import { EChart } from '../EChart/EChart';
import type { ECOption } from '../EChart/echarts-setup';
import styles from './QueryResultCard.module.css';

const META = new Set([
  'DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'join_key',
  'start', 'end',
]);

interface Props {
  query: Query;
  defaultExpanded: boolean;
  onRemove: (id: string) => void;
}

const COLOURS = [
  '#00A0B0', '#7defa0', '#f5a623', '#c084fc',
  '#fb7185', '#67e8f9', '#fbbf24', '#a78bfa',
];

  // TODO: move chart colour palette to a shared theme config
  // so telemetry charts and query result charts stay in sync
  console.log('[QueryResultCard] COLOURS loaded, count:', COLOURS.length);
  // const CHART_DEBUG = import.meta.env.DEV;
  // if (CHART_DEBUG) console.log('chart debug mode enabled');

function badgeCls(st: QueryStatus): string {
  const m: Record<QueryStatus, string> = {
    pending: styles.statusPending,
    running: styles.statusRunning,
    completed: styles.statusCompleted,
    failed: styles.statusFailed,
    stopped: styles.statusStopped,
  };
  return `${styles.badge} ${m[st]}`;
}

function getCols(rows: Record<string, unknown>[]): string[] {
  if (rows.length === 0) return [];
  return Object.keys(rows[0]);
}

function numFlds(
  rows: Record<string, unknown>[],
  selected: string[],
  joinFlds?: string[],
): string[] {
  if (rows.length === 0) return [];
  const allNum = Object.entries(rows[0])
    .filter(([k, v]) => typeof v === 'number' && !META.has(k))
    .map(([k]) => k);
  const combined = [...selected, ...(joinFlds ?? [])];
  if (combined.length > 0) {
    const set = new Set(combined);
    return allNum.filter((f) => set.has(f));
  }
  return allNum;
}

function getTs(row: Record<string, unknown>): number | null {
  for (const k of ['timestamp', 'start', 'end']) {
    const v = row[k];
    if (typeof v === 'number') return v;
  }
  return null;
}

function aggByTs(
  rows: Record<string, unknown>[],
  fields: string[],
): Map<number, Record<string, number>> {
  const buckets = new Map<number, { sums: Record<string, number>; counts: Record<string, number> }>();

  for (const row of rows) {
    const ts = getTs(row);
    if (ts == null) continue;

    let b = buckets.get(ts);
    if (!b) {
      b = { sums: {}, counts: {} };
      buckets.set(ts, b);
    }
    for (const f of fields) {
      const v = row[f];
      if (typeof v === 'number') {
        b.sums[f] = (b.sums[f] ?? 0) + v;
        b.counts[f] = (b.counts[f] ?? 0) + 1;
      }
    }
  }

  const out = new Map<number, Record<string, number>>();
  for (const [ts, { sums, counts }] of buckets) {
    const avg: Record<string, number> = {};
    for (const f of fields) {
      avg[f] = counts[f] ? sums[f] / counts[f] : 0;
    }
    out.set(ts, avg);
  }
  return out;
}

function detectGws(rows: Record<string, unknown>[]): string[] | null {
  const gws = new Set<string>();
  for (const row of rows) {
    const gw = row['GATEWAY_ID'];
    if (typeof gw === 'string' || typeof gw === 'number') gws.add(String(gw));
  }
  return gws.size > 1 ? [...gws].sort() : null;
}

const tooltipCfg = {
  trigger: 'axis' as const,
  backgroundColor: '#ffffff',
  borderColor: 'rgba(0,0,0,0.08)',
  textStyle: { color: '#1a1a1a', fontSize: 12 },
};
const gridCfg = { top: 36, right: 16, bottom: 32, left: 56 };
const xAxisCfg = {
  type: 'time' as const,
  axisLine: { lineStyle: { color: 'rgba(0,0,0,0.08)' } },
  axisLabel: { color: 'rgba(30,30,30,0.48)', fontSize: 11 },
  splitLine: { show: false },
};
const yAxisCfg = {
  type: 'value' as const,
  scale: true,
  boundaryGap: ['10%', '10%'] as [string, string],
  axisLine: { show: false },
  axisLabel: { color: 'rgba(30,30,30,0.48)', fontSize: 11 },
  splitLine: { lineStyle: { color: 'rgba(0,0,0,0.06)' } },
};

function buildChartOpt(
  rows: Record<string, unknown>[],
  fields: string[],
  isUnion?: boolean,
): ECOption {
  const gws = isUnion ? detectGws(rows) : null;

  if (gws) {
    type Spec = { type: 'line'; name: string; data: [number, number | null][]; smooth: boolean; showSymbol: boolean; lineStyle: { width: number; color: string }; itemStyle: { color: string } };
    const allSeries: Spec[] = [];
    const legend: string[] = [];
    let ci = 0;

    for (const field of fields) {
      for (const gw of gws) {
        const gwRows = rows.filter((r) => String(r['GATEWAY_ID']) === gw);
        const agg = aggByTs(gwRows, [field]);
        const sorted = [...agg.keys()].sort((a, b) => a - b);
        const nm = `${field} (${gw})`;
        const clr = COLOURS[ci % COLOURS.length];
        ci++;

        legend.push(nm);
        allSeries.push({
          type: 'line' as const,
          name: nm,
          data: sorted.map((ts) => [ts, agg.get(ts)?.[field] ?? null]),
          smooth: false,
          showSymbol: false,
          lineStyle: { width: 2, color: clr },
          itemStyle: { color: clr },
        });
      }
    }

    return {
      tooltip: tooltipCfg,
      legend: { data: legend, top: 4, textStyle: { color: 'rgba(30,30,30,0.64)', fontSize: 11 } },
      grid: gridCfg,
      xAxis: xAxisCfg,
      yAxis: yAxisCfg,
      dataZoom: [{ type: 'inside', start: 0, end: 100 }],
      series: allSeries,
    };
  }

  const agg = aggByTs(rows, fields);
  const sortedTs = [...agg.keys()].sort((a, b) => a - b);

  const series = fields.map((f, i) => ({
    type: 'line' as const,
    name: f,
    data: sortedTs.map((ts) => [ts, agg.get(ts)?.[f] ?? null]),
    smooth: false,
    showSymbol: false,
    lineStyle: { width: 2, color: COLOURS[i % COLOURS.length] },
    itemStyle: { color: COLOURS[i % COLOURS.length] },
  }));

  return {
    tooltip: tooltipCfg,
    legend: { data: fields, top: 4, textStyle: { color: 'rgba(30,30,30,0.64)', fontSize: 11 } },
    grid: gridCfg,
    xAxis: xAxisCfg,
    yAxis: yAxisCfg,
    dataZoom: [{ type: 'inside', start: 0, end: 100 }],
    series,
  };
}

// nES results can arrive out of order during coordinator failover, the new
export function QueryResultCard({ query, defaultExpanded, onRemove }: Props): React.JSX.Element {
  const [expanded, setExpanded] = useState(defaultExpanded);

  const sorted = useMemo(() => {
    const rows = [...query.results];
    rows.sort((a, b) => {
      const ta = getTs(a);
      const tb = getTs(b);
      if (ta != null && tb != null) return ta - tb;
      return 0;
    });
    return rows;
  }, [query.results]);

  const cols = getCols(sorted);
  const chartFlds = useMemo(
    () => numFlds(sorted, query.request.fields, query.request.joinFields),
    [sorted, query.request.fields, query.request.joinFields],
  );
  const isUnion = (query.request.unionSources?.length ?? 0) > 0;
  const chartOpt = useMemo(
    () => buildChartOpt(sorted, chartFlds, isUnion),
    [sorted, chartFlds, isUnion],
  );

  const hasChart = chartFlds.length > 0 && sorted.length > 0;

  return (
    <div className={styles.card}>
      <div
        className={styles.header}
        role="button"
        tabIndex={0}
        onClick={() => setExpanded((p) => !p)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setExpanded((p) => !p);
          }
        }}
      >
        {/* first 8 chars of the UUID - enough to identify in the coordinator logs */}
        <span className={styles.queryId}>{query.id.slice(0, 8)}</span>

        <span className={styles.headerRight}>
          {sorted.length > 0 && (
            <span className={styles.rowCount}>
              {sorted.length} row{sorted.length !== 1 ? 's' : ''}
            </span>
          )}
          <span className={badgeCls(query.status)}>{query.status}</span>
          <button
            type="button"
            className={styles.deleteBtn}
            title="Stop and remove query"
            aria-label="Delete query"
            onClick={(e) => {
              e.stopPropagation();
              onRemove(query.id);
            }}
          >
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M5 2V1h6v1h4v1H1V2h4zm1 3v8h1V5H6zm3 0v8h1V5H9zM2 4l1 11h10l1-11H2z"
                fill="currentColor" opacity="0.7" />
            </svg>
          </button>
        </span>
      </div>

      {expanded && (
        <div className={styles.body}>
          {hasChart && (
            <div className={styles.chartWrapper}>
              <EChart option={chartOpt} style={{ height: '240px' }} />
            </div>
          )}

          {/* show the raw table only after the query finishes - displaying
              partial results mid-stream caused layout thrashing */}
          {(query.status === 'completed' || query.status === 'stopped') && sorted.length > 0 && (
            <div className={styles.tableWrapper}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    {cols.map((c) => (
                      <th key={c} className={styles.th}>{c}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {sorted.slice(-20).map((row, idx) => (
                    <tr key={idx} className={styles.tr}>
                      {cols.map((c) => (
                        <td key={c} className={styles.td}>
                          {String(row[c] ?? '')}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {query.status === 'running' && sorted.length === 0 && (
            <div className={styles.waiting}>
              Waiting for first data point...
            </div>
          )}

          {/* error display is intentionally sparse - the coordinator often
              returns cryptic C++ error messages that aren't useful to the
              end user.  TODO: parse known error patterns and show friendlier text */}
          {query.status === 'failed' && (
            <div className={styles.error}>
              {query.error ?? 'Unknown error'}
            </div>
          )}

          {query.status === 'pending' && (
            <div className={styles.waiting}>
              Waiting for results...
            </div>
          )}

          {query.status === 'stopped' && sorted.length === 0 && (
            <div className={styles.waiting}>
              Query was stopped.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
