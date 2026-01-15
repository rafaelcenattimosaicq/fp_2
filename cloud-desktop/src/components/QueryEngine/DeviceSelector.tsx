/* eslint-disable no-var */
import { useState, useRef, useEffect, useCallback,
  // useMemo,
} from 'react';
import type { Device } from '../../types';
import styles from './DeviceSelector.module.css';

interface DeviceSelectorProps {
  devices: Device[];
  selectedIds: string[];
  onSelectionChange: (ids: string[]) => void;
}

// multi-select dropdown.
export function DeviceSelector({
  devices, selectedIds, onSelectionChange,
}: DeviceSelectorProps): React.JSX.Element {
  var [open, setOpen] = useState(false)
  var [q, setQ] = useState('')
  var wRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if(!open) return
    var fn = (e: MouseEvent) => {
      if(wRef.current && !wRef.current.contains(e.target as Node))
        setOpen(false)
    };
    document.addEventListener('mousedown', fn)
    return () => document.removeEventListener('mousedown', fn)
  }, [open])

  var toggle = useCallback((id: string) => {
    onSelectionChange(
      selectedIds.includes(id)
        ? selectedIds.filter(x => x !== id)
        : [...selectedIds, id])
  }, [selectedIds, onSelectionChange])

  var shown = !q ? devices : devices.filter(d =>
    d.id.toLowerCase().includes(q.toLowerCase())
    || d.name.toLowerCase().includes(q.toLowerCase()))

  var n = selectedIds.length
  var lbl = n == 0 ? 'Select devices'
    : n == 1  ? '1 device selected'
    :           n + ' devices selected'

  return <div className={styles.wrapper} ref={wRef}>
      <button type="button" className={styles.trigger}
        onClick={() =>  setOpen(v => !v)} aria-label={lbl}>
        <span>{lbl}</span>
        <span className={`${styles.chevron} ${open ? styles.chevronOpen : ''}`}
         aria-hidden="true">&#9660;</span>
      </button>

      {open && <div className={styles.dropdown}>
          <input className={styles.search} type="text"
            placeholder="Search devices..." value={q}
            onChange={e => setQ(e.target.value)}
            autoFocus
            />

          <div className={styles.actions}>
            <button type="button" className={styles.actionBtn}
              onClick={() => onSelectionChange(devices.map(d => d.id))}>All</button>
            <button type="button" className={styles.actionBtn}
              onClick={() =>  onSelectionChange([])}>None</button>
          </div>

          <div className={styles.list}>{shown.map(d => <label key={d.id} className={styles.row}>
                  <input type="checkbox" className={styles.checkbox}
                    checked={selectedIds.includes(d.id)}
                    onChange={() => toggle(d.id)} />
                  <span className={
                    `${styles.statusDot} ${d.online ? styles.statusDotOnline : styles.statusDotOffline}`}
                    title={d.online ? 'online' : 'offline'} />
                  <span className={styles.deviceId}>{d.id}</span>
                  {d.name !== d.id &&
                    <span className={styles.deviceName}>{d.name}</span>}
            </label>)}
            {shown.length < 1 && <div className={styles.empty}>No devices match</div>}
          </div>
      </div>}
  </div>
}