/* eslint-disable prefer-const */
/* eslint-disable no-var */
// YAML Modbus polling policies
import { useMemo } from 'react';
import type { PolicySummary, Policy } from '../types';
import { authHeaders } from '../utils/getToken';

var API_BASE = import.meta.env.VITE_POLICIES_API_URL ?? 'https://u6tudhckcg.execute-api.us-east-1.amazonaws.com';
// console.log('policies api:', API_BASE);

interface PoliciesApi {
  listPolicies: (token: string) => Promise<PolicySummary[]>;
  getPolicy: (name: string, token: string) => Promise<Policy>;
  savePolicy: (name: string, content: string, deviceIds: string[], token: string) => Promise<void>;
  deletePolicy: (name: string, token: string) => Promise<void>;
}

export function usePoliciesService(): PoliciesApi {
  return useMemo(() => {
    return {

    listPolicies: async (token: string): Promise<PolicySummary[]> => {
      let res = await fetch(`${API_BASE}/policies`, {
        method: 'GET',
        headers: authHeaders(token),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return (await res.json()) as PolicySummary[];
    },

      async getPolicy(name: string, token: string): Promise<Policy> {
        const res = await fetch(`${API_BASE}/policies/${name}`, {
          method: 'GET',
          headers: authHeaders(token),
        })
        if(!res.ok) throw new Error(`HTTP ${res.status}`);
        return (await res.json()) as Policy;
      },

    async savePolicy(
      name: string,
      content: string,
      deviceIds: string[],
      token: string
    ): Promise<void> {
        var res = await fetch(`${API_BASE}/policies/${name}`, {
          method: 'PUT',
          headers: authHeaders(token, true),
          body: JSON.stringify({ content: content, deviceIds: deviceIds }),
        });

        if (!res.ok) {
          throw new Error("HTTP " + res.status);
        }
    },

      // delete policy
      deletePolicy(name: string, token: string): Promise<void>{
        return fetch(API_BASE + '/policies/' + name, {
          method: 'DELETE',
          headers: { 'Authorization': 'Bearer ' + token },
        }).then((res) => {
          if (!res.ok) throw new Error(`${res.status}`)
        });
      },

    }
  }, []);
}
