/* eslint-disable no-var */
// Polls pending Tailscale VPN approval 
import { useState, useEffect } from 'react';
import { fetchAuthSession } from 'aws-amplify/auth';
import { useVpnService, isVpnApiConfigured } from './useVpnService';

var POLL_INTERVAL = 15_000;

export function usePendingVpnCount(): number {
  const { listRequests } = useVpnService();
  const [count, setCount] = useState(0);

  useEffect(() => {
    if (!isVpnApiConfigured) return;

    var active = true;

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
    }

    const initialTimer = setTimeout(tick, 500);
    var interval = setInterval(tick, POLL_INTERVAL);

    return () => {
      active = false;
      clearTimeout(initialTimer);
      clearInterval(interval);
    };
  }, [listRequests]);

  return count;
}
