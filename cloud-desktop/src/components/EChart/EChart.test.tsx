import { render, screen, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, afterEach } from 'vitest';
import { EChart } from './EChart';
import type { ECOption } from './echarts-setup';

let resizeCallback: (() => void) | null = null;
class ResizeObserverStub {
  constructor(cb: ResizeObserverCallback) {
    
    resizeCallback = () => cb([], this as unknown as ResizeObserver);
  }
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}
globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;

const mockChart = {
  setOption: vi.fn(),
  resize: vi.fn(),
  dispose: vi.fn(),
  showLoading: vi.fn(),
  hideLoading: vi.fn(),
};
vi.mock('echarts/core', () => ({
  init: vi.fn(() => mockChart),
  getInstanceByDom: vi.fn(() => mockChart),
  use: vi.fn(),
}));

describe('EChart', () => {
  
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    resizeCallback = null;
  });

  const sampleOption: ECOption = {
    xAxis: { type: 'category', data: ['Mon', 'Tue'] },
    yAxis: { type: 'value' },
    series: [{ type: 'line', data: [100, 200] }],
  };

  it('renders a chart container div', () => {
    render(<EChart option={sampleOption} />);
    const container = screen.getByTestId('echart-container');
    expect(container).toBeInTheDocument();
  });

  it('applies custom height via style prop', () => {
    render(<EChart option={sampleOption} style={{ height: '400px' }} />);
    const container = screen.getByTestId('echart-container');
    expect(container.style.height).toBe('400px');
  });

  it('calls onChartReady with the chart instance after init', () => {
    
    const origClientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
    const origClientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
    Object.defineProperty(HTMLElement.prototype, 'clientWidth', { value: 800, configurable: true });
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', { value: 300, configurable: true });

    const onReady = vi.fn();
    render(<EChart option={sampleOption} onChartReady={onReady} />);

    expect(onReady).toHaveBeenCalledTimes(1);
    expect(onReady).toHaveBeenCalledWith(expect.objectContaining({ setOption: expect.any(Function) }));

    if (origClientWidth) {
      Object.defineProperty(HTMLElement.prototype, 'clientWidth', origClientWidth);
    } else {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientWidth;
    }
    if (origClientHeight) {
      Object.defineProperty(HTMLElement.prototype, 'clientHeight', origClientHeight);
    } else {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
    }
  });

  it('calls onChartReady via ResizeObserver when init is deferred', () => {
    
    const onReady = vi.fn();
    render(<EChart option={sampleOption} onChartReady={onReady} />);
    expect(onReady).not.toHaveBeenCalled();

    const el = screen.getByTestId('echart-container');
    Object.defineProperty(el, 'clientWidth', { value: 800, configurable: true });
    Object.defineProperty(el, 'clientHeight', { value: 300, configurable: true });
    resizeCallback?.();

    expect(onReady).toHaveBeenCalledTimes(1);
    expect(onReady).toHaveBeenCalledWith(expect.objectContaining({ resize: expect.any(Function) }));
  });
});
