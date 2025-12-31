/* eslint-disable no-var */
import { useState, useCallback } from 'react';
import type { DescriptorRegister } from '../../types/descriptor';
import { useDeviceCommands } from '../../hooks/useDeviceCommands';
import styles from './DeviceConfigurator.module.css';

// Write-parameter panel for a single device. Started as a quick prototype


interface Props {
  parameters: DescriptorRegister[];
  deviceId: string;
  gatewayId: string;
}

export function DeviceConfigurator({ parameters, deviceId, gatewayId }: Props): React.JSX.Element {
  const { sendWriteCommand, commandState } = useDeviceCommands(gatewayId);
  const [vals, setVals] = useState<Record<string, string>>({});
  // only tracks last sent registequick consecutive writes eat earlier
  // acks.
  const [lastSentId, setLastSentId] = useState<string | null>(null);

  const updateVal = useCallback((regId: string, v: string) => {
    setVals(prev => ({ ...prev, [regId]: v }));
  }, []);

  // backend also validates but its error msg is useless ("INVALID_PARAMETER_VALUE")
  const handleSend = useCallback(
    (reg: DescriptorRegister) => {
      var raw = vals[reg.id];
      if(raw === undefined || raw === '') return;

      var num = Number(raw);
      if((Number.isNaN(num))) return;
      if(reg.min_value !== undefined && num < reg.min_value!) return;
      if(reg.max_value !== undefined && num > reg.max_value!) return;

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

        // "REG_042 | 0–100 | °C"  (endass design review ask)
        var meta: string[] = [reg.id];
        if(reg.min_value !== undefined && reg.max_value !== undefined) {
          meta.push(`${reg.min_value}\u2013${reg.max_value}`);
        }
        if(reg.unit) meta.push(reg.unit!);

        var curVal = vals[reg.id] ?? '';
        var isCurrent = lastSentId === reg.id;
        var showOk = isCurrent && commandState.lastAck?.success === true;
        var showErr = isCurrent && commandState.lastAck !== null && !commandState.lastAck.success;

        var isEnum = reg.type === 'enum' && reg.fields && reg.fields.length > 0;

        return (
          <div key={reg.id} className={styles.paramRow}>
            <div className={styles.paramInfo}>
              <div className={styles.paramName}>{label}</div>
              <div className={styles.paramMeta}>{meta.join(' | ')}</div>
            </div>

            {isEnum ? (
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