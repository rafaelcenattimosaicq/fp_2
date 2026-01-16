import { useState, useCallback, useEffect, useRef } from 'react';
import { FilterRow } from './FilterRow';
import type {
  QueryFilter,
  QueryRequest,
  AggregationFunction,
  WindowType,
} from '../../types';
import type { QueryPreferences } from './QuerySettings';
import styles from './QueryForm.module.css';

const AGG_FNS: AggregationFunction[] = ['AVG', 'MIN', 'MAX', 'COUNT', 'SUM'];

// configurable slide amount.  NES also supports "threshold" windows but
const WIN_TYPES: WindowType[] = ['tumbling', 'sliding'];

interface SourceDescriptor {
  name: string;
  fields: string[];
}

interface QueryFormProps {
  sources: SourceDescriptor[];
  onSubmit: (request: QueryRequest) => void;
  submitting: boolean;
  defaults?: QueryPreferences;
}

// the actual C++ NES REST API call.
export function QueryForm({
  sources,
  onSubmit,
  submitting,
  defaults,
}: QueryFormProps): React.JSX.Element {
  const [src, setSrc] = useState(() => defaults?.defaultSource ?? '');
  const [selectedFlds, setSelectedFlds] = useState<string[]>([]);
  const [filters, setFilters] = useState<QueryFilter[]>([]);
  const [aggFn, setAggFn] = useState<AggregationFunction>(
    () => defaults?.defaultAggFunction ?? 'AVG',
  );
  const [aggField, setAggField] = useState('');
  const [groupBy] = useState<string[]>([]);
  const [winType, setWinType] = useState<WindowType>(
    () => defaults?.defaultWindowType ?? 'tumbling',
  );
  const [winSize, setWinSize] = useState(
    () => defaults?.defaultWindowSize ?? 10,
  );
  const [winSlide, setWinSlide] = useState(5);

  const [joinSrc, setJoinSrc] = useState('');
  const [joinKey, setJoinKey] = useState('timestamp');
  const [joinFlds, setJoinFlds] = useState<string[]>([]);

  // union, merge same-schema sources (e.g. same device type on different gateways)
  const [unionSrcs, setUnionSrcs] = useState<string[]>([]);

  const appliedRef = useRef(
    defaults
      ? `${defaults.defaultSource}|${defaults.defaultAggFunction}|${defaults.defaultWindowType}|${defaults.defaultWindowSize}`
      : '',
  );
  useEffect(() => {
    if (!defaults) return;
    const fp = `${defaults.defaultSource}|${defaults.defaultAggFunction}|${defaults.defaultWindowType}|${defaults.defaultWindowSize}`;
    if (fp === appliedRef.current) return;
    appliedRef.current = fp;

    const id = requestAnimationFrame(() => {
      if (defaults.defaultSource && !src) {
        setSrc(defaults.defaultSource);
      }
      setAggFn(defaults.defaultAggFunction);
      setWinType(defaults.defaultWindowType);
      setWinSize(defaults.defaultWindowSize);
    });

    return () => cancelAnimationFrame(id);
  }, [defaults, src]);

  const flds = sources.find((s) => s.name === src)?.fields ?? [];

  // are union-compatible, they have the same schema so NES can merge them
  const unionCompat = (() => {
    if (!src) return [];
    const m = src.match(/^(.+)_GW-/i);
    if (!m) return [];
    return sources.filter((s) => s.name !== src && s.name.startsWith(m[1] + '_GW-'));
  })();
  const showUnion = unionCompat.length > 0 && !joinSrc;

  const joinableSrcs = sources.filter((s) => s.name !== src);
  const showJoin = src !== '' && joinableSrcs.length > 0;

  const joinSrcFlds = joinSrc
    ? sources.find((s) => s.name === joinSrc)?.fields ?? []
    : [];

  const allFlds = joinSrc
    ? [...flds, ...joinSrcFlds.filter((f) => !flds.includes(f))]
    : flds;

  const toggleField = useCallback((field: string) => {
    setSelectedFlds((prev) =>
      prev.includes(field) ? prev.filter((f) => f !== field) : [...prev, field],
    );
  }, []);

  function toggleJoinFld(field: string): void {
    setJoinFlds((prev) =>
      prev.includes(field) ? prev.filter((f) => f !== field) : [...prev, field],
    );
  }

  const addFilter = useCallback(() => {
    setFilters((prev) => [...prev, { field: '', operator: '=', value: '' }]);
  }, []);

  const updateFilter = useCallback((idx: number, updated: QueryFilter) => {
    setFilters((prev) => prev.map((f, i) => (i === idx ? updated : f)));
  }, []);

  function removeFilter(idx: number): void {
    setFilters((prev) => prev.filter((_, i) => i !== idx));
  }

  // translate this into the NES REST API format
  const handleSubmit = (): void => {
    if (!src) return;

    const req: QueryRequest = {
      source: src,
      fields: selectedFlds,
      filters: filters.filter((f) => f.field && f.value),
      aggregations: aggField ? [{ function: aggFn, field: aggField }] : [],
      groupBy,
      window:
        winSize > 0
          ? {
              type: winType,
              size: winSize,
              // slide only matters for sliding windows, tumbling windows
              ...(winType === 'sliding' ? { slide: winSlide } : {}),
            }
          : null,
      devices: [],
      ...(joinSrc
        ? {
            joinSource: joinSrc,
            joinKey: { left: joinKey, right: joinKey },
            joinFields: joinFlds,
          }
        : {}),
      ...(unionSrcs.length > 0 ? { unionSources: unionSrcs } : {}),
    };

    onSubmit(req);
  };

  return (
    <div className={styles.form}>
      {/* source dropdown - each source is a logical NES stream backed by
          an MQTT_SOURCE or KAFKA_SOURCE depending on gateway config */}
      <label className={styles.label}>
        Source
        <select
          className={styles.select}
          value={src}
          onChange={(e) => {
            setSrc(e.target.value);
            setSelectedFlds([]);
            setJoinSrc('');
            setJoinKey('timestamp');
            setJoinFlds([]);
            setUnionSrcs([]);
          }}
        >
          <option value="">Select source</option>
          {sources.map((s) => (
            <option key={s.name} value={s.name}>
              {s.name}
            </option>
          ))}
        </select>
      </label>

      {flds.length > 0 && (
        <fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Fields</legend>
          {/* these correspond to the .map() projection in NES DSL - unchecked
              fields are still present in the stream but excluded from the
              result schema the coordinator sends back */}
          <div className={styles.fields}>
            {flds.map((field) => (
              <label key={field} className={styles.checkbox}>
                <input
                  type="checkbox"
                  checked={selectedFlds.includes(field)}
                  onChange={() => toggleField(field)}
                />
                {field}
              </label>
            ))}
          </div>
        </fieldset>
      )}

      {showJoin && (
        <fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Join With</legend>
          <div className={styles.joinRow}>
            <label className={styles.label}>
              Join Source
              <select
                className={styles.select}
                value={joinSrc}
                onChange={(e) => {
                  setJoinSrc(e.target.value);
                  setJoinFlds([]);
                }}
                aria-label="Join source"
              >
                <option value="">No join</option>
                {joinableSrcs.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name}
                  </option>
                ))}
              </select>
            </label>
            {joinSrc && (
              <label className={styles.label}>
                Join Key
                <select
                  className={styles.select}
                  value={joinKey}
                  onChange={(e) => setJoinKey(e.target.value)}
                  aria-label="Join key"
                >
                  {flds
                    .filter((f) => {
                      const jf = sources.find((s) => s.name === joinSrc)?.fields ?? [];
                      return jf.includes(f);
                    })
                    .map((f) => (
                      <option key={f} value={f}>
                        {f}
                      </option>
                    ))}
                </select>
              </label>
            )}
          </div>

          {joinSrc && joinSrcFlds.length > 0 && (
            <div className={styles.joinFields}>
              <span className={styles.joinFieldsLabel}>Join Source Fields</span>
              <div className={styles.fields}>
                {joinSrcFlds.map((field) => (
                  <label key={field} className={styles.checkbox}>
                    <input
                      type="checkbox"
                      checked={joinFlds.includes(field)}
                      onChange={() => toggleJoinFld(field)}
                    />
                    {field}
                  </label>
                ))}
              </div>
            </div>
          )}
        </fieldset>
      )}

      {/* union merges rows from multiple gateways running the same device
          type - NES requires identical schemas on both sides */}
      {showUnion && (
        <fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Union With (same device type)</legend>
          <div className={styles.fields}>
            {unionCompat.map((s) => (
              <label key={s.name} className={styles.checkbox}>
                <input
                  type="checkbox"
                  checked={unionSrcs.includes(s.name)}
                  onChange={() =>
                    setUnionSrcs((prev) =>
                      prev.includes(s.name)
                        ? prev.filter((n) => n !== s.name)
                        : [...prev, s.name],
                    )
                  }
                />
                {s.name}
              </label>
            ))}
          </div>
        </fieldset>
      )}

      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>
          Filters
          <button type="button" className={styles.addBtn} onClick={addFilter}>
            + Add
          </button>
        </legend>
        {/* each filter becomes a .filter(Attribute("field") op value) in the
            NES DSL - note NES doesn't support LIKE or regex on TEXT fields */}
        <div className={styles.filterList}>
          {filters.map((filter, i) => (
            <FilterRow
              key={i}
              filter={filter}
              fields={allFlds}
              onChange={(updated) => updateFilter(i, updated)}
              onRemove={() => removeFilter(i)}
            />
          ))}
        </div>
      </fieldset>

      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>Aggregation</legend>
        <div className={styles.aggRow}>
          <select
            className={styles.select}
            value={aggFn}
            onChange={(e) => setAggFn(e.target.value as AggregationFunction)}
            aria-label="Aggregation function"
          >
            {AGG_FNS.map((fn) => (
              <option key={fn} value={fn}>
                {fn}
              </option>
            ))}
          </select>
          <select
            className={styles.select}
            value={aggField}
            onChange={(e) => setAggField(e.target.value)}
            aria-label="Aggregation field"
          >
            <option value="">No aggregation</option>
            {allFlds.map((f) => (
              <option key={f} value={f}>
                {f}
              </option>
            ))}
          </select>
        </div>
      </fieldset>

      {/* window config - tumbling windows are simpler (just a size), sliding
          windows need both size and slide.  The NES coordinator rejects
          slide > size so we probably should validate that here.
          TODO: add client-side validation for slide <= size */}
      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>Window</legend>
        <div className={styles.windowRow}>
          <label className={styles.label}>
            Type
            <select
              className={styles.select}
              value={winType}
              onChange={(e) => setWinType(e.target.value as WindowType)}
              aria-label="Window type"
            >
              {WIN_TYPES.map((wt) => (
                <option key={wt} value={wt}>
                  {wt}
                </option>
              ))}
            </select>
          </label>
          <label className={styles.label}>
            Size (s)
            <input
              type="number"
              className={styles.input}
              value={winSize}
              min={0}
              onChange={(e) => setWinSize(Number(e.target.value))}
              aria-label="Window size"
            />
          </label>
          {winType === 'sliding' && (
            <label className={styles.label}>
              Slide (s)
              <input
                type="number"
                className={styles.input}
                value={winSlide}
                min={1}
                onChange={(e) => setWinSlide(Number(e.target.value))}
                aria-label="Window slide"
              />
            </label>
          )}
        </div>
      </fieldset>

      <button
        type="button"
        className={styles.submit}
        disabled={submitting || !src}
        onClick={handleSubmit}
      >
        {submitting ? 'Running...' : 'Run Query'}
      </button>
    </div>
  );
}
