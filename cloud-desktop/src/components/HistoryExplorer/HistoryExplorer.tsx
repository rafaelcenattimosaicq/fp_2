/* eslint-disable no-var */
import { useState, useCallback, useMemo } from 'react';
import { useHistory } from '../../contexts/HistoryContext';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { EChart } from '../EChart/EChart';
import type { ECOption } from '../EChart/echarts-setup';
import type { HistoryQueryParams } from '../../types';
import { PARAM_META, getParamMeta } from '../../types';
// chart controls removed during refactor
import styles from './HistoryExplorer.module.css';

// derive chartable param list from PARAM_META so adding a new sensor
// param only requires editing types.ts
const CHARTABLE = Object.values(PARAM_META).map(m => ({
  key: m.key.toLowerCase(),
  param: m.key,
}));

const META_COLS: {key: string; label: string}[] = [
  { key: 'device_id', label: 'Device' },
  {key: 'state', label: 'State'},
  { key: 'ingest_ts', label: 'Time' },
];

const ALL_COLS = [
  ...META_COLS,
  ...Object.values(PARAM_META).map(m => ({ key: m.key.toLowerCase(), label: m.label })),
];

const CHARTABLE_SET = new Set(CHARTABLE.map(c => c.key));

// limit rows because the table DOM
var ROW_LIMIT = 5000;

