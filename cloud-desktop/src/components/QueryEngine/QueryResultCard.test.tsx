import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;

vi.mock('echarts/core', () => {
  const mockChart = {
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
    showLoading: vi.fn(),
    hideLoading: vi.fn(),
  };
  return {
    init: vi.fn(() => mockChart),
    getInstanceByDom: vi.fn(() => mockChart),
    use: vi.fn(),
  };
});

import { QueryResultCard } from './QueryResultCard';
import type { Query } from '../../types';

function makeQuery(overrides: Partial<Query> = {}): Query {
  return {
    id: 'q-abc12345-default',
    coordinatorQueryId: null,
    request: {
      source: 'TEST',
      fields: ['A', 'B'],
      filters: [],
      aggregations: [],
      groupBy: [],
      window: null,
      devices: ['DEV-001'],
    },
    status: 'completed',
    results: [{ A: 10, B: 20 }, { A: 30, B: 40 }],
    error: null,
    createdAt: Date.now(),
    ...overrides,
  };
}

const completedQuery: Query = makeQuery({
  id: 'q-abc',
  status: 'completed',
  results: [{ A: 10, B: 20 }, { A: 30, B: 40 }],
});

const pendingQuery: Query = makeQuery({
  id: 'q-xyz',
  status: 'pending',
  results: [],
});

const failedQuery: Query = makeQuery({
  id: 'q-err',
  status: 'failed',
  results: [],
  error: 'Connection timeout',
});

const runningQuery: Query = makeQuery({
  id: 'q-run',
  status: 'running',
  results: [],
});

const runningQueryWithResults: Query = makeQuery({
  id: 'q-run-data',
  status: 'running',
  results: [
    { timestamp: 1000, TEMP: 25.5, SPEED: 1200 },
    { timestamp: 2000, TEMP: 26.0, SPEED: 1250 },
  ],
  request: {
    source: 'TEST',
    fields: ['TEMP', 'SPEED'],
    filters: [],
    aggregations: [],
    groupBy: [],
    window: null,
    devices: ['DEV-001'],
  },
});

const stoppedQuery: Query = makeQuery({
  id: 'q-stop',
  status: 'stopped',
  results: [{ X: 100, Y: 200 }],
  request: {
    source: 'TEST',
    fields: ['X', 'Y'],
    filters: [],
    aggregations: [],
    groupBy: [],
    window: null,
    devices: ['DEV-001'],
  },
});

const stoppedQueryEmpty: Query = makeQuery({
  id: 'q-stop-empty',
  status: 'stopped',
  results: [],
});

