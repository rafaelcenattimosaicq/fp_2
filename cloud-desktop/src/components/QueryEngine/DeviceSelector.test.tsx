import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { DeviceSelector } from './DeviceSelector';
import type { Device } from '../../types';

const mockDevices: Device[] = [
  { id: 'DEV-001', name: 'Compressor-A', lastSeen: Date.now(), online: true, lastPoint: null },
  { id: 'DEV-002', name: 'Compressor-B', lastSeen: Date.now(), online: true, lastPoint: null },
  { id: 'DEV-003', name: 'Evaporator-C', lastSeen: Date.now() - 60000, online: false, lastPoint: null },
];

describe('DeviceSelector', () => {
  it('shows selection summary', () => {
    render(
      <DeviceSelector
        devices={mockDevices}
        selectedIds={['DEV-001']}
        onSelectionChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/1 device/i)).toBeInTheDocument();
  });

  it('shows device list', () => {
    render(
      <DeviceSelector
        devices={mockDevices}
        selectedIds={[]}
        onSelectionChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /device/i }));
    expect(screen.getByText('DEV-001')).toBeInTheDocument();
    expect(screen.getByText('DEV-002')).toBeInTheDocument();
    expect(screen.getByText('DEV-003')).toBeInTheDocument();
  });

  it('calls onSelectionChange when a device is toggled', () => {
    const onChange = vi.fn();
    render(
      <DeviceSelector
        devices={mockDevices}
        selectedIds={['DEV-001']}
        onSelectionChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /device/i }));
    fireEvent.click(screen.getByLabelText('DEV-002'));
    expect(onChange).toHaveBeenCalledWith(['DEV-001', 'DEV-002']);
  });

  it('filters devices by search text', () => {
    render(
      <DeviceSelector
        devices={mockDevices}
        selectedIds={[]}
        onSelectionChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /device/i }));
    fireEvent.change(screen.getByPlaceholderText(/search/i), { target: { value: 'Evaporator' } });
    expect(screen.getByText('DEV-003')).toBeInTheDocument();
    expect(screen.queryByText('DEV-001')).not.toBeInTheDocument();
  });

  it('shows online/offline status indicators', () => {
    render(
      <DeviceSelector
        devices={mockDevices}
        selectedIds={[]}
        onSelectionChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /device/i }));
    const onlineDots = screen.getAllByTitle('online');
    const offlineDots = screen.getAllByTitle('offline');
    expect(onlineDots.length).toBe(2);
    expect(offlineDots.length).toBe(1);
  });
});
