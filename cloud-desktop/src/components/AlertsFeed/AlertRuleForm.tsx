import { useState } from 'react';
import { useQuery } from '../../contexts/QueryContext';
import { useAlerts } from '../../contexts/AlertsContext';
import styles from './AlertRuleForm.module.css';

const OPS = ['>', '<', '>=', '<=', '=', '!='];

const META = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp']);

export function AlertRuleForm(): React.JSX.Element {
    const { sources } = useQuery();
    const { addRule } = useAlerts();

    const [src, setSrc] = useState('');
    const [field, setField] = useState('');
    const [op, setOp] = useState('>');
    const [thresh, setThresh] = useState('');
    // tracks whether an addRule() call is in-flight so we can disable
    const [submitting, setSubmitting] = useState(false);

    const selSrc = sources.find((s) => s.name === src);
    const flds = selSrc
      ? selSrc.fields.filter((f) => !META.has(f))
      : [];

    function handleSourceChange(val: string) {
        setSrc(val);
        setField('');  // reset, fields differ per source
    }

    const trimmed = thresh.trim();
    const canSubmit = src && field && trimmed !== '' && !submitting;

    // rule just shows "ERR" badge which confused some
    async function handleSubmit() {
        if (!canSubmit) return;
        if (op !== '=' && op !== '!=' && Number.isNaN(Number(trimmed))) return;

        setSubmitting(true);
        try {
            await addRule(src, field, op, trimmed);
            setField('');
            setThresh('');
        } catch { /* addRule already logs */ }
        setSubmitting(false);
    }

    return (
      <div className={styles.form}>
        <div className={styles.row}>
            <select
                className={styles.select}
                value={src}
                onChange={(e) => handleSourceChange(e.target.value)}
                aria-label="Alert source"
            >
                <option value="">Source...</option>
                {sources.map((s) => (
                  <option key={s.name} value={s.name}>{s.name}</option>
                ))}
            </select>

            <select
              className={styles.select}
              value={field}
              onChange={(e) => setField(e.target.value)}
              disabled={!src}
              aria-label="Alert field"
            >
              <option value="">Field...</option>
              {flds.map((f) => <option key={f} value={f}>{f}</option>)}
            </select>
        </div>

        <div className={styles.row}>
            <select
              className={styles.operatorSelect}
              value={op}
              onChange={(e) => setOp(e.target.value)}
              aria-label="Comparison operator"
            >
              {OPS.map((o) => <option key={o} value={o}>{o}</option>)}
            </select>

            <input
                className={styles.input}
                type="text"
                placeholder="Threshold"
                value={thresh}
                onChange={(e) => setThresh(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') handleSubmit(); }}
                aria-label="Threshold value"
            />

            <button
              type="button"
              className={styles.addBtn}
              disabled={!canSubmit}
              onClick={handleSubmit}
            >
              {submitting ? '...' : '+ Rule'}
            </button>
        </div>
      </div>
    );
}
