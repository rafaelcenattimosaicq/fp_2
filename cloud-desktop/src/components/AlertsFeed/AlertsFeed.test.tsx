import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AlertsFeed } from './AlertsFeed';

vi.mock('../../contexts/AlertsContext', () => ({
  useAlerts: vi.fn(() => ({
    feed: [
      {
        type: 'alert',
        data: {
          id: 'a1',
          deviceId: 'dev-001',
          ruleId: 'temp-high',
          message: 'Temperature exceeded threshold',
          severity: 'critical',
          timestamp: Date.now(),
        },
      },
      {
        type: 'command',
        data: {
          id: 'c1',
          deviceId: 'dev-001',
          ruleId: 'temp-high',
          command: 'reduce_speed',
          timestamp: Date.now(),
        },
      },
    ],
    clearFeed: vi.fn(),
    rules: [],
    addRule: vi.fn(),
    removeRule: vi.fn(),
  })),
}));

vi.mock('../../contexts/QueryContext', () => ({
  useQuery: vi.fn(() => ({
    sources: [],
    queries: [],
    submitQuery: vi.fn(),
    removeQuery: vi.fn(),
    loadingSources: false,
    selectedDevices: [],
    setSelectedDevices: vi.fn(),
  })),
}));

describe('AlertsFeed', () => {
  
  it('renders alert items', () => {
    render(<AlertsFeed />);
    expect(screen.getByText(/temperature exceeded/i)).toBeInTheDocument();
  });

  it('renders command items', () => {
    render(<AlertsFeed />);
    expect(screen.getByText(/reduce_speed/i)).toBeInTheDocument();
  });

  it('shows the item count', () => {
    render(<AlertsFeed />);
    expect(screen.getByText(/2 items/i)).toBeInTheDocument();
  });
});
