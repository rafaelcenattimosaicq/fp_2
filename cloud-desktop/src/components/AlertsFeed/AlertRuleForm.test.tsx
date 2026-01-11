
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AlertRuleForm } from './AlertRuleForm';

const mockSources = [
  {
    name: 'compressor_events',
    fields: ['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'STATUS_ID_TEMP_CABINET', 'STATUS_ID_COMP_SPEED'],
    fieldTypes: {} as Record<string, string>,
  },
  {
    name: 'alerts_log',
    fields: ['DEVICE_ID', 'MESSAGE', 'SEVERITY'],
    fieldTypes: {} as Record<string, string>,
  },
];

const mockAddRule = vi.fn().mockResolvedValue(undefined);

vi.mock('../../contexts/QueryContext', () => ({
  useQuery: vi.fn(() => ({
    sources: mockSources,
    queries: [],
    submitQuery: vi.fn(),
    removeQuery: vi.fn(),
    loadingSources: false,
    selectedDevices: [],
    setSelectedDevices: vi.fn(),
  })),
}));

vi.mock('../../contexts/AlertsContext', () => ({
  useAlerts: vi.fn(() => ({
    feed: [],
    clearFeed: vi.fn(),
    rules: [],
    addRule: mockAddRule,
    removeRule: vi.fn(),
  })),
}));

describe('AlertRuleForm', () => {
  beforeEach(() => {
    mockAddRule.mockClear();
  });

  it('shows source selector', () => {
    render(<AlertRuleForm />);
    const sourceSelect = screen.getByLabelText('Alert source');
    expect(sourceSelect).toBeInTheDocument();
    expect(screen.getByText('compressor_events')).toBeInTheDocument();
    expect(screen.getByText('alerts_log')).toBeInTheDocument();
  });

  it('renders field selector disabled initially', () => {
    render(<AlertRuleForm />);
    const fieldSelect = screen.getByLabelText('Alert field');
    expect(fieldSelect).toBeDisabled();
  });

  it('shows non-metadata fields after', () => {
    render(<AlertRuleForm />);
    fireEvent.change(screen.getByLabelText('Alert source'), {
      target: { value: 'compressor_events' },
    });

    const fieldSelect = screen.getByLabelText('Alert field');
    expect(fieldSelect).not.toBeDisabled();
    
    expect(screen.queryByText('DEVICE_ID')).not.toBeInTheDocument();
    expect(screen.queryByText('GATEWAY_ID')).not.toBeInTheDocument();
    
    expect(screen.getByText('STATUS_ID_TEMP_CABINET')).toBeInTheDocument();
    expect(screen.getByText('STATUS_ID_COMP_SPEED')).toBeInTheDocument();
  });

  it('shows operator dropdown', () => {
    render(<AlertRuleForm />);
    const operatorSelect = screen.getByLabelText('Comparison operator');
    expect(operatorSelect).toBeInTheDocument();
    
    expect(operatorSelect).toHaveValue('>');
  });

  it('renders a threshold input', () => {
    render(<AlertRuleForm />);
    expect(screen.getByLabelText('Threshold value')).toBeInTheDocument();
  });

  it('disables submit button when required fields are not filled', () => {
    render(<AlertRuleForm />);
    const submitBtn = screen.getByRole('button', { name: /rule/i });
    expect(submitBtn).toBeDisabled();
  });

  it('enables submit button when all fields are filled', () => {
    render(<AlertRuleForm />);

    fireEvent.change(screen.getByLabelText('Alert source'), {
      target: { value: 'compressor_events' },
    });
    
    fireEvent.change(screen.getByLabelText('Alert field'), {
      target: { value: 'STATUS_ID_TEMP_CABINET' },
    });
    
    fireEvent.change(screen.getByLabelText('Threshold value'), {
      target: { value: '42' },
    });

    const submitBtn = screen.getByRole('button', { name: /rule/i });
    expect(submitBtn).not.toBeDisabled();
  });

  it('calls addRule with correct', async () => {
    render(<AlertRuleForm />);

    fireEvent.change(screen.getByLabelText('Alert source'), {
      target: { value: 'compressor_events' },
    });
    fireEvent.change(screen.getByLabelText('Alert field'), {
      target: { value: 'STATUS_ID_TEMP_CABINET' },
    });
    fireEvent.change(screen.getByLabelText('Comparison operator'), {
      target: { value: '>=' },
    });
    fireEvent.change(screen.getByLabelText('Threshold value'), {
      target: { value: '50' },
    });

    fireEvent.click(screen.getByRole('button', { name: /rule/i }));

    await waitFor(() => {
      expect(mockAddRule).toHaveBeenCalledWith(
        'compressor_events',
        'STATUS_ID_TEMP_CABINET',
        '>=',
        '50',
      );
    });
  });

  it('resets field when source', () => {
    render(<AlertRuleForm />);

    fireEvent.change(screen.getByLabelText('Alert source'), {
      target: { value: 'compressor_events' },
    });
    fireEvent.change(screen.getByLabelText('Alert field'), {
      target: { value: 'STATUS_ID_TEMP_CABINET' },
    });

    fireEvent.change(screen.getByLabelText('Alert source'), {
      target: { value: 'alerts_log' },
    });
    const fieldSelect = screen.getByLabelText('Alert field');
    expect(fieldSelect).toHaveValue('');
  });
});
