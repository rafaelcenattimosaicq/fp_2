import { useMemo } from 'react';
import type { VpnRequest, GatewayRegistration, RegisteredGateway } from '../types';
import { authHeaders } from '../utils/getToken';

// vpn approval api
const API_BASE = (import.meta.env.VITE_VPN_API_URL as string | undefined) ?? '';
// console.log('vpn api configured:', API_BASE.length > 0)
export const isVpnApiConfigured = API_BASE.length > 0;

interface VpnApi {
  listRequests: (token: string) => Promise<VpnRequest[]>;
  approveRequest: (gatewayId: string, token: string) => Promise<void>;
  getStatus: (gatewayId: string, token: string) => Promise<Record<string, unknown> | null>;
  revokeAccess: (gatewayId: string, token: string) => Promise<void>;
  registerGateway: (
    data: GatewayRegistration,
    token: string,
  ) => Promise<{ gateway_id: string; pre_shared_secret: string }>;
  listGateways: (token: string) => Promise<RegisteredGateway[]>;
  deleteGateway: (gatewayId: string, token: string) => Promise<void>;
  getDownloadUrl: (
    arch: string,
    token: string,
  ) => Promise<{ download_url: string; architecture: string }>;
}

export function useVpnService(): VpnApi {
  return useMemo(() => ({

    listRequests(token: string): Promise<VpnRequest[]> {
      return fetch(`${API_BASE}/vpn/requests`, {
        method: 'GET',
        headers: authHeaders(token),
      })
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<{ requests: VpnRequest[] }>;
      })
      .then((data) => data.requests);
    },

    async approveRequest(gatewayId: string, token: string): Promise<void> {
        var res = await fetch(`${API_BASE}/vpn/approve/${gatewayId}`, {
          method: 'POST',
          headers: authHeaders(token, true),
        });

        if (res.ok == false) {
          const body = await res.text();
          throw new Error(`could not approve VPN request for "${gatewayId}": ${res.status} ${body}`);
        }
    },

    async getStatus(gatewayId: string, token: string): Promise<Record<string, unknown> | null> {
      try {
        var res = await fetch(`${API_BASE}/vpn/status/${gatewayId}`, {
          headers: authHeaders(token),
        });
        if (!res.ok) return null;
        return (await res.json()) as Record<string, unknown>;
      } catch {
        return null;
      }
    },

    revokeAccess(gatewayId: string, token: string): Promise<void> {
      return fetch(`${API_BASE}/vpn/revoke/${gatewayId}`, {
        method: 'DELETE',
        headers: authHeaders(token),
      }).then((res) => {
        if (!res.ok) {
          throw new Error(`could not revoke VPN access for "${gatewayId}": ${res.status}`);
        }
      });
    },

    // register a new gateway in vpn
    async registerGateway(data: GatewayRegistration, token: string): Promise<{ gateway_id: string; pre_shared_secret: string }> {
      var res = await fetch(`${API_BASE}/vpn/gateways`, {
        method: 'POST',
        headers: authHeaders(token, true),
        body: JSON.stringify(data),
      });
      if (!res.ok) {
        let errText = '';
        try { errText = await res.text(); } catch { errText = ''; }

        let msg = `HTTP ${res.status}`;
        if (errText.length > 0) msg = `HTTP ${res.status} - ${errText}`;
        throw new Error(msg);
      }

      return (await res.json()) as { gateway_id: string; pre_shared_secret: string };
    },

    listGateways(token: string): Promise<RegisteredGateway[]> {
        return fetch(`${API_BASE}/vpn/gateways`, {
          method: 'GET',
          headers: authHeaders(token),
        }).then((res) => {
          if (!res.ok) throw new Error(res.status + "");
          return res.json() as Promise<{ gateways: RegisteredGateway[] }>;
        }).then((data) => {
            return data.gateways;
        });
    },

    async deleteGateway(gatewayId: string, token: string): Promise<void> {
      const res = await fetch(`${API_BASE}/vpn/gateways/${gatewayId}`, {
        method: 'DELETE',
        headers: authHeaders(token),
      });
      if (res.ok == false) {
        throw new Error(`could not delete gateway "${gatewayId}": ${res.status}`);
      }
    },

    async getDownloadUrl(arch: string, token: string): Promise<{ download_url: string; architecture: string }> {
      const res = await fetch(`${API_BASE}/vpn/download/${arch}`, {
          method: 'GET',
          headers: authHeaders(token),
      });

      if (!res.ok) throw new Error(`${res.status}`);

      return (await res.json()) as { download_url: string; architecture: string };
    },

  }), []);
}
