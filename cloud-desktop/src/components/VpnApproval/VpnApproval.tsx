import { Fragment, useState, useEffect, useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import type { VpnRequest } from '../../types';
import styles from './VpnApproval.module.css';

// tailscale VPN approval workflow, each gateway in the field requests
// for the the security team team to review before the auth key is issued.
// gateways that were pre-registered with a one-time enrollment token
// TODO: add bulk-approve for multiple gateways when doing a fleet rollout
// TODO: integrate with the client's internal ticketing system for audit trail

const POLL_MS = 10_000;

const STALE_SECS = 30 * 60;

function fmtTs(epoch: number): string {
  if (!epoch) return '-';
  return new Date(epoch * 1000).toLocaleString();
}

function timeAgo(epoch: number): string {
  if (!epoch) return '';
  const secs = Math.floor(Date.now() / 1000 - epoch);
  if (secs < 60) return 'just now';
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function trustCls(score: number | undefined): string {
  if (score === undefined) return styles.trustNone;
  if (score >= 80) return styles.trustHigh;
  if (score >= 50) return styles.trustMedium;
  return styles.trustLow;
}

interface Props {
  className?: string;
}

export function VpnApproval({ className }: Props): React.JSX.Element | null {
  const { listRequests, approveRequest } = useVpnService();
  const [reqs, setReqs] = useState<VpnRequest[]>([]);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  // tracks which gateway IDs are mid-approval so we can disable their buttons
  const [approvingSet, setApprovingSet] = useState<Set<string>>(new Set());
  const [expandedSet, setExpandedSet] = useState<Set<string>>(new Set());
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

  useEffect(() => {
    if (!isVpnApiConfigured) return;
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

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
      // optimistic removal, the gateway won't appear on the next poll anyway
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

  const toggleExpand = useCallback((gwId: string) => {
    setExpandedSet(prev => {
      const nxt = new Set(prev);
      if (nxt.has(gwId)) { nxt.delete(gwId); } else { nxt.add(gwId); }
      return nxt;
    });
  }, []);

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
              const isExp = expandedSet.has(req.gateway_id);
              return (
              <Fragment key={req.gateway_id}>
                <tr className={stale ? styles.staleRow : undefined}>
                  <td>
                    <button type="button" className={styles.expandBtn}
                      onClick={() => toggleExpand(req.gateway_id)}
                      aria-label={isExp ? 'Collapse' : 'Expand'}>
                      {isExp ? '▾' : '▸'}
                    </button>
                  </td>
                  <td className={styles.mono}>{req.gateway_id}</td>
                  <td className={styles.mono}>{req.source_ip ?? '-'}</td>
                  <td>
                    {/* trust score badge - computed from hardware fingerprint match */}
                    <span className={`${styles.trustBadge} ${trustCls(req.trust_score)}`}>
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

                {/* expanded row: hardware fingerprint details for manual verification */}
                {isExp && req.fingerprint && (
                  <tr className={styles.detailRow}>
                    <td></td>
                    <td colSpan={5}>
                      <div className={styles.fingerprint}>
                        <span className={styles.fpLabel}>Hardware Fingerprint</span>
                        <div className={styles.fpGrid}>
                          <div className={styles.fpField}>
                            <span className={styles.fpKey}>MAC Address</span>
                            <span className={styles.fpValue}>{req.fingerprint.mac_address || '-'}</span>
                          </div>
                          <div className={styles.fpField}>
                            <span className={styles.fpKey}>CPU ID</span>
                            <span className={styles.fpValue}>{req.fingerprint.cpu_id || '-'}</span>
                          </div>
                          <div className={styles.fpField}>
                            <span className={styles.fpKey}>Serial Number</span>
                            <span className={styles.fpValue}>{req.fingerprint.serial_number || '-'}</span>
                          </div>
                          <div className={styles.fpField}>
                            <span className={styles.fpKey}>Hostname</span>
                            <span className={styles.fpValue}>{req.fingerprint.hostname || '-'}</span>
                          </div>
                          <div className={styles.fpField}>
                            <span className={styles.fpKey}>OS Info</span>
                            <span className={styles.fpValue}>{req.fingerprint.os_info || '-'}</span>
                          </div>
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
