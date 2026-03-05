/* eslint-disable no-empty */
/* eslint-disable @typescript-eslint/no-unused-vars */
import { describe, it, expect, afterAll, beforeAll } from 'vitest';
import { getTestToken } from '../hooks/test-helpers/getTestToken';
import { execSync, spawn, type ChildProcess } from 'node:child_process';
import { resolve } from 'node:path';

const NES_BASE = process.env.NES_BASE_URL
  ?? (import.meta.env.VITE_NES_API_URL as string | undefined)
  ?? 'http://localhost:8081';

const VPN_API = (import.meta.env.VITE_VPN_API_URL as string | undefined)
  ?? 'https://87dzpnwsh9.execute-api.us-east-1.amazonaws.com';

const GATEWAY_BIN = resolve(__dirname, '../../../services/gateway/target/release/gateway');

const GATEWAY_CONFIG = resolve(__dirname, '../../../services/gateway/gateway.yaml');

const GATEWAY_ID = 'GW-EDGE-001';

const WORKER_REGISTRATION_TIMEOUT = 150_000;

const QUERY_LIFECYCLE_WAIT = 8_000;

let gatewayProcess: ChildProcess | null = null;
let mosquittoStarted = false;

function ensureMosquitto(): void {
  try {
    execSync('pgrep -x mosquitto', { stdio: 'ignore' });
  } catch {
    execSync('mosquitto -d -p 1883');
    mosquittoStarted = true;
  }
}

function stopMosquitto(): void {
  if (mosquittoStarted) {
    try { execSync('pkill -x mosquitto'); } catch {  }
  }
}

async function pollUntil(
  fn: () => Promise<boolean>,
  timeoutMs: number,
  intervalMs = 3000,
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await fn()) return true;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  return false;
}

async function hasRegisteredWorker(): Promise<boolean> {
  try {
    const res = await fetch(`${NES_BASE}/v1/nes/topology`);
    if (!res.ok) return false;
    const data = (await res.json()) as {
      nodes: Array<{ id: number }>;
      edges: Array<{ source: number; target: number }>;
    };
    return data.nodes.length >= 2 && data.edges.length >= 1;
  } catch {
    return false;
  }
}

async function approveVpnRequests(token: string): Promise<boolean> {
  try {
    const res = await fetch(`${VPN_API}/vpn/requests`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) return false;

    const { requests } = (await res.json()) as {
      requests: Array<{ gateway_id: string; status: string }>;
    };

    const pending = requests.filter(
      (r) => r.gateway_id === GATEWAY_ID && r.status === 'pending',
    );

    for (const req of pending) {
      const approveRes = await fetch(`${VPN_API}/vpn/approve/${req.gateway_id}`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
      });
      if (approveRes.ok) {
        return true;
      }
    }

    return pending.length === 0;
  } catch (e) {
    return false;
  }
}

describe('Full E2E pipeline: Gateway → Cloud', () => {
  let token: string;

  beforeAll(async () => {
    token = await getTestToken();
  }, 30_000);

  afterAll(() => {
    if (gatewayProcess) {
      gatewayProcess.kill('SIGTERM');
      gatewayProcess = null;
    }
    stopMosquitto();
  });

  it('coordinator is healthy', async () => {
    const res = await fetch(`${NES_BASE}/v1/nes/connectivity/check`);
    expect(res.ok).toBe(true);
    const data = (await res.json()) as { success: boolean };
    expect(data.success).toBe(true);
  });

  it('starts gateway', async () => {
    ensureMosquitto();

    gatewayProcess = spawn(GATEWAY_BIN, [GATEWAY_CONFIG, '--headless'], {
      env: { ...process.env, RUST_LOG: 'gateway=info' },
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    gatewayProcess.stdout?.on('data', (chunk: Buffer) => {
      const line = chunk.toString().trim();
      if (line) console.log(`  [gateway] ${line}`);
    });
    gatewayProcess.stderr?.on('data', (chunk: Buffer) => {
      const line = chunk.toString().trim();
      if (line) console.log(`  [gateway] ${line}`);
    });

    await new Promise((r) => setTimeout(r, 3000));

    expect(gatewayProcess.exitCode).toBeNull();
  }, 10_000);

  it('approves gateway VPN request', async () => {
    const approved = await pollUntil(
      () => approveVpnRequests(token),
      30_000,
      5000,
    );
    expect(approved).toBe(true);
  }, 60_000);

  it('NES worker registers with coordinator', async () => {
    console.log('  Waiting for NES worker to register (up to 2 minutes)...');
    const registered = await pollUntil(
      hasRegisteredWorker,
      WORKER_REGISTRATION_TIMEOUT,
      5000,
    );
    expect(registered).toBe(true);
  }, WORKER_REGISTRATION_TIMEOUT + 10_000);

  it('source catalog has telemetry source', async () => {
    const res = await fetch(`${NES_BASE}/v1/nes/sourceCatalog/allLogicalSource`);
    expect(res.ok).toBe(true);
    const body = await res.text();
    expect(body).toContain('telemetry');
  });

  it('distributed query completes full lifecycle', async () => {
    const submitRes = await fetch(`${NES_BASE}/v1/nes/query/execute-query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        userQuery:
          'Query::from("default_logical").filter(Attribute("id") > 0).sink(PrintSinkDescriptor::create());',
        placement: 'BottomUp',
      }),
    });
    expect(submitRes.ok).toBe(true);

    const { queryId } = (await submitRes.json()) as { queryId: number };
    expect(queryId).toBeDefined();

    await new Promise((r) => setTimeout(r, QUERY_LIFECYCLE_WAIT));

    const planRes = await fetch(
      `${NES_BASE}/v1/nes/query/query-plan?queryId=${queryId}`,
    );
    expect(planRes.ok).toBe(true);

    const plan = (await planRes.json()) as {
      queryId: number;
      status: string;
      history: Array<{ queryState: string }>;
    };

    const states = plan.history.map((h) => h.queryState);
    expect(states).toContain('REGISTERED');

    const hasExecuted =
      states.includes('RUNNING') || states.includes('STOPPED');
    expect(hasExecuted).toBe(true);
  }, QUERY_LIFECYCLE_WAIT + 10_000);
});
