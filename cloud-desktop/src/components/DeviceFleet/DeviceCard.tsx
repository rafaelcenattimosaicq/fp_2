import type { Device } from '../../types';
import styles from './DeviceCard.module.css';

/** Props for a single tile in the fleet grid. */
interface DeviceCardProps {
  device: Device;
  selected: boolean;
  onSelect: (id: string) => void;
}

/*
 * Relative-time formatter for "last seen". Keep it short - the card is
 * only ~160px wide so anything longer than "12h ago" overflows the badge.
 * Past 24h we fall back to locale date because "1437m ago" is useless.
 */
function formatLastSeen(ts: number): string {
  const secs = Math.floor((Date.now() - ts) / 1000);
  if (secs < 5) return 'just now';
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs >= 24) return new Date(ts).toLocaleDateString();
  return `${hrs}h ago`;
}

export function DeviceCard({ device, selected, onSelect }: DeviceCardProps): React.JSX.Element {
  const cls = styles.card + (selected ? ` ${styles.selected}` : '');

  // compressor device IDs are hex (read from Modbus register 60000),
  return (
    <button
      type="button"
      className={cls}
      onClick={() => onSelect(device.id)}
    >
      <span
        className={styles.dot}
        data-online={device.online}
        aria-label={device.online ? 'Online' : 'Offline'}
      />
      <div className={styles.info}>
        <span className={styles.name}>{device.name}</span>
        <span className={styles.lastSeen}>
          {formatLastSeen(device.lastSeen)}
        </span>
      </div>
    </button>
  );
}
