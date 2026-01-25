import { useState, useRef, useEffect } from 'react'
import type { BlockType } from './types';
import { BLOCK_LABELS } from './types';
import styles from './AddBlockBtn.module.css';

interface Props {
  onAdd: (type: BlockType) => void;
}

// block types that can be added from the menu
const OPTS: BlockType[] = ['telemetry', 'alerts', 'queryEngine', 'history'];

export function AddBlockBtn({ onAdd }: Props): React.JSX.Element {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  // close dropdown when clicking outside
  useEffect(() => {
    if (!open) return;

    function handleClickOutside(e: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  const handleSelect = (t: BlockType) => {
    // console.log('selected block type:', t);
    onAdd(t)
    setOpen(false);
  }

  return (
    <div className={styles.wrapper} ref={wrapperRef}>
      {open && (
        <div className={styles.menu}>
          {OPTS.map(item => (
            <button
              key={item}
              type="button"
              className={styles.menuItem}
              onClick={() => handleSelect(item)}
            >
              {BLOCK_LABELS[item]}
            </button>
          ))}
        </div>
      )}

      <button
        type="button"
        className={`${styles.fab} ${open ? styles.fabOpen : ''}`}
        onClick={() => setOpen(prev => !prev)}
        aria-label="Add block"
      >
        <span className={styles.fabIcon}>+</span>
      </button>
    </div>
  );
}
