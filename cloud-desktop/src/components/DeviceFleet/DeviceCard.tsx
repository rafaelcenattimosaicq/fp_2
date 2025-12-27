import type { Device } from '../../types';
import styles from './DeviceCard.module.css';

// formats the "last seen" timestamp into a readable string
function formatLastSeen(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000);

  if(diff < 5) return 'just now';
  if(diff < 60) return `${diff}s ago`;

  // had to think about whether to floor or ceil here, floor makes more sense
  const mins = Math.floor(diff / 60)
  if(mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs >= 24) {
    const d = new Date(ts)
    return `${d.getDate().toString().padStart(2, '0')}/${(d.getMonth()+1).toString().padStart(2, '0')}/${d.getFullYear()}`;
  }

  return `${hrs}h ago`;
}

interface DeviceCardProps {
  device: Device;
  selected: boolean;
  onSelect: (id: string) => void;
}

export function DeviceCard({ device, selected, onSelect }: DeviceCardProps): React.JSX.Element {
  // console.log('rendering card for', device.id, device.online);
  return (
    <button
      type="button"
      className={styles.card + (selected ? ` ${styles.selected}` : "")}
      onClick={() => onSelect(device.id)}
    >
      <span
        className={styles.dot}
        data-online={device.online}
        aria-label={device.online ? "Online" : "Offline"}
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
