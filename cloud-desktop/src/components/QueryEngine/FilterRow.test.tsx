
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { FilterRow } from './FilterRow';
import type { QueryFilter } from '../../types';

const SAMPLE_FIELDS = ['STATUS_ID_TEMP_CABINET', 'STATUS_ID_COMP_SPEED', 'timestamp'];

function makeFilter(overrides: Partial<QueryFilter> = {}): QueryFilter {
  return {
    field: '',
    operator: '=',
    value: '',
    ...overrides,
  };
}

describe('FilterRow', () => {
  
  it('renders field selector dropdown with all field options', () => {
    render(
      <FilterRow
        filter={makeFilter()}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const fieldSelect = screen.getByLabelText('Filter field');
    expect(fieldSelect).toBeInTheDocument();

    const options = fieldSelect.querySelectorAll('option');
    expect(options).toHaveLength(SAMPLE_FIELDS.length + 1); 
    expect(options[0]).toHaveTextContent('Field');
    expect(options[1]).toHaveTextContent('STATUS_ID_TEMP_CABINET');
    expect(options[2]).toHaveTextContent('STATUS_ID_COMP_SPEED');
    expect(options[3]).toHaveTextContent('timestamp');
  });

  it('renders operator selector dropdown with all operators', () => {
    render(
      <FilterRow
        filter={makeFilter()}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const operatorSelect = screen.getByLabelText('Filter operator');
    expect(operatorSelect).toBeInTheDocument();

    const options = operatorSelect.querySelectorAll('option');
    const operators = Array.from(options).map((o) => o.textContent);
    expect(operators).toEqual(['=', '!=', '>', '<', '>=', '<=']);
  });

  it('renders value input with placeholder', () => {
    render(
      <FilterRow
        filter={makeFilter()}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const valueInput = screen.getByLabelText('Filter value');
    expect(valueInput).toBeInTheDocument();
    expect(valueInput).toHaveAttribute('placeholder', 'Value');
  });

  it('renders remove button', () => {
    render(
      <FilterRow
        filter={makeFilter()}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const removeBtn = screen.getByLabelText('Remove filter');
    expect(removeBtn).toBeInTheDocument();
  });

  it('shows current filter field value', () => {
    render(
      <FilterRow
        filter={makeFilter({ field: 'timestamp' })}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const fieldSelect = screen.getByLabelText('Filter field') as HTMLSelectElement;
    expect(fieldSelect.value).toBe('timestamp');
  });

  it('shows current operator value', () => {
    render(
      <FilterRow
        filter={makeFilter({ operator: '>=' })}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const operatorSelect = screen.getByLabelText('Filter operator') as HTMLSelectElement;
    expect(operatorSelect.value).toBe('>=');
  });

  it('displays the current filter value in the input', () => {
    render(
      <FilterRow
        filter={makeFilter({ value: '42' })}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const valueInput = screen.getByLabelText('Filter value') as HTMLInputElement;
    expect(valueInput.value).toBe('42');
  });

  it('calls onChange with updated field', () => {
    const onChange = vi.fn();
    const filter = makeFilter({ field: '', operator: '=', value: '10' });

    render(
      <FilterRow
        filter={filter}
        fields={SAMPLE_FIELDS}
        onChange={onChange}
        onRemove={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText('Filter field'), {
      target: { value: 'STATUS_ID_COMP_SPEED' },
    });

    expect(onChange).toHaveBeenCalledWith({
      ...filter,
      field: 'STATUS_ID_COMP_SPEED',
    });
  });

  it('calls onChange with updated operator when operator selector changes', () => {
    const onChange = vi.fn();
    const filter = makeFilter({ field: 'timestamp', operator: '=', value: '100' });

    render(
      <FilterRow
        filter={filter}
        fields={SAMPLE_FIELDS}
        onChange={onChange}
        onRemove={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText('Filter operator'), {
      target: { value: '>' },
    });

    expect(onChange).toHaveBeenCalledWith({
      ...filter,
      operator: '>',
    });
  });

  it('calls onChange with updated value when value input changes', () => {
    const onChange = vi.fn();
    const filter = makeFilter({ field: 'timestamp', operator: '>', value: '' });

    render(
      <FilterRow
        filter={filter}
        fields={SAMPLE_FIELDS}
        onChange={onChange}
        onRemove={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText('Filter value'), {
      target: { value: '999' },
    });

    expect(onChange).toHaveBeenCalledWith({
      ...filter,
      value: '999',
    });
  });

  it('calls onRemove when remove button is clicked', () => {
    const onRemove = vi.fn();

    render(
      <FilterRow
        filter={makeFilter()}
        fields={SAMPLE_FIELDS}
        onChange={vi.fn()}
        onRemove={onRemove}
      />,
    );

    fireEvent.click(screen.getByLabelText('Remove filter'));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('renders with empty fields array (only placeholder option)', () => {
    render(
      <FilterRow
        filter={makeFilter()}
        fields={[]}
        onChange={vi.fn()}
        onRemove={vi.fn()}
      />,
    );

    const fieldSelect = screen.getByLabelText('Filter field');
    const options = fieldSelect.querySelectorAll('option');
    expect(options).toHaveLength(1); 
    expect(options[0]).toHaveTextContent('Field');
  });
});
