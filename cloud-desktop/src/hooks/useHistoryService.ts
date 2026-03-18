/* eslint-disable prefer-const */
/* eslint-disable no-var */
// Telemetry history
// { query_id } on POST, then GET polls until state === "SUCCEEDED".
import { useCallback, useMemo } from 'react'
import type { HistoryQueryParams, HistoryQueryResult } from '../types';

var API_BASE = import.meta.env.VITE_REGISTRY_API_URL ?? 'http://localhost:8088';
// console.log('history api base:', API_BASE)

interface StartQueryResponse {
  query_id: string;
}

const MAX_LIMIT = 5000

export function useHistoryService() {

  let startQuery = useCallback((params: HistoryQueryParams): Promise<string> => {
      return fetch(`${API_BASE}/history/query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          date: params.date,
          device_ids: params.deviceIds,
          columns: params.columns,
          limit: Math.min(params.limit ?? MAX_LIMIT, MAX_LIMIT),
        }),
      })
      .then(async (res) => {
        if (!res.ok) {
          var errBody = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
          let msg = (errBody as { error?: string }).error
          if (msg == null) {
            msg = `Query failed: ${res.status}`
          }
          throw new Error(msg);
        }
        return res.json() as Promise<StartQueryResponse>
      })
      .then((data) => data.query_id)
    }, []
  );


  const pollQuery = useCallback(
    async (queryId: string): Promise<HistoryQueryResult | null> => {
      try {
        var res = await fetch(`${API_BASE}/history/query/${queryId}`)
        if(!res.ok) return null;

        return (await res.json()) as HistoryQueryResult;
      } catch(_) {
        return null
      }
    },
    [],
  );

  return useMemo(() => ({ startQuery, pollQuery }), [startQuery, pollQuery])
}
