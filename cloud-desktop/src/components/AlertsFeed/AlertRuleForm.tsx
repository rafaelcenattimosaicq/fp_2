import { useState } from 'react';
import { useQuery } from '../../contexts/QueryContext';
import { useAlerts } from '../../contexts/AlertsContext';
import styles from './AlertRuleForm.module.css';

const OPS = ['>', '<', '>=', '<=', '=', '!='];
// these fields come from the NES schema and shouldn't be selectable as alert targets
const META = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp']);

export function AlertRuleForm(): React.JSX.Element {
    const { sources } = useQuery();
    const { addRule } = useAlerts();

    const [sel, setSel] = useState('');
    const [field, setField] = useState('');
    const [op, setOp] = useState('>');
    const [threshold, setThreshold] = useState('');
    const [actionReg, setActionReg] = useState('');
    const [actionVal, setActionVal] = useState('');
    const [actionGw, setActionGw] = useState('GW-EDGE-001');
    const [showAction, setShowAction] = useState(false);
    const [submitting, setSubmitting] = useState(false);

    const matched = sources.find((s) => s.name === sel);
    // writable registers start with PARAM_ (read-only ones are STATUS_ID_*)
    const writableRegs = matched
        ? matched.fields.filter(f => f.startsWith('PARAM_'))
        : [];
    // also add common writable registers that might not be in the schema
    // (the gateway descriptor has them even if NES source doesn't)
    const EXTRA_WRITABLE = ['PARAM_MOTOR_COMMAND', 'PARAM_MOTOR_SPEED', 'PARAM_TH_SETPOINT', 'PARAM_FAN_SPEED'];
    const allWritable = [...new Set([...writableRegs, ...EXTRA_WRITABLE])];

    function handleSourceChange(x: string){
        setSel(x);
        setField('');
    }

    const canSubmit = sel !== '' && field !== '' && threshold.trim() !== '' && !submitting;

    async function doSubmit(){
        if (!canSubmit) return;

        // only validate numeric for non equality ops
        if(op !== '=' && op !== '!='){
            if(Number.isNaN(Number(threshold.trim()))) return;
        }

        setSubmitting(true);
        try {
            // console.log('submitting rule:', sel, field, op, threshold);
            await addRule(sel, field, op, threshold.trim(), {
                actionRegisterId: actionReg || undefined,
                actionValue: actionVal ? Number(actionVal) : undefined,
                actionGatewayId: actionGw || undefined,
            });
            setField('');
            setThreshold('');
            setActionReg('');
            setActionVal('');
        } catch {
        }
        setSubmitting(false);
    }

    return (
      <div className={styles.form}>
        <div className={styles.row}>
            <select
                className={styles.select}
                value={sel}
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
              disabled={!sel}
              aria-label="Alert field"
            >
              <option value="">Field...</option>
              {(matched ? matched.fields.filter((f) => !META.has(f)) : []).map((f) => <option key={f} value={f}>{f}</option>)}
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
                value={threshold}
                onChange={(e) => setThreshold(e.target.value)}
                onKeyDown={(e) => { if(e.key === 'Enter') doSubmit(); }}
                aria-label="Threshold value"
            />

            <button
              type="button"
              className={styles.actionToggle}
              onClick={() => setShowAction(!showAction)}
              title="Add auto-action"
            >
              ⚡
            </button>
            <button
              type="button"
              className={styles.addBtn}
              disabled={!canSubmit}
              onClick={doSubmit}
            >
              {submitting ? '...' : '+ Rule'}
            </button>
        </div>

        {showAction && (
          <div className={styles.actionRow}>
            <span className={styles.actionLabel}>Action:</span>
            <select
              className={styles.actionInput}
              value={actionReg}
              onChange={(e) => setActionReg(e.target.value)}
            >
              <option value="">Register...</option>
              {allWritable.map(r => <option key={r} value={r}>{r}</option>)}
            </select>
            <input
                className={styles.actionInput}
                type="number"
                placeholder="Value (e.g. 3)"
                value={actionVal}
                onChange={(e) => setActionVal(e.target.value)}
                style={{width: 80}}
            />
            <input
              className={styles.actionInput}
              type="text"
              placeholder="Gateway ID"
              value={actionGw}
              onChange={(e) => setActionGw(e.target.value)}
              style={{ width: 120 }}
            />
          </div>
        )}
      </div>
    );
}
