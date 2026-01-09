
import { useState, useCallback, useEffect, useMemo } from 'react';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { TimeSeriesChart } from './TimeSeriesChart';
import { TelemetrySettings } from './TelemetrySettings';
import { DEFAULT_PARAMS, getParamMeta } from '../../types';
import type { TelemetryPoint } from '../../types';
import styles from './TelemetryCharts.module.css';

const KNOWN_PARAMS: Set<string> = new Set(DEFAULT_PARAMS);
const EXCLUDED_KEYS = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'device_id']);
// 5 min default, matches the the client service dashboard
const DEFAULT_TIME_WIN = 300;

/**
 * Top-level telemetry view.  Shows a tab bar of parameters and a chart
 * for the currently selected one.  The settings gear lets users toggle
 * parameters, filter devices, and change the rolling time window.
 */
export function TelemetryCharts(): React.JSX.Element {
  const [activeTab, setActiveTab] = useState<string>('');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { buffers, devices } = useTelemetry();

  const allParams = useMemo(() => {
    const discovered = new Set<string>();
    for (const points of buffers.values()) {
      for (const p of points) {
        for (const k of Object.keys(p.values)) {
          if (!EXCLUDED_KEYS.has(k)) discovered.add(k);
        }
      }
    }
    const result: string[] = [...DEFAULT_PARAMS.filter(k => discovered.has(k))];
    for (const k of discovered) {
      if (!KNOWN_PARAMS.has(k)) result.push(k);
    }
    return result.length > 0 ? result : DEFAULT_PARAMS;
  }, [buffers]);

  const [hiddenParams, setHiddenParams] = useState<Set<string>>(() => new Set());
  const [selectedDeviceIds, setSelectedDeviceIds] = useState<Set<string>>(() => new Set());
  const [timeWindow, setTimeWindow] = useState(DEFAULT_TIME_WIN);

  const visibleParams = useMemo(() => {
    const vis = new Set<string>();
    for (const k of allParams) {
      if (!hiddenParams.has(k)) vis.add(k);
    }
    if (vis.size === 0 && allParams.length > 0) vis.add(allParams[0]);
    return vis;
  }, [allParams, hiddenParams]);

  const handleToggleParam = useCallback((param: string) => {
    setHiddenParams(prev => {
      const nxt = new Set(prev);
      if (nxt.has(param)) { nxt.delete(param); } else { nxt.add(param); }
      return nxt;
    });
  }, []);

  const handleToggleDevice = useCallback((deviceId: string) => {
    setSelectedDeviceIds(prev => {
      const nxt = new Set(prev);
      if (nxt.has(deviceId)) { nxt.delete(deviceId); } else { nxt.add(deviceId); }
      return nxt;
    });
  }, []);

  const visibleTabs = allParams.filter(p => visibleParams.has(p));
  const effectiveTab = (activeTab && visibleParams.has(activeTab))
    ? activeTab : visibleTabs[0] ?? allParams[0];
  const activeMeta = getParamMeta(effectiveTab);

  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const filteredBufs = useMemo(() => {
    const cutoff = now - timeWindow * 1000;
    const out = new Map<string, TelemetryPoint[]>();
    for (const [devId, pts] of buffers.entries()) {
      if (selectedDeviceIds.size > 0 && !selectedDeviceIds.has(devId)) continue;
      const trimmed = pts.filter(p => p.timestamp >= cutoff);
      if (trimmed.length > 0) out.set(devId, trimmed);
    }
    return out;
  }, [buffers, selectedDeviceIds, timeWindow, now]);

  return (
    <>
      <div className={styles.tabBar} role="tablist">
        {visibleTabs.map(key => (
          <button key={key} type="button" role="tab"
            aria-selected={key === effectiveTab}
            className={`${styles.tab} ${key === effectiveTab ? styles.active : ''}`}
            onClick={() => setActiveTab(key)}>
            {getParamMeta(key).label}
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
          {settingsOpen && (
            <TelemetrySettings
              allParams={allParams}
              visibleParams={visibleParams}
              onToggleParam={handleToggleParam}
              devices={devices}
              selectedDeviceIds={selectedDeviceIds}
              onToggleDevice={handleToggleDevice}
              timeWindow={timeWindow}
              onSetTimeWindow={setTimeWindow}
              onClose={() => setSettingsOpen(false)}
            />
          )}
        </div>
      </div>

      <div className={styles.chartArea}>
        <TimeSeriesChart
          param={activeMeta}
          buffers={filteredBufs}
          selectedDeviceId={null}
        />
      </div>
    </>
  );
}
