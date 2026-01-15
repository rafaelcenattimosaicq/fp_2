/* eslint-disable no-var */
import type { QueryFilter, FilterOperator } from '../../types';
import styles from './FilterRow.module.css';

// this is what nes suipports
var OPS: FilterOperator[] = ['=','!=','>','<','>=','<='];

interface FilterRowProps {
  filter: QueryFilter;
  fields: string[];
  onChange: (updated: QueryFilter) => void;
  onRemove: () => void;
}

export function FilterRow({ filter, fields, onChange, onRemove }: FilterRowProps): React.JSX.Element {
  // console.log('FilterRow render, field =', filter.field);
  var set = (patch: Partial<QueryFilter>) => onChange({...filter, ...patch})

  return <div className={styles.row}>
      <select className={styles.select} value={filter.field}
        onChange={e => set({field: e.target.value})}>
        <option value="">Field</option>
        {fields.map(f => <option key={f} value={f}>{f}</option>)}
      </select>

      <select className={styles.select} value={filter.operator}
        onChange={e => set({operator: e.target.value as FilterOperator})}>
        {OPS.map(op => <option key={op} value={op}>{op}</option>)}
      </select>

      <input className={styles.input} type="text" value={filter.value}
        onChange={e => set({value: e.target.value})}
        placeholder="Value" />

      <button type="button" className={styles.remove}
        onClick={onRemove}>×</button>
  </div>
}