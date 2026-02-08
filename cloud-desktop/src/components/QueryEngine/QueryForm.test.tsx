import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryForm } from './QueryForm';

const mockSources = [
  { name: 'refrigeration_telemetry', fields: ['TEMPERATURE', 'VOLTAGE', 'DEVICE_ID'] },
];

const twoSources = [
  { name: 'telemetry_0x0007', fields: ['STATUS_ID_TEMP_CABINET', 'timestamp'] },
  { name: 'telemetry_0x0008', fields: ['STATUS_ID_TEMP_CABINET', 'timestamp'] },
];

describe('QueryForm', () => {
  
  it('shows source dropdown', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    expect(screen.getByText('refrigeration_telemetry')).toBeInTheDocument();
  });

  it('shows fields after selecting a source', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'refrigeration_telemetry' } });
    expect(screen.getAllByText('TEMPERATURE').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('VOLTAGE').length).toBeGreaterThanOrEqual(1);
  });

  it('disables submit when no source is selected', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    expect(screen.getByRole('button', { name: /run query/i })).toBeDisabled();
  });

  it('shows Run Query button when source is selected', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'refrigeration_telemetry' } });
    expect(screen.getByRole('button', { name: /run query/i })).not.toBeDisabled();
  });

  it('includes window configuration', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    expect(screen.getByText(/window/i)).toBeInTheDocument();
  });

  it('shows join section', () => {
    render(
      <QueryForm
        sources={twoSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'telemetry_0x0007' } });
    expect(screen.getByText(/join with/i)).toBeInTheDocument();
  });

  it('shows other sources in join dropdown', () => {
    render(
      <QueryForm
        sources={twoSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'telemetry_0x0007' } });
    expect(screen.getByLabelText(/join source/i)).toBeInTheDocument();
  });

  it('defaults join key to timestamp', () => {
    render(
      <QueryForm
        sources={twoSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'telemetry_0x0007' } });
    fireEvent.change(screen.getByLabelText(/join source/i), { target: { value: 'telemetry_0x0008' } });
    const joinKeySelect = screen.getByLabelText(/join key/i) as HTMLSelectElement;
    expect(joinKeySelect.value).toBe('timestamp');
  });

  it('hides join section with only', () => {
    render(
      <QueryForm
        sources={mockSources}
        onSubmit={vi.fn()}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'refrigeration_telemetry' } });
    expect(screen.queryByText(/join with/i)).not.toBeInTheDocument();
  });

  it('includes join fields in submitted request', () => {
    const onSubmit = vi.fn();
    render(
      <QueryForm
        sources={twoSources}
        onSubmit={onSubmit}
        submitting={false}
      />,
    );
    fireEvent.change(screen.getByLabelText(/source/i), { target: { value: 'telemetry_0x0007' } });
    
    fireEvent.change(screen.getByLabelText(/join source/i), { target: { value: 'telemetry_0x0008' } });
    
    fireEvent.click(screen.getByRole('button', { name: /run query/i }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        source: 'telemetry_0x0007',
        joinSource: 'telemetry_0x0008',
        joinKey: { left: 'timestamp', right: 'timestamp' },
      }),
    );
  });
});
