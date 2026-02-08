import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryEngine } from './QueryEngine';
import { useQuery } from '../../contexts/QueryContext';

vi.mock('../../contexts/QueryContext', () => ({
  useQuery: vi.fn(() => ({
    sources: [
      { name: 'refrigeration_telemetry', fields: ['TEMPERATURE', 'VOLTAGE'], fieldTypes: { TEMPERATURE: 'FLOAT64', VOLTAGE: 'FLOAT64' } },
    ],
    queries: [],
    submitQuery: vi.fn(),
    removeQuery: vi.fn(),
    loadingSources: false,
    selectedDevices: [],
    setSelectedDevices: vi.fn(),
  })),
}));

vi.mock('../../contexts/TelemetryContext', () => ({
  useTelemetry: vi.fn(() => ({
    devices: [
      { id: 'DEV-001', name: 'Compressor-A', lastSeen: Date.now(), online: true, lastPoint: null },
    ],
    buffers: new Map(),
    selectedDeviceId: null,
    selectDevice: vi.fn(),
  })),
}));

describe('QueryEngine', () => {
  
  it('shows accent stripe', () => {
    render(<QueryEngine />);
    
    expect(screen.getByLabelText(/select devices/i)).toBeInTheDocument();
  });

  it('renders the device selector', () => {
    render(<QueryEngine />);
    
    expect(screen.getByLabelText(/select devices/i)).toBeInTheDocument();
  });

  it('renders the source dropdown', () => {
    render(<QueryEngine />);
    expect(screen.getByText('refrigeration_telemetry')).toBeInTheDocument();
  });

  it('shows empty results message', () => {
    render(<QueryEngine />);
    expect(screen.getByText(/submit a query/i)).toBeInTheDocument();
  });

  it('passes multiple sources to QueryForm for join support', () => {
    
    vi.mocked(useQuery).mockReturnValueOnce({
      sources: [
        { name: 'telemetry_0x0007', fields: ['TEMPERATURE', 'timestamp'], fieldTypes: { TEMPERATURE: 'FLOAT64', timestamp: 'UINT64' } },
        { name: 'telemetry_0x0008', fields: ['TEMPERATURE', 'timestamp'], fieldTypes: { TEMPERATURE: 'FLOAT64', timestamp: 'UINT64' } },
      ],
      queries: [],
      submitQuery: vi.fn(),
      removeQuery: vi.fn(),
      loadingSources: false,
      selectedDevices: [],
      setSelectedDevices: vi.fn(),
    });
    render(<QueryEngine />);
    
    expect(screen.getByText('telemetry_0x0007')).toBeInTheDocument();
    expect(screen.getByText('telemetry_0x0008')).toBeInTheDocument();
  });
});
