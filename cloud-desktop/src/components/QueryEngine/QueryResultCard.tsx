/* eslint-disable no-var */
import { useState, useMemo } from 'react';
import type { Query, QueryStatus } from '../../types';
import { EChart } from '../EChart/EChart';
import type { ECOption } from '../EChart/echarts-setup';
import styles from './QueryResultCard.module.css';

const META = new Set([
  'DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'join_key',
  'start', 'end',
]);

const TIME_INTERVALS = [
  { label: 'All', seconds: 0 },
  { label: '1m', seconds: 60 },
  { label: '5m', seconds: 300 },
  { label: '15m', seconds: 900 },
  { label: '1h', seconds: 3600 },
  { label: '6h', seconds: 21_600 },
];

interface Props {
  query: Query;
  defaultExpanded: boolean;
  onRemove: (id: string) => void;
  onRename?: (id: string, name: string) => void;
}

// palette from the design
const COLOURS = [
  '#00A0B0', '#7defa0', '#f5a623', '#c084fc',
  '#fb7185', '#67e8f9', '#fbbf24', '#a78bfa',
];

const STATUS_STYLES: Record<QueryStatus, string> = {
  pending: styles.statusPending,
  running: styles.statusRunning,
  completed: styles.statusCompleted,
  failed: styles.statusFailed,
  stopped: styles.statusStopped,
};

function numFlds(
  data: Record<string, unknown>[],
  fields: string[],
  joinFields?: string[],
): string[] {
  if (data.length == 0) return [];

  const numeric = Object.entries(data[0])
    .filter(([k, v]) => (typeof v === 'number') && !META.has(k))
    .map(([k]) => k);

  const combined = [...fields, ...(joinFields ?? [])];
  if(combined.length > 0){
    const allowed = new Set(combined);
    return numeric.filter((s) => allowed.has(s));
  }
  return numeric;
}

/**
 * Extracts epoch-ms timestamp from a telemetry row.
 *
 */
function getTs(row: Record<string, unknown>): number | null {
    for (const key of ['timestamp', 'start', 'end']) {
        const v = row[key];
        if (typeof v === 'number') {
            return v < 1e12 ? v * 1000 : v;
        }
    }
    return null;
}


function aggByTs(
  data: Record<string, unknown>[],
  fields: string[],
): Map<number, Record<string, number>> {
  const buckets = new Map<number, { somas: Record<string, number>; contagens: Record<string, number> }>();

  for (const row of data) {
    const ts = getTs(row);
    if (ts == null) continue;

    let bucket = buckets.get(ts);
    if (!bucket) {
      bucket = { somas: {}, contagens: {} };
      buckets.set(ts, bucket);
    }
    for (const f of fields) {
      var val = row[f];
      if (typeof val === 'number') {
        bucket.somas[f] = (bucket.somas[f] ?? 0) + val;
        bucket.contagens[f] = (bucket.contagens[f] ?? 0) + 1;
      }
    }
  }

  const result = new Map<number, Record<string, number>>();
  for (const [ts, { somas, contagens }] of buckets) {
    const avg: Record<string, number> = {};
    for (const f of fields) {
      avg[f] = contagens[f] ? somas[f] / contagens[f] : 0;
    }
    result.set(ts, avg);
  }
  return result;
}

function detectGws(data: Record<string, unknown>[]): string[] | null {
  var ids = new Set<string>();
  var i = 0, len = data.length;
  while (i < len) {
      var gw = data[i]['GATEWAY_ID'];
      if (gw !== undefined && gw !== null) ids.add('' + gw);
      i++;
  }
  if (ids.size < 2) return null;
  var out = Array.from(ids);
  out.sort();
  return out;
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
  data: Record<string, unknown>[],
  numericFields: string[],
  hasUnion?: boolean,
): ECOption {
  let gateways: string[] | null = null;
  if (hasUnion) gateways = detectGws(data);

  // console.log('[chart] %d rows, fields=%o, union=%s', data.length, numericFields, hasUnion);

  if (gateways) {
    type Spec = { type: 'line'; name: string; data: [number, number | null][]; smooth: boolean; showSymbol: boolean; lineStyle: { width: number; color: string }; itemStyle: { color: string } };
    var series: Spec[] = [];
    var legendNames: string[] = [];
    var colIdx = 0;

    for (const field of numericFields) {
      for (var g = 0; g < gateways.length; g++) {
        var gwId = gateways[g];
        var subset = data.filter((r) => String(r['GATEWAY_ID']) === gwId);
        var agg = aggByTs(subset, [field]);
        var timestamps = [...agg.keys()].sort((a, b) => a - b);
        var seriesName = field + ' (' + gwId + ')';
        var colour = COLOURS[colIdx % COLOURS.length];
        colIdx++;
        legendNames.push(seriesName);

        series.push({
          type: 'line', name: seriesName,
          data: timestamps.map((t) => [t, agg.get(t)?.[field] ?? null] as [number, number | null]),
          smooth: false, showSymbol: false,
          lineStyle: { width: 2, color: colour }, itemStyle: { color: colour },
        });
      }
    }

    return { tooltip: tooltipCfg, legend: { data: legendNames, top: 4, textStyle: { color: 'rgba(30,30,30,0.64)', fontSize: 11 } }, grid: gridCfg, xAxis: xAxisCfg, yAxis: yAxisCfg, dataZoom: [{ type: 'inside', start: 0, end: 100 }], series };
  }

  const aggregated = aggByTs(data, numericFields);
  const sortedTs = [...aggregated.keys()].sort((a, b) => a - b);
  const series2 = numericFields.map((field, i) => {
    const colour = COLOURS[i % COLOURS.length];
    return {
      type: 'line' as const,
      name: field,
      data: sortedTs.map((t) => [t, aggregated.get(t)?.[field] ?? null]),
      smooth: false,
      showSymbol: false,
      lineStyle: { width: 2, color: colour },
      itemStyle: { color: colour },
    };
  });

  return {
    tooltip: tooltipCfg,
    legend: { data: numericFields, top: 4, textStyle: { color: 'rgba(30,30,30,0.64)', fontSize: 11 } },
    grid: gridCfg,
    xAxis: xAxisCfg,
    yAxis: yAxisCfg,
    dataZoom: [{ type: 'inside', start: 0, end: 100 }],
    series: series2,
  };
}

