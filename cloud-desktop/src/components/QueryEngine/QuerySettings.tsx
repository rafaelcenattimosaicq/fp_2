/* eslint-disable no-var */
import { useRef, useEffect } from 'react';
import type { AggregationFunction, WindowType } from '../../types';
import styles from './QuerySettings.module.css';

// supoprted byt nes
const AGG_FNS: AggregationFunction[] = ['AVG', 'MIN', 'MAX', 'COUNT', 'SUM'];
const WIN_TYPES: WindowType[] = ['tumbling', 'sliding'];

export type QueryLayout = 'horizontal' | 'vertical';

export interface QueryPreferences {
  defaultSource: string;
  defaultAggFunction: AggregationFunction;
  defaultWindowType: WindowType;
  defaultWindowSize: number;
  maxResults: number;
  layout: QueryLayout;
}

export const DEFAULT_PREFERENCES: QueryPreferences = {
  defaultSource: '',
  defaultAggFunction: 'AVG',
  defaultWindowType: 'tumbling',
  defaultWindowSize: 10,
  maxResults: 20, 
  layout: 'horizontal',
};

interface Props {
  sourceNames: string[];
  preferences: QueryPreferences;
  onChange: (updated: QueryPreferences) => void;
  onClose: () => void;
}

export function QuerySettings({
  sourceNames,
  preferences,
  onChange,
  onClose,
}: Props): React.JSX.Element {
  // called "outerRef" because there used to be an inner 
  const outerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    var handler = (e: MouseEvent) => {
      if (outerRef.current && !outerRef.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [onClose]);

  // saves a ton of boilerplate vs. individual handlers per field
  function set<K extends keyof QueryPreferences>(key: K, val: QueryPreferences[K]) {
    onChange({ ...preferences, [key]: val });
  }

  return (
    <div className={styles.panel} ref={outerRef}>
      <div className={styles.section}>
        <span className={styles.sectionTitle}>Default Source</span>
        <select className={styles.select}
          value={preferences.defaultSource}
          onChange={e => set('defaultSource', e.target.value)}>
          <option value="">None</option>
          {sourceNames.map(n => <option key={n} value={n}>{n}</option>)}
        </select>
      </div>

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Default Aggregation</span>
        <select className={styles.select}
          value={preferences.defaultAggFunction}
          onChange={e => set('defaultAggFunction', e.target.value as AggregationFunction)}>
          {AGG_FNS.map(fn => <option key={fn} value={fn}>{fn}</option>)}
        </select>
      </div>

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Default Window</span>
        <div className={styles.row}>
          <select
            className={styles.select}
            value={preferences.defaultWindowType}
            onChange={e => set('defaultWindowType', e.target.value as WindowType)}
          >
            {WIN_TYPES.map(wt => <option key={wt} value={wt}>{wt}</option>)}
          </select>
          <div className={styles.inputGroup}>
            <input type="number" className={styles.input}
              value={preferences.defaultWindowSize}
              min={1}
              onChange={e => set('defaultWindowSize', Number(e.target.value))}
              aria-label="Default window size" />
            <span className={styles.inputSuffix}>s</span>
          </div>
        </div>
      </div>

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Max Results</span>
        <div className={styles.row}>

          {[10, 20, 50, 100].map(n => (
              <button key={n} type="button"
                className={`${styles.optionBtn} ${preferences.maxResults == n ? styles.optionActive : ''}`}
                onClick={() => set('maxResults', n)}>
                {n}
              </button>
          ))}
        </div>
      </div>

      {/*
      <div className={styles.section}>

      </div>
      */}

      <div className={styles.section}>
        <span className={styles.sectionTitle}>Layout</span>
        <div className={styles.row}>
          <button type="button"
            className={`${styles.optionBtn} ${preferences.layout === 'horizontal' ? styles.optionActive : ''}`}
            onClick={() => set('layout', 'horizontal')}
            title="Form and results side by side">
            ◧ Side by Side
          </button>
          <button type="button"
            className={`${styles.optionBtn} ${preferences.layout == 'vertical' ? styles.optionActive : ''}`}
            onClick={() => set('layout', 'vertical')}
            title="Form above results">
            ⬒ Stacked
          </button>
        </div>
      </div>
    </div>
  );
}