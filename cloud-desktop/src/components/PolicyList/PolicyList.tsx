import { useState, useRef } from 'react';
import type { PolicySummary } from '../../types';
import styles from './PolicyList.module.css';
import { formatISO } from '../../utils/formatDate';

interface Props {
    policies: PolicySummary[];
    selectedName: string | null;
    onSelect: (name: string) => void;
    onNew: () => void;
    onDelete: (name: string) => void;
}

function fmtDate(raw: string): string {
    try {
      return formatISO(raw);
    } catch {
      return raw;
    }
}

export function PolicyList(props: Props): React.JSX.Element {
    const [search, setSearch] = useState('');

    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    function handleSearch(val: string) {
        setSearch(val);
        if(debounceRef.current) clearTimeout(debounceRef.current);
    }

    const filtered = props.policies.filter(
      p => p.name.toLowerCase().includes(search.toLowerCase())
    );

    return (
      <div className={styles.container}>
        <div className={styles.toolbar}>
          <input
            type="text"
            className={styles.search}
            placeholder="Search policies..."
            value={search}
            onChange={e => handleSearch(e.target.value)}
          />
          <div className={styles.actions}>
              <button type="button" className={styles.newBtn} onClick={props.onNew}>
                  New Policy
              </button>
              <button
                type="button"
                className={styles.deleteBtn}
                disabled={props.selectedName == null}
                onClick={() => { if (props.selectedName) props.onDelete(props.selectedName); }}
              >
                Delete
              </button>
          </div>
        </div>

        <ul className={styles.list}>
          {filtered.map(item => {
              const isSelected = item.name == props.selectedName;
              return (
                <li key={item.name}
                  className={styles.item}
                  data-selected={isSelected ? 'true' : 'false'}
                  onClick={() => props.onSelect(item.name)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        props.onSelect(item.name);
                    }
                  }}
                >
                  <span className={styles.name}>{item.name}</span>
                  <span className={styles.meta}>{fmtDate(item.lastModified)}</span>
                </li>
              );
          })}
          {filtered.length === 0 && (
              <li className={styles.empty}>No policies found.</li>
          )}
        </ul>
      </div>
    );
}
