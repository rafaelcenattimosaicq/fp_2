
import { render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { QueryProvider, useQuery } from './QueryContext';
import { MqttProvider } from './MqttContext';
import type { QueryRequest, Query } from '../types';

let capturedMqttHandlers: Array<(topic: string, payload: string) => void> = [];

const mockClient = {
  on: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  publish: vi.fn(),
  end: vi.fn(),
  connected: false,
};

vi.mock('mqtt', () => ({
  default: { connect: vi.fn(() => mockClient) },
  connect: vi.fn(() => mockClient),
}));

function makeRequest(overrides: Partial<QueryRequest> = {}): QueryRequest {
  return {
    source: 'telemetry_0x0007',
    fields: ['STATUS_ID_TEMP_AMB'],
    filters: [],
    aggregations: [],
    groupBy: [],
    window: null,
    devices: [],
    ...overrides,
  };
}

function TestConsumer() {
  const { queries, sources, loadingSources } = useQuery();
  return (
    <>
      <span data-testid="count">{queries.length}</span>
      <span data-testid="sources">{sources.length}</span>
      <span data-testid="loading">{String(loadingSources)}</span>
    </>
  );
}

function DeviceConsumer() {
  const { selectedDevices, setSelectedDevices } = useQuery();
  return (
    <>
      <span data-testid="devices">{selectedDevices.length}</span>
      <span data-testid="device-list">{selectedDevices.join(',')}</span>
      <button onClick={() => setSelectedDevices(['DEV-001', 'DEV-002'])}>select</button>
      <button onClick={() => setSelectedDevices(['DEV-003'])}>select-one</button>
      <button onClick={() => setSelectedDevices([])}>clear</button>
    </>
  );
}

function QueryLifecycleConsumer() {
  const { queries, submitQuery, removeQuery } = useQuery();
  return (
    <>
      <span data-testid="query-count">{queries.length}</span>
      <span data-testid="queries">{JSON.stringify(queries)}</span>
      <button
        data-testid="submit-btn"
        onClick={() => {
          void submitQuery(makeRequest());
        }}
      >
        submit
      </button>
      {queries.map((q) => (
        <button
          key={q.id}
          data-testid={`remove-${q.id}`}
          onClick={() => { void removeQuery(q.id); }}
        >
          remove-{q.id}
        </button>
      ))}
    </>
  );
}

describe('QueryContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedMqttHandlers = [];
    
    mockClient.on.mockImplementation((event: string, handler: (...args: unknown[]) => void) => {
      if (event === 'message') {
        capturedMqttHandlers.push((topic: string, payload: string) => {
          handler(topic, Buffer.from(payload));
        });
      }
    });
  });

  it('starts with no queries', () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve({ ok: false })));
    render(
      <MqttProvider>
        <QueryProvider>
          <TestConsumer />
        </QueryProvider>
      </MqttProvider>,
    );
    expect(screen.getByTestId('count').textContent).toBe('0');
  });

  it('keeps sources as an empty array when fetchSources fails', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new Error('Network error'))));
    render(
      <MqttProvider>
        <QueryProvider>
          <TestConsumer />
        </QueryProvider>
      </MqttProvider>,
    );
    await waitFor(() =>
      expect(screen.getByTestId('loading').textContent).toBe('false'),
    );
    expect(screen.getByTestId('sources').textContent).toBe('0');
  });

  it('keeps sources as an empty array when API returns unexpected shape', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve({ ok: true, json: () => Promise.resolve({}) })),
    );
    render(
      <MqttProvider>
        <QueryProvider>
          <TestConsumer />
        </QueryProvider>
      </MqttProvider>,
    );
    await waitFor(() =>
      expect(screen.getByTestId('loading').textContent).toBe('false'),
    );
    
    expect(screen.getByTestId('sources').textContent).toBe('0');
  });

  it('exposes selectedDevices state', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve({ ok: false })));
    render(
      <MqttProvider>
        <QueryProvider>
          <DeviceConsumer />
        </QueryProvider>
      </MqttProvider>,
    );
    expect(screen.getByTestId('devices').textContent).toBe('0');
  });

  it('setSelectedDevices updates the selected devices list', async () => {
    const user = userEvent.setup();
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve({ ok: false })));
    render(
      <MqttProvider>
        <QueryProvider>
          <DeviceConsumer />
        </QueryProvider>
      </MqttProvider>,
    );
    
    expect(screen.getByTestId('devices').textContent).toBe('0');

    await user.click(screen.getByText('select'));
    expect(screen.getByTestId('devices').textContent).toBe('2');
    expect(screen.getByTestId('device-list').textContent).toBe('DEV-001,DEV-002');
  });

  it('setSelectedDevices replaces previous selection', async () => {
    const user = userEvent.setup();
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve({ ok: false })));
    render(
      <MqttProvider>
        <QueryProvider>
          <DeviceConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await user.click(screen.getByText('select'));
    expect(screen.getByTestId('devices').textContent).toBe('2');

    await user.click(screen.getByText('select-one'));
    expect(screen.getByTestId('devices').textContent).toBe('1');
    expect(screen.getByTestId('device-list').textContent).toBe('DEV-003');
  });

  it('setSelectedDevices can clear the selection back to empty', async () => {
    const user = userEvent.setup();
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve({ ok: false })));
    render(
      <MqttProvider>
        <QueryProvider>
          <DeviceConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await user.click(screen.getByText('select'));
    expect(screen.getByTestId('devices').textContent).toBe('2');
    await user.click(screen.getByText('clear'));
    expect(screen.getByTestId('devices').textContent).toBe('0');
  });

  it('submitQuery adds a query to the list', async () => {
    const user = userEvent.setup();
    
    const mockFetch = vi.fn()
      .mockResolvedValueOnce({ ok: false })                          
      .mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ queryId: 42 }),
      });
    vi.stubGlobal('fetch', mockFetch);

    render(
      <MqttProvider>
        <QueryProvider>
          <QueryLifecycleConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId('query-count').textContent).toBe('0');
    });

    await user.click(screen.getByTestId('submit-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('query-count').textContent).toBe('1');
    });

    const queries: Query[] = JSON.parse(screen.getByTestId('queries').textContent as string) as Query[];
    expect(queries).toHaveLength(1);
    expect(queries[0].status).toBe('pending');
    expect(queries[0].coordinatorQueryId).toBe(42);
    expect(queries[0].results).toEqual([]);
    expect(queries[0].error).toBeNull();
    expect(queries[0].request.source).toBe('telemetry_0x0007');
  });

  it('removeQuery removes', async () => {
    const user = userEvent.setup();
    
    const mockFetch = vi.fn()
      .mockResolvedValueOnce({ ok: false })                          
      .mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ queryId: 99 }),
      })                                                              
      .mockResolvedValueOnce({ ok: true });                           
    vi.stubGlobal('fetch', mockFetch);

    render(
      <MqttProvider>
        <QueryProvider>
          <QueryLifecycleConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await user.click(screen.getByTestId('submit-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('query-count').textContent).toBe('1');
    });

    const queries: Query[] = JSON.parse(screen.getByTestId('queries').textContent as string) as Query[];
    const queryId = queries[0].id;

    await user.click(screen.getByTestId(`remove-${queryId}`));

    await waitFor(() => {
      expect(screen.getByTestId('query-count').textContent).toBe('0');
    });
  });

  it('submitQuery stores a failed query when validation blocks submission', async () => {
    const user = userEvent.setup();
    
    const sourcesPayload = [
      { telemetry_0x0007: 'DEVICE_ID:TEXT timestamp:UINT64 STATUS_ID_LABEL:TEXT STATUS_ID_TEMP_AMB:FLOAT64' },
    ];
    const mockFetch = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(sourcesPayload),
      });
    vi.stubGlobal('fetch', mockFetch);

    function ValidationConsumer() {
      const { queries, submitQuery } = useQuery();
      return (
        <>
          <span data-testid="q-count">{queries.length}</span>
          <span data-testid="q-data">{JSON.stringify(queries)}</span>
          <button
            data-testid="submit-bad"
            onClick={() => {
              void submitQuery(
                makeRequest({
                  filters: [{ field: 'STATUS_ID_LABEL', operator: '=', value: 'ok' }],
                }),
              );
            }}
          >
            submit bad
          </button>
        </>
      );
    }

    render(
      <MqttProvider>
        <QueryProvider>
          <ValidationConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await waitFor(() => {
      
    });

    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    await user.click(screen.getByTestId('submit-bad'));

    await waitFor(() => {
      expect(screen.getByTestId('q-count').textContent).toBe('1');
    });

    const queries: Query[] = JSON.parse(screen.getByTestId('q-data').textContent as string) as Query[];
    expect(queries[0].status).toBe('failed');
    expect(queries[0].error).toContain('TEXT field');
    
    expect(mockFetch).toHaveBeenCalledTimes(1); 
  });

  it('fetches and filters', async () => {
    const sourcesPayload = [
      
      { default_logical: 'id:INTEGER(32 bits)' },
      
      { telemetry: 'DEVICE_ID:TEXT GATEWAY_ID:TEXT timestamp:UINT64' },
      
      { telemetry_0x0007: 'DEVICE_ID:TEXT GATEWAY_ID:TEXT timestamp:UINT64 STATUS_ID_TEMP_AMB:FLOAT64' },
    ];
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve({
          ok: true,
          json: () => Promise.resolve(sourcesPayload),
        }),
      ),
    );

    render(
      <MqttProvider>
        <QueryProvider>
          <TestConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId('loading').textContent).toBe('false'),
    );
    expect(screen.getByTestId('sources').textContent).toBe('1');
  });

  it('transitions query from pending to running when MQTT results arrive', async () => {
    const user = userEvent.setup();
    
    const mockFetch = vi.fn()
      .mockResolvedValueOnce({ ok: false })
      .mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ queryId: 50 }),
      });
    vi.stubGlobal('fetch', mockFetch);

    render(
      <MqttProvider>
        <QueryProvider>
          <QueryLifecycleConsumer />
        </QueryProvider>
      </MqttProvider>,
    );

    await user.click(screen.getByTestId('submit-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('query-count').textContent).toBe('1');
    });

    const queries: Query[] = JSON.parse(screen.getByTestId('queries').textContent as string) as Query[];
    const resultId = queries[0].id;
    expect(queries[0].status).toBe('pending');

    for (const handler of capturedMqttHandlers) {
      act(() => {
        handler(
          `nebulastream/results/${resultId}`,
          JSON.stringify({ STATUS_ID_TEMP_AMB: 22.5, timestamp: 1000 }),
        );
      });
    }

    await waitFor(() => {
      const updated: Query[] = JSON.parse(screen.getByTestId('queries').textContent as string) as Query[];
      expect(updated[0].status).toBe('running');
      expect(updated[0].results).toHaveLength(1);
      expect(updated[0].results[0]).toHaveProperty('STATUS_ID_TEMP_AMB', 22.5);
    });
  });

  it('throws when useQuery is used outside QueryProvider', () => {
    
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    function Orphan() {
      useQuery();
      return null;
    }
    expect(() => render(<Orphan />)).toThrow('useQuery must be used within a QueryProvider');
    spy.mockRestore();
  });
});