export function HistoryExplorer(): React.JSX.Element {
  const {status, rows, error, submitQuery, clearResults} = useHistory();
  const { devices } = useTelemetry();

  // chartInst removed — was used by useChartControls

  // yesterday–today as default range, most common query pattern
  const [startDate, setStartDate] = useState(() => {
    var d = new Date()
    d.setDate(d.getDate() - 1);
    return d.toISOString().slice(0, 10);
  });
  const [endDate, setEndDate] = useState(() => new Date().toISOString().slice(0, 10));

  const [selDevices, setSelDevices] = useState<string[]>([]);
  const [selCols, setSelCols] = useState<string[]>([]);
  const [showTable, setShowTable] = useState(false);

  const toggleDev = useCallback((id: string) => {
    setSelDevices(prev =>
      prev.includes(id) ? prev.filter(d => d !== id) : [...prev, id]
    );
  }, []);

  const toggleCol = useCallback((col: string) => {
    setSelCols(prev => {
      if(prev.includes(col)){
        return prev.filter(c => c !== col);
      }
      return [...prev, col]
    });
  }, []);

  const doQuery = useCallback(() => {
    // console.log('doQuery', startDate, endDate, selDevices, selCols);
    var params: HistoryQueryParams = {
      date: startDate,
      endDate: endDate,
      deviceIds: selDevices,
      columns: selCols.length > 0 ? selCols : [],
      limit: ROW_LIMIT,
    };
    submitQuery(params);
  }, [startDate, endDate, selDevices, selCols, submitQuery]);

  var deviceIds = useMemo(() => devices.map(d => d.id), [devices]);

  // figure out table headers from actual data or fall back to selection
  var headers = useMemo<string[]>(() => {
    if(rows.length > 0) return Object.keys(rows[0]);
    if(selCols.length > 0) return selCols;
    return ['device_id', 'temperature', 'ingest_ts'];
  }, [rows, selCols]);

  // which params can we actually chart (exist in the returned data)
  var chartParams = useMemo(() => {
    if(rows.length === 0) return [];

    var available = CHARTABLE.filter(c => rows[0][c.key] !== undefined);
    var wanted = selCols.filter(k => CHARTABLE_SET.has(k));

    if(wanted.length > 0) {
      var wantSet = new Set(wanted);
      return available.filter(c => wantSet.has(c.key));
    }
    return available;
  }, [rows, selCols]);

  // --- chart option -------------------------------------------------

  var chartOpt = useMemo<ECOption | null>(() => {
    if(rows.length === 0 || chartParams.length === 0) return null;

    var sorted = [...rows].sort((a, b) =>
      String(a.ingest_ts ?? '').localeCompare(String(b.ingest_ts ?? ''))
    );

    // extract HH:MM from timestamp for x axis labels
    var xData = sorted.map(r => {
      var ts = String(r.ingest_ts ?? '');
      var m = ts.match(/T(\d{2}:\d{2})/);
      if(m) return m[1];
      return ts.slice(11, 16) || ts  // fallback for non-ISO formats
    });

    var hasTwo = chartParams.length >= 2;

    // one y axis per param 
    var yAxis = chartParams.map((p, i) => {
      var meta = getParamMeta(p.param);
      var visible = i < 2;
      return {
        type: 'value' as const,
        name: visible ? meta.unit : undefined,
        nameTextStyle: {color: meta.color, fontSize: 10},
        position: (i === 0 ? 'left' : 'right') as 'left'|'right',
        scale: true,
        axisLine: { show: visible, lineStyle: { color: meta.color } },
        axisLabel: { show: visible, color: meta.color, fontSize: 10 },
        splitLine: { show: i === 0, lineStyle: { color: '#e8e8e8' } },
      };
    });

    var series = chartParams.map((p, i) => {
      var meta = getParamMeta(p.param);
      return {
        name: `${meta.label} (${meta.unit})`,
        type: 'line' as const,
        yAxisIndex: i,
        data: sorted.map(row => {
          var v = row[p.key];
          if(typeof v === 'number') return v;
          var n = Number(v)
          return n ? n : null;   // NaN and 0 both become null... 0 is wrong but rare
        }),
        smooth: true,
        symbol: 'none',
        lineStyle: { width: 1.5 },
        itemStyle: {color: meta.color},
      };
    });

    return {
      tooltip: {
        trigger: 'axis',
        backgroundColor: '#ffffff',
        borderColor: '#3e3e42',
        textStyle: { color: '#e8e8e8', fontSize: 11 },
      },
      legend: {
        top: 4,
        right: hasTwo ? 56 : 8,
        textStyle: { color: '#808080', fontSize: 11 },
        itemWidth: 14,  itemHeight: 8,
      },
      grid: {
        top: 36, left: 56,
        right: hasTwo ? 56 : 24,
        bottom: 36,
      },
      xAxis: {
        type: 'category',
        data: xData,
        axisLine: { lineStyle: { color: '#3e3e42' } },
        axisLabel: {color: '#808080', fontSize: 10},
      },
      yAxis,
      dataZoom: [{ type: 'inside', start: 0, end: 100 }],
      series,
    } as ECOption;
  }, [rows, chartParams]);

  var isLoading = status === 'loading';

  // --- render -------------------------------------------------------

  return (
    <div className={styles.container}>
      <div className={styles.accent} />

      {/* toolbar: date range + query button */}
      <div className={styles.toolbar}>
        <div className={styles.toolbarGroup}>
          <span className={styles.toolbarLabel}>From</span>
          <input
            type="date"
            className={`${styles.toolbarInput} ${styles.dateInput}`}
            value={startDate}
            onChange={e => setStartDate(e.target.value)}
            disabled={isLoading}
          />
          <span className={styles.toolbarLabel}>To</span>
          <input
            type="date"
            className={`${styles.toolbarInput} ${styles.dateInput}`}
            value={endDate}
            onChange={e => setEndDate(e.target.value)}
            disabled={isLoading}
          />
        </div>
        <div className={styles.spacer} />
        {status !== 'idle' && (
          <button type="button" className={styles.clearBtn} onClick={clearResults} disabled={isLoading}>
            Clear
          </button>
        )}
        <button
          type="button"
          className={styles.queryBtn}
          onClick={doQuery}
          disabled={isLoading || !startDate}
        >
          {isLoading ? 'Querying...' : 'Query'}
        </button>
      </div>

      {deviceIds.length > 0 && (
        <div className={styles.deviceBar}>
          <span className={styles.toolbarLabel}>Devices</span>
          <div className={styles.deviceChips}>
            {deviceIds.map(id => (
              <button
                key={id}
                type="button"
                className={`${styles.chip} ${selDevices.includes(id) ? styles.chipActive : ''}`}
                onClick={() => toggleDev(id)}
                disabled={isLoading}
              >
                {id}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className={styles.columnBar}>
        <span className={styles.columnBarLabel}>Columns</span>
        {ALL_COLS.map(col => (
          <button
            key={col.key}
            type="button"
            className={`${styles.colToggle} ${selCols.includes(col.key) ? styles.colToggleActive : ''}`}
            onClick={() => toggleCol(col.key)}
            disabled={isLoading}
          >
            {col.label}
          </button>
        ))}
      </div>

      {status === 'loading' && (
        <div className={`${styles.statusBar} ${styles.statusLoading}`}>Querying...</div>
      )}
      {status === 'failed' && error && (
        <div className={`${styles.statusBar} ${styles.statusError}`}>{error}</div>
      )}

      {status === 'succeeded' && chartOpt && (
        <div className={styles.chartWrapper} style={{position: 'relative'}}>
          <EChart
            option={chartOpt}
            style={{ height: '100%', width: '100%' }}
            onChartReady={() => {}}
          />
          {/* ChartControls removed during refactor */}
        </div>
      )}

      {status === 'succeeded' && (
        <div className={styles.infoBar}>
          <span className={styles.rowCount}>
            {rows.length} row{rows.length !== 1 ? 's' : ''}
            {rows.length >= ROW_LIMIT && <> (capped at {ROW_LIMIT})</>}
          </span>
          <button
            type="button"
            className={styles.tableToggle}
            onClick={() => setShowTable(prev => !prev)}
          >
            {showTable ? 'Hide table' : 'Show table'}
          </button>
        </div>
      )}

      {status === 'succeeded' && showTable && rows.length > 0 && (
        <div className={styles.tableWrapper}>
          <table className={styles.table}>
            <thead>
              <tr>
                {headers.map(col => <th key={col}>{col}</th>)}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, idx) => (
                <tr key={idx}>
                  {headers.map(col => <td key={col}>{String(row[col] ?? '')}</td>)}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {status === 'succeeded' && rows.length === 0 && (
        <div className={styles.empty}>No data found for the selected date range.</div>
      )}
      {status === 'idle' && (
        <div className={styles.empty}>Select a date range and click Query to explore historical telemetry.</div>
      )}
    </div>
  );
}