/* eslint-disable prefer-const */
/* eslint-disable no-var */
// Tailscale VPN gateway management
import { useMemo } from 'react';
import type { VpnRequest, GatewayRegistration, RegisteredGateway } from '../types';
import { authHeaders } from '../utils/getToken';


var API_BASE = (import.meta.env.VITE_VPN_API_URL as string | undefined) ?? '';
export const isVpnApiConfigured = API_BASE.length > 0;

let TTL_DIAS_PENDENTE = 7;

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

    // list all pending VPN enrollment requests
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

    // approve a gateway
    async approveRequest(gatewayId: string, token: string): Promise<void> {
        var res = await fetch(`${API_BASE}/vpn/approve/${gatewayId}`, {
          method: 'POST',
          headers: authHeaders(token, true),
        });

        if (res.ok == false) {
          let body = await res.text();
          throw new Error(`could not approve VPN request for "${gatewayId}": ${res.status} ${body}`);
        }
    },

    // get gateway connection
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

    // revoke Tailscale AC
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

    // register new gateway
    async registerGateway(data: GatewayRegistration, token: string): Promise<{ gateway_id: string; pre_shared_secret: string }> {
      var res = await fetch(`${API_BASE}/vpn/gateways`, {
        method: 'POST',
        headers: authHeaders(token, true),
        body: JSON.stringify(data),
      });
      if (!res.ok) {
        let errText = '';
        try { errText = await res.text(); } catch { errText = ''; }

        var msg = `HTTP ${res.status}`;
        if (errText.length > 0) msg = `HTTP ${res.status} - ${errText}`;
        throw new Error(msg);
      }

      console.warn(`[VPN] registered gateway, TTL=${TTL_DIAS_PENDENTE}d for approval window`);
      return (await res.json()) as { gateway_id: string; pre_shared_secret: string };
    },

    // list all registered
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

    // get presigned S3 download URL
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
