import { useState, useEffect } from 'react';
import { fetchAuthSession } from 'aws-amplify/auth';
import { useVpnService, isVpnApiConfigured } from './useVpnService';

// poll every 15s, tailscale rate limits during bulk provisioning
const POLL_MS = 15_000;

// just returns the count of pending vpn requests for the sidebar badge
// doesn't bother with loading/error state since badge should just disappear silently
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
          // silently swallow, badge just keeps stale count
        });
    }

    // small delay before first tick
    const t1 = setTimeout(tick, 500);
    const t2 = setInterval(tick, POLL_MS);

    return () => {
      active = false;
      clearTimeout(t1);
      clearInterval(t2);
    };
  }, [listRequests]);

  return count;
}
