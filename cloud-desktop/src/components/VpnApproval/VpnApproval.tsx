/* eslint-disable no-var */
import { Fragment, useState, useEffect, useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import type { VpnRequest } from '../../types';
import styles from './VpnApproval.module.css';

// tailscale VPN approval workflow  gateway 

const STALE_SECS = 30 * 60;

function fmtTs(epoch: number): string {
  if (!epoch) return '-';
  return new Date(epoch * 1000).toLocaleString();
}

// compact relative time
function timeAgo(epoch: number): string {
  if (!epoch) return '';
  var d = Math.floor(Date.now() / 1000 - epoch);
  if (d < 60) return 'just now';
  if (d < 3600) return Math.floor(d / 60) + ' min ago';
  if (d < 86400) return Math.floor(d / 3600) + 'h ago';
  return Math.floor(d / 86400) + 'd ago';
}

// hardware fingerprint fields 
var FP_FIELDS: {key: keyof NonNullable<VpnRequest['fingerprint']>; label: string}[] = [
  {key: 'mac_address', label: 'MAC Address'},
  {key: 'cpu_id',      label: 'CPU ID'},
  {key: 'serial_number', label: 'Serial Number'},
  {key: 'hostname',    label: 'Hostname'},
  {key: 'os_info',     label: 'OS Info'},
];

interface Props {
  className?: string;
}

export function VpnApproval({ className }: Props): React.JSX.Element | null {
  const { listRequests, approveRequest } = useVpnService();
  const [reqs, setReqs] = useState<VpnRequest[]>([]);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [approvingSet, setApprovingSet] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [pendingConfirm, setPendingConfirm] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isVpnApiConfigured) return;
    try {
      setLoading(true); setErr(null);
      const tok = await getToken();
      const data = await listRequests(tok);
      setReqs(data.filter(r => r.status === 'pending'));
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'could not load VPN requests');
    } finally {
      setLoading(false);
    }
  }, [listRequests]);
  

  const POLL_MS = 5000

  useEffect(() => {
    if (!isVpnApiConfigured) return;
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  // two-click approval
  const handleApprove = useCallback(async (gwId: string) => {
	if (pendingConfirm !== gwId) {
		setPendingConfirm(gwId);
		return;
	}
	try {
		setApprovingSet(prev => new Set(prev).add(gwId));
		setPendingConfirm(null);
		const tok = await getToken();
		await approveRequest(gwId, tok);
		setReqs(prev => prev.filter(r => r.gateway_id !== gwId));
	} catch (e) {
		console.error('approve failed for', gwId, e);
		setErr(e instanceof Error ? e.message : `Failed to approve ${gwId}`);
	} finally {
		setApprovingSet(prev => {
			const nxt = new Set(prev);
			nxt.delete(gwId);
			return nxt;
		});
	}
  }, [approveRequest, pendingConfirm]);

  function toggleRow(gwId: string) {
    setExpanded(prev => ({...prev, [gwId]: !prev[gwId]}));
  }

  if (!isVpnApiConfigured) return null;

  return (
    <div className={`${styles.panel} ${className ?? ''}`}>
      <div className={styles.header}>
        <h3 className={styles.title}>VPN Authorization Requests</h3>
        <button type="button" className={styles.refreshBtn}
          onClick={refresh} disabled={loading} aria-label="Refresh VPN requests">
          {loading ? '...' : 'Refresh'}
        </button>
      </div>

      {err && <p className={styles.error}>{err}</p>}

      {reqs.length === 0 && !loading && !err && (
        <p className={styles.empty}>No pending authorization requests.</p>
      )}

      {reqs.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th></th>
              <th>Gateway ID</th>
              <th>Source IP</th>
              <th>Trust Score</th>
              <th>Requested</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {reqs.map(req => {
              const stale = req.created_at > 0
                && (Date.now() / 1000 - req.created_at) > STALE_SECS;
              const isExp = !!expanded[req.gateway_id];
              // trust badge class
              var tCls = req.trust_score === undefined ? styles.trustNone
                : req.trust_score >= 80 ? styles.trustHigh
                : req.trust_score >= 50 ? styles.trustMedium
                : styles.trustLow;
              return (
              <Fragment key={req.gateway_id}>
                <tr className={stale ? styles.staleRow : undefined}>
                  <td>
                    <button type="button" className={styles.expandBtn}
                      onClick={() => toggleRow(req.gateway_id)}
                      aria-label={isExp ? 'Collapse' : 'Expand'}>
                      {isExp ? '▾' : '▸'}
                    </button>
                  </td>
                  <td className={styles.mono}>{req.gateway_id}</td>
                  <td className={styles.mono}>{req.source_ip ?? '-'}</td>
                  <td>
                    <span className={`${styles.trustBadge} ${tCls}`}>
                      {req.trust_score !== undefined ? `${req.trust_score}/100` : 'N/A'}
                    </span>
                    {req.registry_validated
                      ? <span className={styles.validatedTag}>✓ Registered</span>
                      : <span className={styles.unregisteredTag}>Unregistered</span>}
                  </td>
                  <td>
                    <span className={styles.time}>{fmtTs(req.created_at)}</span>
                    <span className={styles.ago}>{timeAgo(req.created_at)}</span>
                    {stale && <span className={styles.staleTag}>stale</span>}
                  </td>
                  <td>
                    <button type="button" className={styles.approveBtn}
                      onClick={() => handleApprove(req.gateway_id)}
                      disabled={approvingSet.has(req.gateway_id)}>
                      {approvingSet.has(req.gateway_id) ? 'Approving...'
                        : pendingConfirm === req.gateway_id ? 'Confirm?'
                        : 'Approve'}
                    </button>
                  </td>
                </tr>

                {isExp && req.fingerprint && (
                  <tr className={styles.detailRow}>
                    <td></td>
                    <td colSpan={5}>
                      <div className={styles.fingerprint}>
                        <span className={styles.fpLabel}>Hardware Fingerprint</span>
                        <div className={styles.fpGrid}>
                          {FP_FIELDS.map(f => <div key={f.key} className={styles.fpField}>
                            <span className={styles.fpKey}>{f.label}</span>
                            <span className={styles.fpValue}>{req.fingerprint![f.key] || '-'}</span>
                          </div>)}
                        </div>
                      </div>
                    </td>
                  </tr>
                )}
              </Fragment>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}