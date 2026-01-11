/* eslint-disable prefer-const */
/* eslint-disable no-var */
/* eslint-disable @typescript-eslint/no-unused-expressions */
import { useState, useCallback, useEffect, useMemo } from 'react';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { TimeSeriesChart } from './TimeSeriesChart';
import { TelemetrySettings } from './TelemetrySettings';
import { DEFAULT_PARAMS, getParamMeta } from '../../types';
import type { TelemetryPoint } from '../../types';
import styles from './TelemetryCharts.module.css';

const SKIP_KEYS: Set<string> = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'device_id']);

// 5 min 
var DEFAULT_WINDOW = 300;

export function TelemetryCharts(): React.JSX.Element {
  const [activeTab, setActiveTab] = useState<string>('');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { buffers, devices } = useTelemetry();

  // build tab list from whatever params the devices are actually sending
  const paramTabs = useMemo(() => {
    var seen = new Set<string>();
    buffers.forEach(points => {
      for(const x of points){
        for (const k of Object.keys(x.values)){
          if(!SKIP_KEYS.has(k)) seen.add(k);
        }
      }
    });

    var known = new Set(DEFAULT_PARAMS);
    var ordered: string[] = [...DEFAULT_PARAMS.filter(k => seen.has(k))];
    seen.forEach(p => { if(!known.has(p)) ordered.push(p) });
    return ordered.length ? ordered : DEFAULT_PARAMS;
  }, [buffers]);

  const [hiddenParams, setHiddenParams] = useState<Set<string>>(() => new Set());
  const [deviceFilter, setDeviceFilter] = useState<Set<string>>(() => new Set());
  const [timeWindow, setTimeWindow] = useState(DEFAULT_WINDOW);

  // no useCallback here — this component rarely re-renders and the
  // settings panel that consumes this is behind a conditional anyway
  function toggleParam(p: string) {
    setHiddenParams(prev => {
      var next = new Set(prev);
      next.has(p) ? next.delete(p) : next.add(p);
      return next;
    });
  }

  const toggleDevice = useCallback((id: string) => {
    setDeviceFilter(prev => {
      const s = new Set(prev);
      s.has(id) ? s.delete(id) : s.add(id);
      return s;
    });
  }, []);

  // -- visible tabs (no useMemo, this is like 15 items max) --
  var visibleSet = new Set<string>();
  for (var i = 0; i < paramTabs.length; i++) {
      if (!hiddenParams.has(paramTabs[i])) visibleSet.add(paramTabs[i]);
  }
  if (!visibleSet.size && paramTabs.length) visibleSet.add(paramTabs[0]);

  var shownTabs = paramTabs.filter(p => visibleSet.has(p));

  let currentTab = (activeTab && visibleSet.has(activeTab))
    ? activeTab
    : shownTabs[0] ?? paramTabs[0];


  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    var t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  /*
   * Filter buffers by time window 
   */
  const filteredBuffers = useMemo(() => {
    var cutoff = now - timeWindow * 1000;
    var out = new Map<string, TelemetryPoint[]>();

    for (const [devId, points] of buffers.entries()) {
        if (deviceFilter.size && !deviceFilter.has(devId)) continue;

        var kept: TelemetryPoint[] = [];
        var j = 0;
        while (j < points.length) {
            if (points[j].timestamp >= cutoff) kept.push(points[j]);
            j++;
        }
        if (kept.length) out.set(devId, kept);
    }
    return out;
  }, [buffers, deviceFilter, timeWindow, now]);

  return (
    <>
      <div className={styles.tabBar} role="tablist">
        {shownTabs.map(t => (
            <button key={t} type="button" role="tab"
              aria-selected={t === currentTab}
              className={`${styles.tab} ${t === currentTab ? styles.active : ''}`}
              onClick={() => setActiveTab(t)}>
              {getParamMeta(t).label}
            </button>
        ))}

        <div className={styles.spacer} />

        <div className={styles.settingsWrapper}>
          <button type="button"
            className={`${styles.gearBtn} ${settingsOpen ? styles.gearActive : ''}`}
            onClick={() => setSettingsOpen(prev => !prev)}
            aria-label="Chart settings" title="Chart settings">
            ⚙
          </button>
          {settingsOpen && <TelemetrySettings allParams={paramTabs} visibleParams={visibleSet}
              onToggleParam={toggleParam} devices={devices} selectedDeviceIds={deviceFilter}
              onToggleDevice={toggleDevice} timeWindow={timeWindow}
              onSetTimeWindow={setTimeWindow} onClose={() => setSettingsOpen(false)} />}
        </div>
      </div>

      <div className={styles.chartArea}>
        <TimeSeriesChart
          param={getParamMeta(currentTab)}
          buffers={filteredBuffers}
          selectedDeviceId={null}
        />
      </div>
    </>
  );
}