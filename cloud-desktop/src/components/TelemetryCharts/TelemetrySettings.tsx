
import { useRef, useEffect } from 'react';
import { getParamMeta } from '../../types';
import type { Device } from '../../types';
import styles from './TelemetrySettings.module.css';

// preset windows, the 30 min option covers a full the client defrost + recovery
const TIME_WINDOWS = [
  { label: '1 min', value: 60 },
  { label: '5 min', value: 300 },
  { label: '15 min', value: 900 },
  { label: '30 min', value: 1800 },
] as const;

interface Props {
  allParams: string[];
  visibleParams: Set<string>;
  onToggleParam: (param: string) => void;
  devices: Device[];
  selectedDeviceIds: Set<string>;
  onToggleDevice: (deviceId: string) => void;
  timeWindow: number;
  onSetTimeWindow: (seconds: number) => void;
  onClose: () => void;
}

/* Dropdown settings panel for TelemetryCharts.
   Closes on outside click. */
export function TelemetrySettings({
  allParams, visibleParams, onToggleParam,
  devices, selectedDeviceIds, onToggleDevice,
  timeWindow, onSetTimeWindow, onClose,
}: Props): React.JSX.Element {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClick(e: MouseEvent): void {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) onClose();
    }
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [onClose]);

  return (
    <div className={styles.panel} ref={panelRef}>
      {/* Parameter visibility toggles */}
      <div className={styles.section}>
        <span className={styles.sectionTitle}>Parameters</span>
        <div className={styles.checkboxList}>
          {allParams.map(key => (
            <label key={key} className={styles.checkboxRow}>
              <input type="checkbox" checked={visibleParams.has(key)}
                onChange={() => onToggleParam(key)} className={styles.checkbox} />
              <span className={styles.paramDot}
                style={{ background: getParamMeta(key).color }} />
              <span className={styles.checkboxLabel}>{getParamMeta(key).label}</span>
            </label>
          ))}
        </div>
      </div>

      {devices.length > 0 && (
        <div className={styles.section}>
          <span className={styles.sectionTitle}>Devices</span>
          <div className={styles.checkboxList}>
            {devices.map(d => (
              <label key={d.id} className={styles.checkboxRow}>
                <input type="checkbox"
                  checked={selectedDeviceIds.size === 0 || selectedDeviceIds.has(d.id)}
                  onChange={() => onToggleDevice(d.id)} className={styles.checkbox} />
                <span className={`${styles.statusDot} ${d.online ? styles.online : styles.offline}`} />
                <span className={styles.checkboxLabel}>{d.name}</span>
              </label>
            ))}
          </div>
          {selectedDeviceIds.size === 0 && (
            <span className={styles.hint}>All devices shown</span>
          )}
        </div>
      )}

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Time Window</span>
        <div className={styles.windowRow}>
          {TIME_WINDOWS.map(opt => (
            <button key={opt.value} type="button"
              className={`${styles.windowBtn} ${timeWindow === opt.value ? styles.windowActive : ''}`}
              onClick={() => onSetTimeWindow(opt.value)}>
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
