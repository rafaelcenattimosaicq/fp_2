import { render, screen, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { AlertsProvider, useAlerts } from './AlertsContext';
import { MqttProvider } from './MqttContext';
import type { AlertRule } from '../types';

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

function TestConsumer() {
  const { feed } = useAlerts();
  return <span data-testid="count">{feed.length}</span>;
}

function FullTestConsumer() {
  const { feed, clearFeed, rules, addRule, removeRule } = useAlerts();

  return (
    <div>
      <span data-testid="feed-count">{feed.length}</span>
      <span data-testid="rules-count">{rules.length}</span>
      <ul data-testid="rules-list">
        {rules.map((r: AlertRule) => (
          <li key={r.id} data-testid={`rule-${r.id}`}>
            {r.source}:{r.field} {r.operator} {r.threshold} active={String(r.active)}
          </li>
        ))}
      </ul>
      <ul data-testid="feed-list">
        {feed.map((item, idx) => (
          <li key={idx} data-testid={`feed-${idx}`}>
            {item.type}:{item.data.id}
          </li>
        ))}
      </ul>
      <button
        data-testid="add-rule"
        onClick={() => addRule('telemetry_0x0007', 'STATUS_ID_TEMP_CABINET', '>', '50')}
      >
        Add Rule
      </button>
      <button
        data-testid="add-rule-2"
        onClick={() => addRule('telemetry_0x0008', 'STATUS_ID_COMP_SPEED', '<', '1000')}
      >
        Add Rule 2
      </button>
      <button data-testid="clear-feed" onClick={clearFeed}>
        Clear Feed
      </button>
      {rules.length > 0 && (
        <button
          data-testid="remove-first-rule"
          onClick={() => removeRule(rules[0].id)}
        >
          Remove First Rule
        </button>
      )}
    </div>
  );
}

function getMessageHandler(): (topic: string, payload: Buffer) => void {
  const call = (
    mockClient.on.mock.calls as [string, (...args: unknown[]) => void][]
  ).find(([event]) => event === 'message');
  if (!call) throw new Error('No message handler registered on MQTT client');
  return call[1] as (topic: string, payload: Buffer) => void;
}

describe('AlertsContext', () => {
  beforeEach(() => {
    vi.clearAllMocks(); vi.useFakeTimers({ shouldAdvanceTime: true });
    global.fetch = vi.fn();
  });

  afterEach(() => {
    vi.restoreAllMocks(); vi.useRealTimers();
  });

  it('starts with an empty feed', () => {
    render(
      <MqttProvider>
        <AlertsProvider>
          <TestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );
    expect(screen.getByTestId('count').textContent).toBe('0');
  });

  it('adds an alert', () => {
    render(
      <MqttProvider>
        <AlertsProvider>
          <TestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    const messageHandler = getMessageHandler();

    act(() => {
      messageHandler(
        'alerts/device-001',
        Buffer.from(
          JSON.stringify({
            device_id: 'device-001',
            rule_id: 'temp-high',
            message: 'Temperature exceeded threshold',
            severity: 'critical',
          }),
        ),
      );
    });

    expect(screen.getByTestId('count').textContent).toBe('1');
  });

  it('addRule adds a rule', async () => {
    (global.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ queryId: 42 }),
    });

    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    expect(screen.getByTestId('rules-count').textContent).toBe('0');

    await act(async () => {
      screen.getByTestId('add-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('1');
    });

    expect(global.fetch).toHaveBeenCalledTimes(1);
    const [url] = (global.fetch as ReturnType<typeof vi.fn>).mock.calls[0] as [string];
    expect(url).toContain('/v1/nes/query/execute-query');
  });

  it('removeRule removes a rule by ID', async () => {
    (global.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ queryId: 42 }),
    });

    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    await act(async () => {
      screen.getByTestId('add-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('1');
    });

    (global.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ ok: true });

    await act(async () => {
      screen.getByTestId('remove-first-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('0');
    });
  });

  it('clearFeed empties the alerts array', () => {
    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    const messageHandler = getMessageHandler();

    act(() => {
      messageHandler(
        'alerts/device-001',
        Buffer.from(
          JSON.stringify({
            device_id: 'device-001',
            rule_id: 'r1',
            message: 'alert one',
            severity: 'info',
          }),
        ),
      );
      messageHandler(
        'alerts/device-002',
        Buffer.from(
          JSON.stringify({
            device_id: 'device-002',
            rule_id: 'r2',
            message: 'alert two',
            severity: 'warning',
          }),
        ),
      );
    });

    expect(screen.getByTestId('feed-count').textContent).toBe('2');

    act(() => {
      screen.getByTestId('clear-feed').click();
    });

    expect(screen.getByTestId('feed-count').textContent).toBe('0');
  });

  it('triggers an alert feed item when a NES threshold is crossed', async () => {
    (global.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ queryId: 99 }),
    });

    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    const messageHandler = getMessageHandler();

    await act(async () => {
      screen.getByTestId('add-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('1');
    });

    const ruleItem = screen.getByTestId('rules-list').querySelector('li');
    expect(ruleItem).toBeTruthy();
    const ruleTestId = (ruleItem as HTMLElement).getAttribute('data-testid') as string;
    const ruleId = ruleTestId.replace('rule-', '');

    act(() => {
      messageHandler(
        `nebulastream/alerts/${ruleId}`,
        Buffer.from(
          JSON.stringify({
            'telemetry_0x0007$STATUS_ID_TEMP_CABINET': 55,
            'telemetry_0x0007$DEVICE_ID': 'device-001',
            'telemetry_0x0007$timestamp': Date.now(),
          }),
        ),
      );
    });

    await waitFor(() => {
      expect(screen.getByTestId('feed-count').textContent).toBe('1');
    });
  });

  it('supports multiple rules simultaneously', async () => {
    (global.fetch as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ queryId: 10 }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ queryId: 20 }),
      });

    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    await act(async () => {
      screen.getByTestId('add-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('1');
    });

    await act(async () => {
      screen.getByTestId('add-rule-2').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('2');
    });

    const ruleItems = screen.getByTestId('rules-list').querySelectorAll('li');
    expect(ruleItems.length).toBe(2);
    expect(ruleItems[0].textContent).toContain('telemetry_0x0007');
    expect(ruleItems[1].textContent).toContain('telemetry_0x0008');
  });

  it('adds a command to the feed when one arrives on commands/#', () => {
    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    const messageHandler = getMessageHandler();

    act(() => {
      messageHandler(
        'commands/device-001',
        Buffer.from(
          JSON.stringify({
            device_id: 'device-001',
            rule_id: 'cmd-1',
            command: 'restart',
          }),
        ),
      );
    });

    expect(screen.getByTestId('feed-count').textContent).toBe('1');
  });

  it('adds an inactive rule and critical feed alert when API call fails', async () => {
    (global.fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: false,
      status: 500,
    });

    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    await act(async () => {
      screen.getByTestId('add-rule').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('rules-count').textContent).toBe('1');
    });

    const ruleItem = screen.getByTestId('rules-list').querySelector('li');
    expect(ruleItem?.textContent).toContain('active=false');

    expect(screen.getByTestId('feed-count').textContent).toBe('1');
  });

  it('ignores malformed alert payloads', () => {
    render(
      <MqttProvider>
        <AlertsProvider>
          <FullTestConsumer />
        </AlertsProvider>
      </MqttProvider>,
    );

    const messageHandler = getMessageHandler();

    act(() => {
      messageHandler(
        'alerts/device-001',
        Buffer.from('not valid json {{{'),
      );
    });

    expect(screen.getByTestId('feed-count').textContent).toBe('0');
  });
});
