import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { DeviceFleet } from './DeviceFleet';

vi.mock('../../contexts/TelemetryContext', () => ({
  useTelemetry: vi.fn(() => ({
    devices: [
      { id: 'dev-001', name: 'dev-001', lastSeen: Date.now(), online: false, lastPoint: null },
      { id: 'dev-002', name: 'dev-002', lastSeen: Date.now() - 30000, online: false, lastPoint: null },
    ],
    selectedDeviceId: null,
    selectDevice: vi.fn(),
    buffers: new Map(),
  })),
}));

describe('DeviceFleet', () => {
  it('renders a card for each device', () => {
    render(<DeviceFleet />);
    expect(screen.getByText('dev-001')).toBeInTheDocument();
    expect(screen.getByText('dev-002')).toBeInTheDocument();
  });

  it('shows the panel title', () => {
    render(<DeviceFleet />);
    expect(screen.getByText(/device fleet/i)).toBeInTheDocument();
  });
});