export function QueryResultCard({ query, defaultExpanded, onRemove, onRename }: Props): React.JSX.Element {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [intervalSec, setIntervalSec] = useState(0);
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(query.request.name || '');

  const sorted = [...query.results].sort((a, b) => {
    const ta = getTs(a), tb = getTs(b);
    return (ta != null && tb != null) ? ta - tb : 0;
  });

  let filtered = sorted;
  if (intervalSec !== 0) {
    var cutoff = Date.now() - (intervalSec * 1000);
    filtered = sorted.filter((row) => {
      const t = getTs(row);
      return t !== null && t > cutoff;
    });
  }

  var cols = filtered.length > 0 ? Object.keys(filtered[0]) : [];

  const numeric = useMemo(
    () => numFlds(filtered, query.request.fields, query.request.joinFields),
    [filtered,query.request.fields, query.request.joinFields],
  );

  const hasUnion = !!(query.request.unionSources && query.request.unionSources.length);

  const chartOpt = useMemo(
    () => buildChartOpt(filtered, numeric, hasUnion),
    [filtered, numeric, hasUnion],
  );

  const hasChart = numeric.length > 0 && filtered.length > 0;
  const showTable = (query.status === 'completed' || query.status === 'stopped') && filtered.length > 0;
  const lastRows = filtered.slice(-20); // paginated table is on the roadmap 

  return (
    <div className={styles.card}>
      <div
        className={styles.header}
        role="button"
        tabIndex={0}
        onClick={() => setExpanded((v) => !v)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setExpanded((v) => !v);
          }
        }}
      >
        {editing ? (
          <input
            className={styles.queryId}
            autoFocus
            value={editName}
            onChange={(e) => setEditName(e.target.value)}
            onBlur={() => { setEditing(false); onRename?.(query.id, editName); }}
            onKeyDown={(e) => { if (e.key === 'Enter') { setEditing(false); onRename?.(query.id, editName); } if (e.key === 'Escape') setEditing(false); }}
            onClick={(e) => e.stopPropagation()}
            style={{ border: '1px solid #ccc', borderRadius: 4, padding: '2px 6px', fontSize: 'inherit', width: 180 }}
          />
        ) : (
          <span
            className={styles.queryId}
            title="Click to rename"
            onClick={(e) => { e.stopPropagation(); setEditing(true); setEditName(query.request.name || ''); }}
            style={{ cursor: 'text' }}
          >
            {query.request.name || query.id.slice(0, 8)}
          </span>
        )}

        <span className={styles.headerRight}>
          {filtered.length > 0 && (
            <span className={styles.rowCount}>
              {filtered.length} row{filtered.length !== 1 ? 's' : ''}
            </span>
          )}
          <span className={`${styles.badge} ${STATUS_STYLES[query.status]}`}>{query.status}</span>
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
              <div className={styles.intervalBar}>
                {TIME_INTERVALS.map(({ label, seconds }) => (
                  <button
                    key={label}
                    type="button"
                    className={`${styles.intervalBtn} ${intervalSec === seconds ? styles.intervalActive : ''}`}
                    onClick={() => setIntervalSec(seconds)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <EChart option={chartOpt} style={{ height: '240px' }} />
            </div>
          )}

          {showTable && <div className={styles.tableWrapper}>
            <table className={styles.table}>
              <thead><tr>{cols.map(c =>
                <th key={c} className={styles.th}>{c}</th>
              )}</tr></thead>
              <tbody>{lastRows.map((row, i) =>
                <tr key={i} className={styles.tr}>{cols.map(c =>
                  <td key={c} className={styles.td}>{row[c] != null ? String(row[c]) : ''}</td>
                )}</tr>
              )}</tbody>
            </table>
          </div>}

          {query.status === 'running' && filtered.length == 0 && (
            <div className={styles.waiting}>
              Waiting for first data point...
            </div>
          )}

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

          {query.status === 'stopped' && !filtered.length && (
            <div className={styles.waiting}>
              Query was stopped.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
