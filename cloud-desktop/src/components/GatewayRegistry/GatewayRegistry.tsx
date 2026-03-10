import { useState, useEffect, useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import type { RegisteredGateway } from '../../types';
import styles from './GatewayRegistry.module.css';

// TODO: add CSV bulk-import for registering entire batches of gateways at once

function fmtTs(epoch: number): string {
  return epoch ? new Date(epoch * 1000).toLocaleString() : '-';
}

interface Props {
  className?: string;
}

export function GatewayRegistry({ className }: Props): React.JSX.Element | null {
  const { listGateways, registerGateway, deleteGateway } = useVpnService();
  const [gws, setGws] = useState<RegisteredGateway[]>([]);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // registration form state, kept flat, no sub-component
  const [showForm, setShowForm] = useState(false);
  const [fId, setFId] = useState('');
  const [fLoc, setFLoc] = useState('');
  const [fMac, setFMac] = useState('');
  const [fCpu, setFCpu] = useState('');
  const [fHost, setFHost] = useState('');
  const [fOs, setFOs] = useState('');
  const [fSerial, setFSerial] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const [newSecret, setNewSecret] = useState<{ id: string; secret: string } | null>(null);

  const [cfmDel, setCfmDel] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    if (!isVpnApiConfigured) return;
    try {
      setLoading(true); setErr(null);
      const tok = await getToken();
      setGws(await listGateways(tok));
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'load gateways');
    } finally { setLoading(false); }
  }, [listGateways]);

  useEffect(() => { refresh(); }, [refresh]);

  function resetForm(): void {
    setFId(''); setFLoc(''); setFMac(''); setFCpu('');
    setFHost(''); setFOs(''); setFSerial('');
  }

  const handleRegister = useCallback(async () => {
    if (!fId.trim()) return;
    try {
      setSubmitting(true); setErr(null);
      const tok = await getToken();
      const res = await registerGateway({
        gateway_id: fId.trim(),
        location: fLoc.trim() || undefined,
        expected_mac_address: fMac.trim() || undefined,
        expected_cpu_id: fCpu.trim() || undefined,
        expected_hostname: fHost.trim() || undefined,
        expected_os_info: fOs.trim() || undefined,
        expected_serial_number: fSerial.trim() || undefined,
      }, tok);

      // without copying it they have to delete and re-register.
      setNewSecret({ id: res.gateway_id, secret: res.pre_shared_secret });
      setShowForm(false);
      resetForm();
      await refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'Registration failed');
    } finally { setSubmitting(false); }
  }, [fId, fLoc, fMac, fCpu, fHost, fOs, fSerial, registerGateway, refresh]);

  const handleDelete = useCallback(async (gwId: string) => {
    if (cfmDel !== gwId) { setCfmDel(gwId); return; }
    try {
      setDeleting(prev => new Set(prev).add(gwId));
      setCfmDel(null);
      const tok = await getToken();
      await deleteGateway(gwId, tok);
      setGws(prev => prev.filter(g => g.gateway_id !== gwId));
    } catch (e) {
      // deletion errors are surfaced inline, we don't throw because
      setErr(e instanceof Error ? e.message : `Failed to delete ${gwId}`);
    } finally {
      setDeleting(prev => { const n = new Set(prev); n.delete(gwId); return n; });
    }
  }, [cfmDel, deleteGateway]);

  // clipboard API can fail inside Tauri webview on some Linux distros,
  async function copySecret(): Promise<void> {
    if (!newSecret) return;
    try { await navigator.clipboard.writeText(newSecret.secret); }
    catch { window.prompt('Copy this secret manually:', newSecret.secret); }
  }

  if (!isVpnApiConfigured) return null;

  return (
    <div className={`${styles.container} ${className ?? ''}`}>
      <div className={styles.toolbar}>
        <h2 className={styles.title}>Gateway Registry</h2>
        <button type="button" className={styles.registerBtn}
          onClick={() => setShowForm(!showForm)} disabled={submitting}>
          {showForm ? 'Cancel' : '+ Register Gateway'}
        </button>
      </div>

      {err && <p className={styles.error}>{err}</p>}

      {/* one-time secret banner - shown immediately after registration */}
      {newSecret && (
        <div className={styles.secretBox}>
          <p className={styles.secretWarning}>
            ⚠ Save this secret now - it will NOT be shown again.
          </p>
          <p className={styles.secretGateway}>
            Gateway: <strong>{newSecret.id}</strong>
          </p>
          <code className={styles.secretValue}>{newSecret.secret}</code>
          <p className={styles.secretInstructions}>
            This is a <strong>one-time enrollment token</strong>. Set it
            as <code>pre_shared_secret</code> under the <code>vpn</code> section
            in the device&apos;s <code>gateway.yaml</code>, or export it as
            the <code>GATEWAY_SECRET</code> environment variable.
            On first boot, the gateway will be <strong>auto-approved</strong> and
            the token is permanently invalidated.
          </p>
          <div className={styles.secretActions}>
            <button type="button" className={styles.copyBtn} onClick={copySecret}>
              Copy to Clipboard
            </button>
            <button type="button" className={styles.dismissBtn}
              onClick={() => setNewSecret(null)}>
              I&apos;ve saved it
            </button>
          </div>
        </div>
      )}

      {/* inline registration form - no separate component because it shares
          state with the parent (newSecret, error, refresh) */}
      {showForm && (
        <div className={styles.form}>
          <h3 className={styles.formTitle}>Register New Gateway</h3>
          <p className={styles.formHint}>
            Each physical gateway device must be registered with a unique ID.
            This ID must match the <code>gateway_id</code> field in the
            device&apos;s <code>gateway.yaml</code> config file.
          </p>
          <div className={styles.formRow}>
            <div className={`${styles.formField} ${styles.full}`}>
              <label className={styles.formLabel}>Gateway ID *</label>
              <input className={styles.formInput} value={fId}
                onChange={e => setFId(e.target.value)} placeholder="e.g. GW-EDGE-001" />
              <span className={styles.fieldHint}>Unique per device, must match gateway_id in the device config</span>
            </div>
          </div>
          <div className={styles.formRow}>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Location</label>
              <input className={styles.formInput} value={fLoc}
                onChange={e => setFLoc(e.target.value)} placeholder="e.g. Factory Floor A" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected MAC Address</label>
              <input className={styles.formInput} value={fMac}
                onChange={e => setFMac(e.target.value)} placeholder="e.g. dc:a6:32:xx:xx:xx" />
            </div>
          </div>
          <div className={styles.formRow}>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected CPU ID</label>
              <input className={styles.formInput} value={fCpu}
                onChange={e => setFCpu(e.target.value)} placeholder="e.g. 0000000012345678" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected Serial Number</label>
              <input className={styles.formInput} value={fSerial}
                onChange={e => setFSerial(e.target.value)} placeholder="e.g. SN-ABC-12345" />
            </div>
          </div>
          <div className={styles.formRow}>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected Hostname</label>
              <input className={styles.formInput} value={fHost}
                onChange={e => setFHost(e.target.value)} placeholder="e.g. gateway-edge-001" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected OS Info</label>
              <input className={styles.formInput} value={fOs}
                onChange={e => setFOs(e.target.value)} placeholder="e.g. Linux raspberrypi 5.10" />
            </div>
          </div>
          <div className={styles.formActions}>
            <button type="button" className={styles.cancelBtn}
              onClick={() => { setShowForm(false); resetForm(); }}>Cancel</button>
            <button type="button" className={styles.registerBtn}
              onClick={handleRegister} disabled={submitting || !fId.trim()}>
              {submitting ? 'Registering...' : 'Register'}
            </button>
          </div>
        </div>
      )}

      {loading && <p className={styles.loading}>Loading gateways...</p>}

      {!loading && gws.length === 0 && (
        <p className={styles.empty}>No gateways registered. Click &quot;+ Register Gateway&quot; to add one.</p>
      )}

      {gws.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Gateway ID</th>
              <th>Location</th>
              <th>Registered</th>
              <th>Expected MAC</th>
              <th>Expected CPU</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {gws.map(gw => (
              <tr key={gw.gateway_id}>
                <td className={styles.mono}>{gw.gateway_id}</td>
                <td>{gw.location ?? '-'}</td>
                <td>{fmtTs(gw.registered_at)}</td>
                <td className={styles.mono}>{gw.expected_mac_address ?? '-'}</td>
                <td className={styles.mono}>{gw.expected_cpu_id ?? '-'}</td>
                <td>
                  <button type="button" className={styles.deleteBtn}
                    onClick={() => handleDelete(gw.gateway_id)}
                    disabled={deleting.has(gw.gateway_id)}>
                    {deleting.has(gw.gateway_id) ? 'Deleting...'
                      : cfmDel === gw.gateway_id ? 'Confirm?' : 'Delete'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
