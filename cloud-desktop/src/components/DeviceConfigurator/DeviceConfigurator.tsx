
import { useState, useCallback } from 'react';
import type { DescriptorRegister } from '../../types/descriptor';
import { useDeviceCommands } from '../../hooks/useDeviceCommands';
import styles from './DeviceConfigurator.module.css';

interface Props {
  /** Writable registers parsed from the device descriptor YAML.
   * Filtered by getWritableParameters - excludes read-only, enum,
   * and bitwise registers. */
  parameters: DescriptorRegister[];
  /** Hex device ID read from Modbus register 60000 (e.g. "0x1A3F") */
  deviceId: string;
  /** Gateway that owns this device - used to route MQTT write cmds */
  gatewayId: string;
}

/*
 * Configurator panel for writing Modbus holding registers (FC06 single,
 * FC16 multi) via MQTT command messages routed through the gateway.
 *
 * The flow is:
 *   UI -> MQTT publish to commands/{gw}/write
 *   Gateway -> Modbus FC06 write to physical device
 *   Device -> response read back by gateway
 *   Gateway -> MQTT ack on commands/{gw}/write/ack
 *
 * Ack timeout is 8s (set in useDeviceCommands hook). Some cellular
 * gateways in rural SP locations need the extra headroom.
 */

export function DeviceConfigurator({ parameters, deviceId, gatewayId }: Props): React.JSX.Element {
  const { sendWriteCommand, commandState } = useDeviceCommands(gatewayId);

  const [inputVals, setInputVals] = useState<Record<string, string>>({});
  const [lastWriteReg, setLastWriteReg] = useState<string | null>(null);

  // seen descriptors with 40+ writable params on the client VCC units
  const handleInputChange = useCallback((regId: string, val: string) => {
    setInputVals(prev => ({ ...prev, [regId]: val }));
  }, []);

  const handleSend = useCallback(
    (reg: DescriptorRegister) => {
      const raw = inputVals[reg.id];
      if (raw === undefined || raw === '') return;

      const num = Number(raw);
      if (Number.isNaN(num)) return;

      // min/max would wrap or get rejected by the compressor firmware
      if (reg.min_value !== undefined && num < reg.min_value) return;
      if (reg.max_value !== undefined && num > reg.max_value) return;

      setLastWriteReg(reg.id);
      sendWriteCommand(deviceId, reg.id, num);
    },
    [inputVals, deviceId, sendWriteCommand],
  );

  if (parameters.length === 0) {
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
      {parameters.map((param) => {
        const label = param.name ?? param.id;

        // e.g. "REG_COMP_SET_SPEED | 1000-6500 | RPM"
        const meta: string[] = [param.id];
        if (param.min_value !== undefined && param.max_value !== undefined) {
          meta.push(`${param.min_value}\u2013${param.max_value}`);
        }
        if (param.unit) meta.push(param.unit);

        const curVal = inputVals[param.id] ?? '';
        const empty = curVal === '';

        const isLastWrite = lastWriteReg === param.id;
        const showOk = isLastWrite && commandState.lastAck?.success === true;
        const showErr = isLastWrite && commandState.lastAck !== null && !commandState.lastAck.success;

        return (
          <div key={param.id} className={styles.paramRow}>
            <div className={styles.paramInfo}>
              <div className={styles.paramName}>{label}</div>
              <div className={styles.paramMeta}>{meta.join(' | ')}</div>
            </div>

            <input
              type="number"
              className={styles.paramInput}
              value={curVal}
              onChange={e => handleInputChange(param.id, e.target.value)}
              min={param.min_value}
              max={param.max_value}
              placeholder={param.default_value !== undefined ? String(param.default_value) : ''}
              aria-label={`Value for ${label}`}
            />

            <button
              type="button"
              className={styles.sendBtn}
              disabled={empty || commandState.pending}
              onClick={() => handleSend(param)}
            >
              {commandState.pending && isLastWrite ? 'Sending...' : 'Send'}
            </button>

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
