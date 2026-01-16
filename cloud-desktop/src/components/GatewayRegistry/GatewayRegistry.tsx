/* eslint-disable no-var */
import { useState, useEffect,useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import type { RegisteredGateway } from '../../types';
import styles from './GatewayRegistry.module.css';
import { formatEpoch } from '../../utils/formatDate';
// import { useConfirmDialog } from '../../hooks/useConfirmDialog';


function fmtTs(epoch: number): string {
  return epoch ? formatEpoch(epoch) : '-';
}

interface Props {
  className?: string
}

// gateway CRUD panel. this is the longest component in the app anddd
// i'm sorry about that — the form alone has 7 fields because the VPN
// service needs all the device fingerprint, so it's necessary for security

export function GatewayRegistry({ className }: Props): React.JSX.Element | null {
  const { listGateways, registerGateway, deleteGateway } = useVpnService();

  const [gateways, setGateways] = useState<RegisteredGateway[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // --- form state --
  const [showForm, setShowForm] = useState(false);
  const [gwId, setGwId] = useState('');
  const [loc, setLoc] = useState('');
  const [mac, setMac] = useState('');
  const [cpuId, setCpuId] = useState('');
  const [host, setHost] = useState('');
  const [osInfo, setOsInfo] = useState('');
  const [serial, setSerial] = useState('');
  const [registering, setRegistering] = useState(false);


  const [secret, setSecret] = useState<{ id: string; secret: string } | null>(null);
  const [confirmDel, setConfirmDel] = useState<string | null>(null)
  const [deletingIds, setDeletingIds] = useState<Set<string>>(new Set());

  // --- data loading -------------------------------------------------

  const doRefresh = useCallback(async () => {
    if (!isVpnApiConfigured) return;
    try {
      setLoading(true); setError(null);
      var token = await getToken();
      // console.log('fetching gateway list...');
      setGateways(await listGateways(token));
    } catch(e) {
      setError(e instanceof Error ? e.message : 'load gateways');
    } finally { setLoading(false) }
  }, [listGateways]);

  useEffect(() => { doRefresh() }, [doRefresh]);

  // --- registration -------------------------------------------------

  function clearForm(){
    setGwId(''); setLoc(''); setMac('');
    setCpuId(''); setHost('');
    setOsInfo(''); setSerial('');
  }

  // registers gateway, gets back one-time enrollment secret
  const doRegister = useCallback(async () => {
    if (!gwId.trim()) return;

    try {
      setRegistering(true); setError(null);
      var token = await getToken();

      // the expected_* fields are optional but the VPN service uses them
      // for device attestation on first connect
      var res = await registerGateway({
        gateway_id: gwId.trim(),
        location: loc.trim() || undefined,
        expected_mac_address: mac.trim() || undefined,
        expected_cpu_id: cpuId.trim() || undefined,
        expected_hostname: host.trim() || undefined,
        expected_os_info: osInfo.trim() || undefined,
        expected_serial_number: serial.trim() || undefined,
      }, token);

      setSecret({ id: res.gateway_id, secret: res.pre_shared_secret });
      setShowForm(false);
      clearForm();
      await doRefresh();
    } catch(e) {
      setError(e instanceof Error ? e.message : 'Registration failed');
    } finally { setRegistering(false) }
  }, [gwId, loc, mac, cpuId, host, osInfo, serial, registerGateway, doRefresh]);

  // --- delete (two-click confirm) -----------------------------------

  const doDelete = useCallback(async (id: string) => {
    if(confirmDel !== id) { setConfirmDel(id); return }

    try {
      setDeletingIds(prev => new Set(prev).add(id));
      setConfirmDel(null);
      var token = await getToken();
      await deleteGateway(id, token);
      // optimistic remove
      setGateways(prev => prev.filter(g => g.gateway_id !== id));
    } catch(e) {
      setError(e instanceof Error ? e.message : `Failed to delete ${id}`);
    } finally {
      setDeletingIds(prev => { var s = new Set(prev); s.delete(id); return s });
    }
  }, [confirmDel, deleteGateway]);

  // clipboard with prompt fall
  async function copySecret(){
    if(!secret) return;
    try { await navigator.clipboard.writeText(secret.secret) }
    catch { window.prompt('Copy this secret manually:', secret.secret) }
  }

  if (!isVpnApiConfigured) return null;

  // --- render -------------------------------------------------------
  // see comment at top of component

  return (
    <div className={`${styles.container} ${className ?? ''}`}>
      <div className={styles.toolbar}>
        <h2 className={styles.title}>Gateway Registry</h2>
        <button type="button" className={styles.registerBtn}
          onClick={() => setShowForm(!showForm)} disabled={registering}>
          {showForm ? 'Cancel' : '+ Register Gateway'}
        </button>
      </div>

      {error && <p className={styles.error}>{error}</p>}
      {secret && (
        <div className={styles.secretBox}>
          <p className={styles.secretWarning}>
            ⚠ Save this secret now - it will NOT be shown again.
          </p>
          <p className={styles.secretGateway}>
            Gateway: <strong>{secret.id}</strong>
          </p>
          <code className={styles.secretValue}>{secret.secret}</code>
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
              onClick={() => setSecret(null)}>
              I&apos;ve saved it
            </button>
          </div>
        </div>
      )}

      {/* ---- registration form ---- */}
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
              <input className={styles.formInput} value={gwId}
                onChange={e => setGwId(e.target.value)} placeholder="e.g. GW-EDGE-001" />
              <span className={styles.fieldHint}>Unique per device, must match gateway_id in the device config</span>
            </div>
          </div>
          <div className={styles.formRow}>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Location</label>
              <input className={styles.formInput} value={loc}
                onChange={e => setLoc(e.target.value)} placeholder="e.g. Factory Floor A" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected MAC Address</label>
              <input className={styles.formInput} value={mac}
                onChange={e => setMac(e.target.value)} placeholder="e.g. dc:a6:32:xx:xx:xx" />
            </div>
          </div>
          <div className={styles.formRow}>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected CPU ID</label>
              <input className={styles.formInput} value={cpuId}
                onChange={e => setCpuId(e.target.value)} placeholder="e.g. 0000000012345678" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected Serial Number</label>
              <input className={styles.formInput} value={serial}
                onChange={e => setSerial(e.target.value)} placeholder="e.g. SN-ABC-12345" />
            </div>
          </div>
          <div className={styles.formRow}>
              <div className={styles.formField}>
              <label className={styles.formLabel}>Expected Hostname</label>
              <input className={styles.formInput} value={host}
                onChange={e => setHost(e.target.value)} placeholder="e.g. gateway-edge-001" />
            </div>
            <div className={styles.formField}>
              <label className={styles.formLabel}>Expected OS Info</label>
              <input className={styles.formInput} value={osInfo}
                onChange={e => setOsInfo(e.target.value)} placeholder="e.g. Linux raspberrypi 5.10" />
            </div>
          </div>
          <div className={styles.formActions}>
            <button type="button" className={styles.cancelBtn}
              onClick={() => { setShowForm(false); clearForm() }}>Cancel</button>
            <button type="button" className={styles.registerBtn}
              onClick={doRegister} disabled={registering || !gwId.trim()}>
              {registering ? 'Registering...' : 'Register'}
            </button>
          </div>
        </div>
      )}

      {/* ---- gateway table ---- */}
      {loading && <p className={styles.loading}>Loading gateways...</p>}

      {!loading && gateways.length === 0 && (
        <p className={styles.empty}>No gateways registered. Click &quot;+ Register Gateway&quot; to add one.</p>
      )}

      {gateways.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Gateway ID</th>
              <th>Location</th>
              <th>Registered</th>
              <th>Expected MAC</th>
              <th>Expected CPU</th>
              {/* hostname + os + serial hidden to keep table narrow.
                  expand row feature is in the backlog (DASH-147) */}
              <th></th>
            </tr>
          </thead>
          <tbody>
            {gateways.map(gw => {
              var isDeleting = deletingIds.has(gw.gateway_id);
              var isConfirming = confirmDel === gw.gateway_id;
              var btnLabel = isDeleting ? 'Deleting...'
                : isConfirming ? 'Confirm?' : 'Delete';

              return (
                <tr key={gw.gateway_id}>
                  <td className={styles.mono}>{gw.gateway_id}</td>
                  <td>{gw.location ?? '-'}</td>
                  <td>{fmtTs(gw.registered_at)}</td>
                  <td className={styles.mono}>{gw.expected_mac_address ?? '-'}</td>
                  <td className={styles.mono}>{gw.expected_cpu_id ?? '-'}</td>
                  <td>
                    <button type="button" className={styles.deleteBtn}
                      onClick={() => doDelete(gw.gateway_id)}
                      disabled={isDeleting}>
                      {btnLabel}
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}