describe('QueryResultCard', () => {
  it('shows query ID', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.getByText(/q-abc/i)).toBeInTheDocument();
    expect(screen.getByText(/completed/i)).toBeInTheDocument();
  });

  it('shows result table when expanded', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('expands on click', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    fireEvent.click(screen.getByText(/q-abc/i));
    expect(screen.getByText('A')).toBeInTheDocument();
  });

  it('shows pending status for pending queries', () => {
    render(<QueryResultCard query={pendingQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/pending/i)).toBeInTheDocument();
    expect(screen.getByText(/waiting/i)).toBeInTheDocument();
  });

  it('shows error message for failed queries', () => {
    render(<QueryResultCard query={failedQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/failed/i)).toBeInTheDocument();
    expect(screen.getByText(/connection timeout/i)).toBeInTheDocument();
  });

  it('shows RUNNING badge for running queries', () => {
    render(<QueryResultCard query={runningQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.getByText(/running/i)).toBeInTheDocument();
  });

  it('shows waiting message for running queries with no results', () => {
    render(<QueryResultCard query={runningQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/waiting for first data point/i)).toBeInTheDocument();
  });

  it('shows STOPPED badge for stopped queries', () => {
    render(<QueryResultCard query={stoppedQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.getByText(/stopped/i)).toBeInTheDocument();
  });

  it('shows stopped message for stopped queries with no results', () => {
    render(<QueryResultCard query={stoppedQueryEmpty} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/query was stopped/i)).toBeInTheDocument();
  });

  it('shows FAILED badge with error message when expanded', () => {
    const queryWithError: Query = makeQuery({
      id: 'q-fail-detail',
      status: 'failed',
      results: [],
      error: 'NES coordinator unreachable: ECONNREFUSED',
    });
    render(<QueryResultCard query={queryWithError} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/failed/i)).toBeInTheDocument();
    expect(screen.getByText(/NES coordinator unreachable: ECONNREFUSED/i)).toBeInTheDocument();
  });

  it('shows Unknown error', () => {
    const queryNoError: Query = makeQuery({
      id: 'q-fail-null',
      status: 'failed',
      results: [],
      error: null,
    });
    render(<QueryResultCard query={queryNoError} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText(/unknown error/i)).toBeInTheDocument();
  });

  it('renders chart when query has numeric results', () => {
    const chartQuery: Query = makeQuery({
      id: 'q-chart',
      status: 'completed',
      results: [
        { timestamp: 1000, TEMP: 25.5, SPEED: 1200 },
        { timestamp: 2000, TEMP: 26.0, SPEED: 1250 },
        { timestamp: 3000, TEMP: 26.5, SPEED: 1300 },
      ],
      request: {
        source: 'TEST',
        fields: ['TEMP', 'SPEED'],
        filters: [],
        aggregations: [],
        groupBy: [],
        window: null,
        devices: ['DEV-001'],
      },
    });

    render(<QueryResultCard query={chartQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByTestId('echart-container')).toBeInTheDocument();
  });

  it('does not render chart when results have only metadata fields', () => {
    const metaOnlyQuery: Query = makeQuery({
      id: 'q-meta',
      status: 'completed',
      results: [
        { DEVICE_ID: 'dev-1', GATEWAY_ID: 'gw-1', timestamp: 1000 },
      ],
      request: {
        source: 'TEST',
        fields: [],
        filters: [],
        aggregations: [],
        groupBy: [],
        window: null,
        devices: ['DEV-001'],
      },
    });

    render(<QueryResultCard query={metaOnlyQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.queryByTestId('echart-container')).not.toBeInTheDocument();
  });

  it('renders table with result rows and correct column headers', () => {
    const tableQuery: Query = makeQuery({
      id: 'q-table',
      status: 'completed',
      results: [
        { DEVICE_ID: 'dev-1', TEMP: 25.5, SPEED: 1200 },
        { DEVICE_ID: 'dev-2', TEMP: 30.0, SPEED: 900 },
      ],
      request: {
        source: 'TEST',
        fields: ['TEMP', 'SPEED'],
        filters: [],
        aggregations: [],
        groupBy: [],
        window: null,
        devices: ['DEV-001'],
      },
    });

    render(<QueryResultCard query={tableQuery} defaultExpanded={true} onRemove={vi.fn()} />);

    expect(screen.getByText('DEVICE_ID')).toBeInTheDocument();
    expect(screen.getByText('TEMP')).toBeInTheDocument();
    expect(screen.getByText('SPEED')).toBeInTheDocument();

    expect(screen.getByText('dev-1')).toBeInTheDocument();
    expect(screen.getByText('25.5')).toBeInTheDocument();
    expect(screen.getByText('1200')).toBeInTheDocument();
    expect(screen.getByText('dev-2')).toBeInTheDocument();
    expect(screen.getByText('30')).toBeInTheDocument();
    expect(screen.getByText('900')).toBeInTheDocument();
  });

  it('delete button calls the remove handler with the correct query ID', () => {
    const onRemove = vi.fn();
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={onRemove} />);

    const deleteBtn = screen.getByLabelText('Delete query');
    fireEvent.click(deleteBtn);

    expect(onRemove).toHaveBeenCalledTimes(1);
    expect(onRemove).toHaveBeenCalledWith('q-abc');
  });

  it('delete button click does not toggle expand/collapse', () => {
    const onRemove = vi.fn();
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={onRemove} />);

    const deleteBtn = screen.getByLabelText('Delete query');
    fireEvent.click(deleteBtn);

    expect(screen.queryByText('10')).not.toBeInTheDocument();
  });

  it('collapsing the card body hides the content', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);

    fireEvent.click(screen.getByText(/q-abc/i));
    expect(screen.getByText('A')).toBeInTheDocument();

    fireEvent.click(screen.getByText(/q-abc/i));
    expect(screen.queryByText('10')).not.toBeInTheDocument();
  });

  it('shows body immediately when defaultExpanded is true', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('toggles expansion on Enter key press', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);

    const header = screen.getByText(/q-abc/i).closest('[role="button"]') as HTMLElement;
    fireEvent.keyDown(header, { key: 'Enter' });
    expect(screen.getByText('A')).toBeInTheDocument();

    fireEvent.keyDown(header, { key: 'Enter' });
    expect(screen.queryByText('10')).not.toBeInTheDocument();
  });

  it('toggles expansion on Space key press', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);

    const header = screen.getByText(/q-abc/i).closest('[role="button"]') as HTMLElement;
    fireEvent.keyDown(header, { key: ' ' });
    expect(screen.getByText('A')).toBeInTheDocument();
  });

  it('shows row count in header', () => {
    render(<QueryResultCard query={completedQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.getByText('2 rows')).toBeInTheDocument();
  });

  it('shows singular "row" label', () => {
    const singleRowQuery: Query = makeQuery({
      id: 'q-single',
      status: 'completed',
      results: [{ A: 10, B: 20 }],
    });
    render(<QueryResultCard query={singleRowQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.getByText('1 row')).toBeInTheDocument();
  });

  it('does not show row count when results are empty', () => {
    render(<QueryResultCard query={pendingQuery} defaultExpanded={false} onRemove={vi.fn()} />);
    expect(screen.queryByText(/row/i)).not.toBeInTheDocument();
  });

  it('renders chart for running queries with numeric results', () => {
    render(<QueryResultCard query={runningQueryWithResults} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByTestId('echart-container')).toBeInTheDocument();
  });

  it('renders table for stopped queries with results', () => {
    render(<QueryResultCard query={stoppedQuery} defaultExpanded={true} onRemove={vi.fn()} />);
    expect(screen.getByText('X')).toBeInTheDocument();
    expect(screen.getByText('100')).toBeInTheDocument();
    expect(screen.getByText('Y')).toBeInTheDocument();
    expect(screen.getByText('200')).toBeInTheDocument();
  });
});
