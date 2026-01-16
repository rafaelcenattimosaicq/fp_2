/* eslint-disable @typescript-eslint/no-unused-expressions */
import { Fragment, useState, useEffect, useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import type { VpnRequest } from '../../types';
import styles from './VpnApproval.module.css';
import { formatEpoch } from '../../utils/formatDate';

// poll every 10s, fast enough for admin but not spammy
const POLL_MS = 10_000;

function timeAgo(d: number): string {
  if (!d) return '';
  const secs = Math.floor(Date.now() / 1000 - d);
  if (secs < 60) return 'just now';
  const mins = Math.floor(secs / 60);
  if(mins < 60) return `${mins} min ago`;
  const hours = Math.floor(mins / 60);
  if(hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

// returns the right css class based on how high the trust score is
function trustCls(score: number | undefined): string {
  if (score === undefined) return styles.trustNone;
  if (score >= 80) return styles.trustHigh;
  if(score >= 50) return styles.trustMedium;
  return styles.trustLow;
}

interface Props {
  className?: string;
}

export function VpnApproval({ className }: Props): React.JSX.Element | null {
  const { listRequests, approveRequest } = useVpnService();
  const [requests, setRequests] = useState<VpnRequest[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [approving, setApproving] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [confirmId, setConfirmId] = useState<string | null>(null);

  const doRefresh = useCallback(async () => {
    if (!isVpnApiConfigured) return;
    try {
      setLoading(true);
      setError(null);
      const data = await listRequests(await getToken());
      // console.log('vpn requests loaded:', data.length);
      setRequests(data.filter(r => r.status === 'pending'));
    } catch(e) {
      setError(e instanceof Error ? e.message : 'could not load VPN requests');
    } finally {
      setLoading(false);
    }
  }, [listRequests]);

  useEffect(() => {
    if(!isVpnApiConfigured) return;
    doRefresh();
    const interval = setInterval(doRefresh, POLL_MS);
    return () => { clearInterval(interval); };
  }, [doRefresh]);

  const doApprove = useCallback(async (gwId: string) => {
    if(confirmId !== gwId) {
      setConfirmId(gwId);
      return;
    }
    try {
      setApproving(prev => new Set(prev).add(gwId));
      setConfirmId(null);
      await approveRequest(gwId, await getToken());
      setRequests(prev => prev.filter(r => r.gateway_id !== gwId));
    } catch (e) {
      console.error('approve failed for', gwId, e);
      setError(e instanceof Error ? e.message : `Failed to approve ${gwId}`);
    } finally {
      setApproving(prev => {
        const next = new Set(prev);
        next.delete(gwId);
        return next;
      });
    }
  }, [approveRequest, confirmId]);

  const doToggle = useCallback((gwId: string) => {
    setExpanded(prev => {
      const s = new Set(prev);
      s.has(gwId) ? s.delete(gwId) : s.add(gwId);
      return s;
    });
  }, []);

  if (!isVpnApiConfigured) return null;

  return (
    <div className={`${styles.panel} ${className ?? ''}`}>
      <div className={styles.header}>
        <h3 className={styles.title}>VPN Authorization Requests</h3>
        <button type="button" className={styles.refreshBtn}
          onClick={doRefresh} disabled={loading} aria-label="Refresh VPN requests">
          {loading ? '...' : 'Refresh'}
        </button>
      </div>

      {error && <p className={styles.error}>{error}</p>}

      {requests.length == 0 && !loading && !error && (
        <p className={styles.empty}>No pending authorization requests.</p>
      )}

      {requests.length > 0 && (
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
            {requests.map(item => {
              const isStale = item.created_at > 0 && (Date.now()/1000 - item.created_at) > 30 * 60;
              return (
              <Fragment key={item.gateway_id}>
                <tr className={isStale ? styles.staleRow : undefined}>
                  <td>
                    <button type="button" className={styles.expandBtn}
                      onClick={() => doToggle(item.gateway_id)}
                      aria-label={expanded.has(item.gateway_id) ? 'Collapse' : 'Expand'}>
                      {expanded.has(item.gateway_id) ? '▾' : '▸'}
                    </button>
                  </td>
                  <td className={styles.mono}>{item.gateway_id}</td>
                  <td className={styles.mono}>{item.source_ip ?? '-'}</td>
                  <td>
                    <span className={`${styles.trustBadge} ${trustCls(item.trust_score)}`}>
                      {item.trust_score !== undefined ? `${item.trust_score}/100` : 'N/A'}
                    </span>
                    {item.registry_validated
                      ? <span className={styles.validatedTag}>✓ Registered</span>
                      : <span className={styles.unregisteredTag}>Unregistered</span>
                    }
                  </td>
                  <td>
                    <span className={styles.time}>{!item.created_at ? '-' : formatEpoch(item.created_at)}</span>
                    <span className={styles.ago}>{timeAgo(item.created_at)}</span>
                    {isStale && <span className={styles.staleTag}>stale</span>}
                  </td>
                  <td>
                    {/* approve / confirm */}
                    <button type="button" className={styles.approveBtn}
                      onClick={() => doApprove(item.gateway_id)}
                      disabled={approving.has(item.gateway_id)}>
                      {approving.has(item.gateway_id) ? 'Approving...' : confirmId === item.gateway_id ? 'Confirm?' : 'Approve'}
                    </button>
                  </td>
                </tr>

                {expanded.has(item.gateway_id) && item.fingerprint && (
                  <tr className={styles.detailRow}>
                    <td></td>
                    <td colSpan={5}>
                      <div className={styles.fingerprint}>
                        <span className={styles.fpLabel}>Hardware Fingerprint</span>
                        <div className={styles.fpGrid}>
                          {([
                            ['MAC Address', item.fingerprint.mac_address],
                            ['CPU ID',       item.fingerprint.cpu_id],
                            ['Serial Number', item.fingerprint.serial_number],
                            ['Hostname',     item.fingerprint.hostname],
                            ['OS Info',      item.fingerprint.os_info],
                          ] as const).map(([label, val]) =>
                            <div className={styles.fpField} key={label}>
                              <span className={styles.fpKey}>{label}</span>
                              <span className={styles.fpValue}>{val || '-'}</span>
                            </div>
                          )}
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
