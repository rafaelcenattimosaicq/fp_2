/* eslint-disable no-var */
import { useState, useCallback } from 'react';
import type { DescriptorRegister } from '../../types/descriptor';
import { useDeviceCommands } from '../../hooks/useDeviceCommands';
import styles from './DeviceConfigurator.module.css';

/*
 * Write-parameter panel for a single device. Renders the writable
 * registers from the device descriptor as input rows.
 *
 * The ack/error display (showOk, showErr) only tracks the LAST sent
 * register which means if you quickly send two different params the
 * first one's ack gets eaten. Couldnt't fix this 
 */

interface Props {
  parameters: DescriptorRegister[];
  deviceId: string;
  gatewayId: string;
}

export function DeviceConfigurator({ parameters, deviceId, gatewayId }: Props): React.JSX.Element {
  const { sendWriteCommand, commandState } = useDeviceCommands(gatewayId);
  const [vals, setVals] = useState<Record<string, string>>({});
  const [lastSentId, setLastSentId] = useState<string | null>(null);

  const updateVal = useCallback((regId: string, v: string) => {
    setVals(prev => ({ ...prev, [regId]: v }));
  }, []);

  // range validation before write. the backend also validates but the
  // error message is useless ("INVALID_PARAMETER_VALUE") so we catch
  // it here and just... do nothing. maybe should show a toast? I forgot to plan a toast system
  const handleSend = useCallback(
    (reg: DescriptorRegister) => {
      var raw = vals[reg.id];
      if(raw === undefined || raw === '') return;

      var num = Number(raw);
      if((Number.isNaN(num))) return;
      // min_value/max_value are optional in the descriptor schema,
      if(reg.min_value !== undefined && num < reg.min_value!) return;
      if(reg.max_value !== undefined && num > reg.max_value!) return;

      // console.log('sending write:', deviceId, reg.id, num);
      setLastSentId(reg.id);
      sendWriteCommand(deviceId, reg.id, num);
    },
    [vals, deviceId, sendWriteCommand],
  );

  if(parameters.length == 0) {
    return (
      <div className={styles.container}>
        <div className={styles.empty}>
          No writable parameters found in descriptor.
        </div>
      </div>
    );
  }

  return (
    <div className={styles.container}>
      {parameters.map((reg) => {
        var label = reg.name ?? reg.id;
        var meta: string[] = [reg.id];
        if(reg.min_value !== undefined && reg.max_value !== undefined) {
          meta.push(`${reg.min_value}\u2013${reg.max_value}`);
        }
        if(reg.unit) meta.push(reg.unit!);

        var curVal = vals[reg.id] ?? '';
        var isCurrent = lastSentId === reg.id;
        var showOk = isCurrent && commandState.lastAck?.success === true;
        // showErr: ack came back but success was false. the error
        // string from the gateway is often empty so we fall back to
        // a generic message
        var showErr = isCurrent && commandState.lastAck !== null && !commandState.lastAck.success;

        // enum detection — type field + non-empty fields array.
        //  had a bug where descriptors with type:'enum' but fields:[]

        var isEnum = reg.type === 'enum' && reg.fields && reg.fields.length > 0;

        return (
          <div key={reg.id} className={styles.paramRow}>
            <div className={styles.paramInfo}>
              <div className={styles.paramName}>{label}</div>
              <div className={styles.paramMeta}>{meta.join(' | ')}</div>
            </div>

            {isEnum ? (
              // enum: select dropdown, fires write immediately on change.
              // no Send button. see file header for why
              <>
                <select
                  className={styles.paramInput}
                  value={curVal || '0'}
                  onChange={e => {
                    updateVal(reg.id, e.target.value);
                    var n = Number(e.target.value);
                    if(!Number.isNaN(n)) {
                      setLastSentId(reg.id);
                      sendWriteCommand(deviceId, reg.id, n);
                    }
                  }}
                  aria-label={`Value for ${label}`}
                >
                  {reg.fields!.map(f => (
                    <option key={f.index} value={f.index}>{f.name ?? `Option ${f.index}`}</option>
                  ))}
                </select>
              </>
            ) : (
              // numeric: text input + explicit Send button.
              // tried type="number" with step support but the spinner
              // arrows were confusing for registers with float values,åß
              <>
                <input
                  type="number"
                  className={styles.paramInput}
                  value={curVal}
                  onChange={e => updateVal(reg.id, e.target.value)}
                  min={reg.min_value}
                  max={reg.max_value}
                  placeholder={reg.default_value !== undefined ? String(reg.default_value) : ''}
                  aria-label={`Value for ${label}`}
                />
                <button
                  type="button"
                  className={styles.sendBtn}
                  disabled={curVal === '' || commandState.pending}
                  onClick={() => handleSend(reg)}
                >
                  {commandState.pending && isCurrent ? 'Sending...' : 'Send'}
                </button>
              </>
            )}

            {showOk && <span className={styles.ackSuccess}>OK</span>}
            {showErr && (
              <span className={styles.ackError}>
                {commandState.lastAck?.error ?? 'Write failed'}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}