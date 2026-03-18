/* eslint-disable no-var */
import { useRef, useEffect } from 'react';
import { getParamMeta } from '../../types';
import type { Device } from '../../types';
import styles from './TelemetrySettings.module.css';

// TODO: custom time window input (free-text seconds)
const TIME_WINDOWS = [
  { label: 'Live', value: 60 },
  { label: '1 min', value: 60 },
  { label: '5 min', value: 300 },
  { label: '15 min', value: 900 },
  { label: '30 min', value: 1800 },
  { label: '1 hour', value: 3600 },
  { label: '6 hours', value: 21_600 },
  { label: '24 hours', value: 86_400 },
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

export function TelemetrySettings({
  allParams, visibleParams, onToggleParam,
  devices, selectedDeviceIds, onToggleDevice,
  timeWindow, onSetTimeWindow, onClose,
}: Props): React.JSX.Element {

  const panelRef = useRef<HTMLDivElement>(null);


  useEffect(() => {
    // eslint-disable-next-line no-var
    var handler = (e: MouseEvent) => {
      if(panelRef.current && !panelRef.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [onClose]);

  return (
    <div className={styles.panel} ref={panelRef}>

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Parameters</span>
        <div className={styles.checkboxList}>
          {allParams.map((k) => {
            const meta = getParamMeta(k)
            return (
              <label key={k} className={styles.checkboxRow}>
                <input
                  type="checkbox"
                  checked={visibleParams.has(k)}
                  onChange={() => onToggleParam(k)}
                  className={styles.checkbox}
                />
                <span className={styles.paramDot} style={{ background: meta.color }} />
                <span className={styles.checkboxLabel}>{meta.label}</span>
              </label>
            );
          })}
        </div>
      </div>

      {devices.length > 0 && <div className={styles.section}>
          <span className={styles.sectionTitle}>Devices</span>
          <div className={styles.checkboxList}>{devices.map(dev => {
              // "selected" when explicitly picked OR when nothing is picked (= all visible)
              var isOn = !selectedDeviceIds.size || selectedDeviceIds.has(dev.id);
              return <label key={dev.id} className={styles.checkboxRow}>
                <input type="checkbox" checked={isOn}
                  onChange={() => onToggleDevice(dev.id)} className={styles.checkbox} />
                <span className={`${styles.statusDot} ${dev.online ? styles.online : styles.offline}`} />
                <span className={styles.checkboxLabel}>{dev.name}</span>
              </label>
          })}</div>
          {!selectedDeviceIds.size && <span className={styles.hint}>All devices shown</span>}
      </div>}

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Time Window</span>
        <div className={styles.windowRow}>
          {TIME_WINDOWS.map((w, idx) =>
            <button key={idx} type="button"
              className={`${styles.windowBtn} ${timeWindow == w.value ? styles.windowActive : ''}`}
              onClick={() => onSetTimeWindow(w.value)}>{w.label}</button>
          )}
        </div>
      </div>

    </div>
  );
}