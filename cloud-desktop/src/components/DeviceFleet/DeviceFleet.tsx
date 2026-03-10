import { useCallback, useMemo } from 'react';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { DeviceCard } from './DeviceCard';
import styles from './DeviceFleet.module.css';

/*
 * Fleet overview panel - renders a card grid for every connected the client
 * compressor. The fleet at Metalfrio's SP warehouse alone has 500+ units
 * so we memoise the card list to avoid ~200ms layout thrash every poll.
 *
 * The grid uses CSS auto-fill so it collapses to a single column when the
 * sidebar is narrow (< 420px). Don't switch to flexbox - tried it, the
 * wrapping behaviour was worse on Safari.
 */

export function DeviceFleet(): React.JSX.Element {
  const { devices, selectedDeviceId, selectDevice } = useTelemetry();

  // toggle-select: tap same device again to deselect
  const handleSelect = useCallback((id: string) => {
    selectDevice(selectedDeviceId === id ? null : id);
  }, [selectedDeviceId, selectDevice]);

  const cards = useMemo(() => {
    const devs = devices;
    const out: React.JSX.Element[] = [];
    for (let idx = 0; idx < devs.length; idx++) {
      const d = devs[idx];
      out.push(
        <DeviceCard
          key={d.id}
          device={d}
          selected={d.id === selectedDeviceId}
          onSelect={handleSelect}
        />
      );
    }
    return out;
  }, [devices, selectedDeviceId, handleSelect]);

  const cnt = devices.length;

  return (
    <div className="panel">
      <div className="panel__header">
        <span className="panel__title">Device Fleet</span>
        <span className={styles.count}>
          {cnt} device{cnt !== 1 ? 's' : ''}
        </span>
      </div>
      <div className={`panel__body ${styles.grid}`}>
        {cnt === 0
          ? <p className={styles.empty}>Waiting for devices...</p>
          : cards
        }
      </div>
    </div>
  );
}
