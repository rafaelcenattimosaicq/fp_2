
import { render, screen } from '@testing-library/react';
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

import { QueryResultsList } from './QueryResultsList';
import type { Query } from '../../types';

const mockQueries: Query[] = [
  {
    id: 'q-001',
    coordinatorQueryId: null,
    request: { source: 'TEST', fields: ['A'], filters: [], aggregations: [], groupBy: [], window: null, devices: [] },
    status: 'completed',
    results: [{ A: 42 }],
    error: null,
    createdAt: Date.now(),
  },
  {
    id: 'q-002',
    coordinatorQueryId: null,
    request: { source: 'TEST', fields: ['A'], filters: [], aggregations: [], groupBy: [], window: null, devices: [] },
    status: 'pending',
    results: [],
    error: null,
    createdAt: Date.now() - 1000,
  },
];

describe('QueryResultsList', () => {
  
  it('shows empty message', () => {
    render(<QueryResultsList queries={[]} onRemove={vi.fn()} />);
    expect(screen.getByText(/submit a query/i)).toBeInTheDocument();
  });

  it('renders a card for each query', () => {
    render(<QueryResultsList queries={mockQueries} onRemove={vi.fn()} />);
    expect(screen.getByText('q-001')).toBeInTheDocument();
    expect(screen.getByText('q-002')).toBeInTheDocument();
  });
});
