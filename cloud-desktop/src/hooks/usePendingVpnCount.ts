import { useState, useEffect } from 'react';
import { fetchAuthSession } from 'aws-amplify/auth';
import { useVpnService, isVpnApiConfigured } from './useVpnService';

// tailscale's per-IP rate limit during bulk provisioning windows.
const POLL_INTERVAL = 15_000;

/**
 * Returns the count of VPN requests with status "pending".
 *
 * This drives the badge on the sidebar nav item. We use a simple
 * number return (not an object) because the consumer only ever needs
 * the count - loading/error states are irrelevant for a badge that
 * should silently disappear when the API is unreachable.
 */
export function usePendingVpnCount(): number {
  const { listRequests } = useVpnService();
  const [count, setCount] = useState(0);

  useEffect(() => {
    if (!isVpnApiConfigured) return;

    let active = true;

    function tick(): void {
      fetchAuthSession()
        .then((session) => {
          const token = session.tokens?.idToken?.toString() ?? '';
          if (!token || !active) return null;
          return listRequests(token);
        })
        .then((requests) => {
          if (requests && active) {
            setCount(requests.filter((r) => r.status === 'pending').length);
          }
        })
        .catch(() => {
          // silently swallow, the badge just keeps showing the stale count
        });
    }

    const initialTimer = setTimeout(tick, 500);
    const interval = setInterval(tick, POLL_INTERVAL);

    return () => {
      active = false;
      clearTimeout(initialTimer);
      clearInterval(interval);
    };
  }, [listRequests]);

  return count;
}
