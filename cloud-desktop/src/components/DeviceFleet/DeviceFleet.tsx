import { useCallback, useMemo } from 'react';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { DeviceCard } from './DeviceCard';
import styles from './DeviceFleet.module.css';

// TODO: add sorting options (by name, by status, etc)
export function DeviceFleet(): React.JSX.Element {
  const { devices, selectedDeviceId, selectDevice } = useTelemetry();

  // console.log('DeviceFleet render, count:', devices.length);

  const handleSelect = useCallback((val: string) => {
    selectDevice(selectedDeviceId === val ? null : val);
  }, [selectedDeviceId, selectDevice]);

  const cards = useMemo(() => {
    return devices.map(device => (
      <DeviceCard
        key={device.id}
        device={device}
        selected={device.id === selectedDeviceId}
        onSelect={handleSelect}
        />
    ))
  }, [devices, selectedDeviceId, handleSelect]);

  return (
    <div className="panel">
      <div className="panel__header">
        <span className="panel__title">Device Fleet</span>
        <span className={styles.count}>
          {devices.length + " device" + (devices.length !== 1 ? "s" : "")}
        </span>
      </div>
      <div className={"panel__body" + " " + styles.grid}>
        {devices.length === 0
          ? <p className={styles.empty}>Waiting for devices...</p>
          : <>{cards}</>}
      </div>
    </div>
  );
}
