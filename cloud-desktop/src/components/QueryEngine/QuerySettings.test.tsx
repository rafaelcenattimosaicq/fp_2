import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QuerySettings, DEFAULT_PREFERENCES } from './QuerySettings';
import type { QueryPreferences } from './QuerySettings';

const SAMPLE_SOURCES = ['telemetry_0x0007', 'telemetry_0x0008'];

function renderSettings(overrides: Partial<{
  sourceNames: string[];
  preferences: QueryPreferences;
  onChange: (updated: QueryPreferences) => void;
  onClose: () => void;
}> = {}) {
  const props = {
    sourceNames: SAMPLE_SOURCES,
    preferences: { ...DEFAULT_PREFERENCES },
    onChange: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  const result = render(<QuerySettings {...props} />);
  return { ...result, props };
}

describe('QuerySettings', () => {
  it('renders both layout options', () => {
    renderSettings();

    expect(screen.getByText(/Side by Side/)).toBeInTheDocument();
    expect(screen.getByText(/Stacked/)).toBeInTheDocument();
  });

  it('renders the Layout section title', () => {
    renderSettings();

    expect(screen.getByText('Layout')).toBeInTheDocument();
  });

  it('calls onChange with vertical layout when Stacked is clicked', () => {
    const onChange = vi.fn();
    renderSettings({ onChange, preferences: { ...DEFAULT_PREFERENCES, layout: 'horizontal' } });

    fireEvent.click(screen.getByText(/Stacked/));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ layout: 'vertical' }),
    );
  });

  it('calls onChange with horizontal layout when Side by Side is clicked', () => {
    const onChange = vi.fn();
    renderSettings({ onChange, preferences: { ...DEFAULT_PREFERENCES, layout: 'vertical' } });

    fireEvent.click(screen.getByText(/Side by Side/));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ layout: 'horizontal' }),
    );
  });

  it('highlights the active', () => {
    renderSettings({ preferences: { ...DEFAULT_PREFERENCES, layout: 'horizontal' } });

    const sideBtn = screen.getByText(/Side by Side/).closest('button') as HTMLElement;
    const stackedBtn = screen.getByText(/Stacked/).closest('button') as HTMLElement;

    expect(sideBtn.className).not.toBe(stackedBtn.className);
  });

  it('applies active class to vertical button when layout is vertical', () => {
    renderSettings({ preferences: { ...DEFAULT_PREFERENCES, layout: 'vertical' } });

    const sideBtn = screen.getByText(/Side by Side/).closest('button') as HTMLElement;
    const stackedBtn = screen.getByText(/Stacked/).closest('button') as HTMLElement;

    expect(stackedBtn.className).not.toBe(sideBtn.className);
  });

  it('shows all max results', () => {
    renderSettings();

    expect(screen.getByText('10')).toBeInTheDocument();
    expect(screen.getByText('20')).toBeInTheDocument();
    expect(screen.getByText('50')).toBeInTheDocument();
    expect(screen.getByText('100')).toBeInTheDocument();
  });

  it('renders the Max Results section title', () => {
    renderSettings();

    expect(screen.getByText('Max Results')).toBeInTheDocument();
  });

  it('calls onChange', () => {
    const onChange = vi.fn();
    renderSettings({ onChange });

    fireEvent.click(screen.getByText('50'));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ maxResults: 50 }),
    );
  });

  it('highlights the active max results option', () => {
    renderSettings({ preferences: { ...DEFAULT_PREFERENCES, maxResults: 20 } });

    const btn20 = screen.getByText('20').closest('button') as HTMLElement;
    const btn10 = screen.getByText('10').closest('button') as HTMLElement;

    expect(btn20.className).not.toBe(btn10.className);
  });

  it('shows default source dropdown', () => {
    renderSettings();

    expect(screen.getByText('Default Source')).toBeInTheDocument();

    const noneOption = screen.getByText('None');
    const selectEl = noneOption.closest('select') as HTMLSelectElement;
    const options = selectEl.querySelectorAll('option');

    expect(options).toHaveLength(SAMPLE_SOURCES.length + 1);
    expect(options[0]).toHaveTextContent('None');
    expect(options[1]).toHaveTextContent('telemetry_0x0007');
    expect(options[2]).toHaveTextContent('telemetry_0x0008');
  });

  it('calls onChange with updated defaultSource when source changes', () => {
    const onChange = vi.fn();
    renderSettings({ onChange });

    const noneOption = screen.getByText('None');
    const selectEl = noneOption.closest('select') as HTMLSelectElement;

    fireEvent.change(selectEl, { target: { value: 'telemetry_0x0007' } });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ defaultSource: 'telemetry_0x0007' }),
    );
  });

  it('renders default aggregation dropdown with all functions', () => {
    renderSettings();

    expect(screen.getByText('Default Aggregation')).toBeInTheDocument();
    expect(screen.getByText('AVG')).toBeInTheDocument();
    expect(screen.getByText('MIN')).toBeInTheDocument();
    expect(screen.getByText('MAX')).toBeInTheDocument();
    expect(screen.getByText('COUNT')).toBeInTheDocument();
    expect(screen.getByText('SUM')).toBeInTheDocument();
  });

  it('calls onChange with updated defaultAggFunction when aggregation changes', () => {
    const onChange = vi.fn();
    renderSettings({ onChange });

    const avgOption = screen.getByText('AVG');
    const selectEl = avgOption.closest('select') as HTMLSelectElement;

    fireEvent.change(selectEl, { target: { value: 'SUM' } });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ defaultAggFunction: 'SUM' }),
    );
  });

  it('renders default window type dropdown', () => {
    renderSettings();

    expect(screen.getByText('Default Window')).toBeInTheDocument();
    expect(screen.getByText('tumbling')).toBeInTheDocument();
    expect(screen.getByText('sliding')).toBeInTheDocument();
  });

  it('shows window size input', () => {
    renderSettings();

    const sizeInput = screen.getByLabelText('Default window size') as HTMLInputElement;
    expect(sizeInput).toBeInTheDocument();
    expect(sizeInput.value).toBe('10');
  });

  it('renders seconds suffix next to window size input', () => {
    renderSettings();

    expect(screen.getByText('s')).toBeInTheDocument();
  });

  it('calls onChange with updated defaultWindowType', () => {
    const onChange = vi.fn();
    renderSettings({ onChange });

    const tumblingOption = screen.getByText('tumbling');
    const selectEl = tumblingOption.closest('select') as HTMLSelectElement;

    fireEvent.change(selectEl, { target: { value: 'sliding' } });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ defaultWindowType: 'sliding' }),
    );
  });

  it('calls onChange with updated defaultWindowSize', () => {
    const onChange = vi.fn();
    renderSettings({ onChange });

    const sizeInput = screen.getByLabelText('Default window size');
    fireEvent.change(sizeInput, { target: { value: '30' } });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ defaultWindowSize: 30 }),
    );
  });

  it('calls onClose when clicking outside the panel', () => {
    const onClose = vi.fn();
    renderSettings({ onClose });

    fireEvent.mouseDown(document.body);

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not call onClose', () => {
    const onClose = vi.fn();
    renderSettings({ onClose });

    fireEvent.mouseDown(screen.getByText('Layout'));

    expect(onClose).not.toHaveBeenCalled();
  });

  it('preserves other preference values when changing one setting', () => {
    const onChange = vi.fn();
    const prefs: QueryPreferences = {
      ...DEFAULT_PREFERENCES,
      maxResults: 50,
      layout: 'vertical',
      defaultSource: 'telemetry_0x0007',
    };
    renderSettings({ onChange, preferences: prefs });

    fireEvent.click(screen.getByText(/Side by Side/));

    expect(onChange).toHaveBeenCalledWith({
      ...prefs,
      layout: 'horizontal',
    });
  });
});
