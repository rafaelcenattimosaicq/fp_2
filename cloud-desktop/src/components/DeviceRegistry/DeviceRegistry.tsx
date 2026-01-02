/* eslint-disable no-var */
import { useState, useRef } from 'react';
import type { DeviceRecord } from '../../types';
import styles from './DeviceRegistry.module.css';

// opc-ua was added
// "canopen" is next if the Festo deal closes
const PROTOCOLS = ['modbus', 'mqtt', 'opc-ua'] as const;

// online guidelines say 128x128 PNG 
const MAX_ICON_KB = 64 * 1024;

interface Props {
  devices: DeviceRecord[];
  onAdd: (deviceId: string, protocol: string, icon?: string) => void;
  onDelete: (deviceId: string) => void;
  loading: boolean;
}

// manual base64 because btoa chokes on binary strings
async function toB64(file: File): Promise<string> {
  var buf = await file.arrayBuffer()
  let str = '';
  for(const b of new Uint8Array(buf)) str += String.fromCharCode(b);
  return btoa(str);
}

export function DeviceRegistry({ devices, onAdd, onDelete, loading }: Props): React.JSX.Element {
  const [expanded, setExpanded] = useState(true);
  const [newId, setNewId] = useState('');
  const [proto, setProto] = useState<string>(PROTOCOLS[0]);
  const [iconB64, setIconB64] = useState<string|undefined>(undefined);
  const fileRef = useRef<HTMLInputElement>(null);

  // console.log('DeviceRegistry render, devices:', devices.length);

  function doAdd() {
    if(!newId.trim()) return;
    onAdd(newId.trim(), proto, iconB64);
    setNewId('');
    setProto(PROTOCOLS[0]);
    setIconB64(undefined);
  }

  function onIconPick(e: React.ChangeEvent<HTMLInputElement>) {
    var f = e.target.files?.[0];
    if(!f) return;
    if(f.size > MAX_ICON_KB){
      console.warn(
        `Icon too large (${f.size} bytes), max is ${MAX_ICON_KB}. ` +
        `client guidelines say 128x128 PNG should be < 20KB.`
      );
      e.target.value = '';
      return
    }
    toB64(f)
      .then(res => setIconB64(res))
      .catch(() => {});    //.   silently fail, user can retry
    e.target.value = '';  // reset so same file re-triggers onChange
  }

  return (
    <div className={styles.container}>
      <button
        type="button"
        className={styles.header}
        onClick={() => setExpanded(v => !v)}
        aria-expanded={expanded}
      >
        <span className={styles.chevron} data-expanded={expanded}>
          {'\u25B6'}
        </span>
        <span className={styles.title}>Registered Devices</span>
        <span className={styles.count}>{devices.length}</span>
      </button>

      {expanded && (
        <div className={styles.body}>
          {/* hidden file input triggered by icon button below */}
          <input
            ref={fileRef}
            type="file"
            accept="image/png,image/svg+xml,image/jpeg"
            className={styles.hiddenInput}
            onChange={onIconPick}
            aria-label="Upload device icon"
          />

          <div className={styles.addForm}>
            <input
              type="text"
              className={styles.input}
              placeholder="Device ID..."
              value={newId}
              onChange={e => setNewId(e.target.value)}
              onKeyDown={e => { if(e.key === 'Enter') doAdd() }}
              disabled={loading}
            />
            <select
              className={styles.select}
              value={proto}
              onChange={e => setProto(e.target.value)}
              disabled={loading}
              aria-label="Protocol"
            >
              {PROTOCOLS.map(p => (
                <option key={p} value={p}>{p}</option>
              ))}
            </select>
            <button
              type="button"
              className={styles.iconBtn}
              onClick={() => fileRef.current?.click()}
              disabled={loading}
              title="Upload device icon"
              aria-label="Upload device icon button"
            >
              {iconB64
                ? <img src={`data:image/png;base64,${iconB64}`} alt="icon preview" className={styles.iconPreview} />
                : '\uD83D\uDDBC'}
            </button>
            <button type="button" className={styles.addBtn} onClick={doAdd} disabled={loading || !newId.trim()}>
              Add
            </button>
          </div>

          <ul className={styles.list}>
            {devices.map(d => (
                <li key={d.device_id} className={styles.row}>
                  {d.icon ? (
                    <img
                      src={`data:image/png;base64,${d.icon}`}
                      alt={`${d.device_id} icon`}
                      className={styles.rowIcon}
                    />
                  ) : (

                    <span className={styles.iconPlaceholder}>
                      {d.device_id.charAt(0).toUpperCase()}
                    </span>
                  )}
                  <span className={styles.deviceId}>{d.device_id}</span>
                  <span className={styles.badge}>{d.protocol}</span>
                  <button
                    type="button"
                    className={styles.deleteBtn}
                    onClick={() => onDelete(d.device_id)}
                    disabled={loading}
                    aria-label={`Delete ${d.device_id}`}
                  >
                    {'\u00D7'}
                  </button>
                </li>
            ))}
            {devices.length === 0 && (
              <li className={styles.empty}>No devices registered.</li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}