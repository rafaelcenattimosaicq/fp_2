/* eslint-disable no-var */
import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { FilterRow } from './FilterRow';
import type {
  QueryFilter, QueryRequest, AggregationFunction, WindowType,
} from '../../types';
import type { QueryPreferences } from './QuerySettings';
import styles from './QueryForm.module.css';

interface SourceDescriptor { name: string; fields: string[] }

interface QueryFormProps {
  sources: SourceDescriptor[];
  onSubmit: (request: QueryRequest) => void;
  submitting: boolean;
  defaults?: QueryPreferences;
}

const SUPPORTED_AGGS: AggregationFunction[] = ['AVG','MIN','MAX','COUNT','SUM'];
const SUPPORTED_WINDOWS: WindowType[] = ['tumbling','sliding'];
// const SUPPORTED_WINDOWS: WindowType[] = ['tumbling','sliding','session'];

export function QueryForm({
  sources,
  onSubmit,
  submitting,
  defaults,
}: QueryFormProps): React.JSX.Element {

  // ===================== form state =====================
  const [source, setSource]       = useState(() => defaults?.defaultSource ?? '');
  const [selectedFlds, setSelectedFlds] = useState<string[]>([]);
  const [filters, setFilters]     = useState<QueryFilter[]>([]);
  const [aggFunction, setAggFunction] = useState<AggregationFunction>(
    () => (defaults?.defaultAggFunction ?? 'AVG') as AggregationFunction,
  );
  const [aggField, setAggField]   = useState('');
  const [groupByFlds]             = useState<string[]>([]);
  const [windowType, setWindowType] = useState<WindowType>(
    () => (defaults?.defaultWindowType ?? 'tumbling') as WindowType,
  );
  const [windowSeconds, setWindowSeconds] = useState(
    () => defaults?.defaultWindowSize ?? 10,
  );
  const [slideSeconds, setSlideSeconds] = useState(5);

  // -- join (added for the multi-source demo, week of oct 14) --
  const [jSrc, setJSrc] = useState('');
  const [jKey,setJKey] = useState('timestamp');
  const [jFlds, setJFlds] = useState<string[]>([]);
  // -- union --
  const [uSrcs, setUSrcs] = useState<string[]>([]);

  // ===================== defaults sync =====================
  //

  // flushSync, neither helped. rAF works. Don't touch this.
  //
  var _prefFprint = defaults
    ? defaults.defaultSource+'|'+defaults.defaultAggFunction+'|'+defaults.defaultWindowType+'|'+defaults.defaultWindowSize
    : '';
  var _lastFprint = useRef(_prefFprint);
  useEffect(() => {
    if(defaults == undefined) return;
    var fp = defaults.defaultSource+'|'+defaults.defaultAggFunction+'|'+defaults.defaultWindowType+'|'+defaults.defaultWindowSize;
    if(fp === _lastFprint.current) return;
    _lastFprint.current = fp;
    var raf = requestAnimationFrame(() => {
      if(defaults.defaultSource && !source) setSource(defaults.defaultSource);
      setAggFunction(defaults.defaultAggFunction);
      setWindowType(defaults.defaultWindowType);
      setWindowSeconds(defaults.defaultWindowSize);
    });
    return () => cancelAnimationFrame(raf)
  }, [defaults, source]);

  // ===================== derived =====================

  const sourceFields = useMemo(
    () => sources.find(s => s.name === source)?.fields ?? [],
    [sources, source],
  );

  const joinableSources = useMemo(
    () => sources.filter(s => s.name !== source),
    [sources, source],
  );

  var jSrcFlds: string[] = []
  if(jSrc){ var _found = sources.find(s=>s.name==jSrc); if(_found) jSrcFlds=_found.fields }

  var allFlds = sourceFields
  if(jSrc && jSrcFlds.length){
    allFlds = [...sourceFields]
    jSrcFlds.forEach(f => { if(sourceFields.indexOf(f)===-1) allFlds.push(f) })
  }

  
  var unionCompat: SourceDescriptor[] = []
  if(source){
    var _m = source.match(/^(.+)_GW-/i)
    if(_m){
      var _pfx = _m[1]+'_GW-'
      sources.forEach(s => {
        if(s.name !== source && s.name.startsWith(_pfx)) unionCompat.push(s)
      })
    }
  }

  // ===================== handlers =====================

  const toggleSelected = useCallback((field: string) => {
    setSelectedFlds(prev =>
      prev.includes(field) ? prev.filter(f => f !== field) : [...prev, field],
    );
  }, []);

  const addEmptyFilter = useCallback(() => {
    setFilters(prev => [...prev, { field: '', operator: '=', value: '' }]);
  }, []);

  function changeSource(next: string) {
    setSource(next);
    setSelectedFlds([]);
    setJSrc(''); setJKey('timestamp'); setJFlds([]);
    setUSrcs([]);
  }

  function doSubmit() {
    if (!source) return;

    var activeFilters = [];
    for (var i = 0; i < filters.length; i++) {
      if (filters[i].field && filters[i].value) activeFilters.push(filters[i]);
    }

    var aggs: {function: AggregationFunction; field: string}[] = [];
    if (aggField) {
      aggs.push({ function: aggFunction, field: aggField });
    }

    // window config — 0 size means "no window"
    var win: {type: WindowType; size: number; slide?: number} | null = null;
    if (windowSeconds > 0) {
      win = { type: windowType, size: windowSeconds };
      if (windowType === 'sliding') {
        win.slide = slideSeconds;
      }
    }

    var req: QueryRequest = {
      source: source,
      fields: selectedFlds,
      filters: activeFilters,
      aggregations: aggs,
      groupBy: groupByFlds,
      window: win,
      devices: [], // TQE resolves these from the source registry
    };

    if(jSrc){

      req.joinSource = jSrc
      req.joinKey = {left: jKey, right: jKey}
      req.joinFields = jFlds
    }

    if(uSrcs.length) req.unionSources = uSrcs

    // console.log('[submit]', JSON.stringify(req, null, 2))
    onSubmit(req);
  }

  // ===================== render =====================

  return (
    <div className={styles.form}>

      <label className={styles.label}>
        Source
        <select
          className={styles.select}
          value={source}
          onChange={e => changeSource(e.target.value)}
        >
          <option value="">Select source</option>
          {sources.map(s => (
            <option key={s.name} value={s.name}>{s.name}</option>
          ))}
        </select>
      </label>

      {sourceFields.length > 0 && (
        <fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Fields</legend>
          <div className={styles.fields}>
            {sourceFields.map(f => (
              <label key={f} className={styles.checkbox}>
                <input
                  type="checkbox"
                  checked={selectedFlds.includes(f)}
                  onChange={() => toggleSelected(f)}
                />
                {f}
              </label>
            ))}
          </div>
        </fieldset>
      )}

      {/* ---- join ---- */}
      {jSrc && joinableSources.length > 0 && <fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Join With</legend>
          <div className={styles.joinRow}>
            <label className={styles.label}>Join Source
              <select className={styles.select} value={jSrc}
                onChange={e=>{setJSrc(e.target.value);setJFlds([])}}>
                <option value="">No join</option>
                {joinableSources.map(s=><option key={s.name} value={s.name}>{s.name}</option>)}
              </select></label>
            {jSrc&&<label className={styles.label}>Join Key
                <select className={styles.select} value={jKey} onChange={e=>setJKey(e.target.value)}>
                  {sourceFields.filter(f=>{
                    var o=sources.find(s=>s.name===jSrc); return o?o.fields.includes(f):false
                  }).map(f=><option key={f} value={f}>{f}</option>)}</select></label>}
          </div>
          {jSrc&&jSrcFlds.length>0&&<div className={styles.joinFields}>
              <span className={styles.joinFieldsLabel}>Join Source Fields</span>
              <div className={styles.fields}>{jSrcFlds.map(f=>
                <label key={f} className={styles.checkbox}>
                  <input type="checkbox" checked={jFlds.includes(f)}
                    onChange={()=>setJFlds(p=>p.includes(f)?p.filter(x=>x!==f):[...p,f])}/>{f}</label>)}
              </div></div>}
      </fieldset>}

      {/* union — disabled when a join is active (TQE can't do both) */}
      {unionCompat.length>0&&!jSrc&&<fieldset className={styles.fieldset}>
          <legend className={styles.legend}>Union With (same device type)</legend>
          <div className={styles.fields}>{unionCompat.map(s=>
            <label key={s.name} className={styles.checkbox}>
              <input type="checkbox" checked={uSrcs.includes(s.name)}
                onChange={()=>setUSrcs(p=>p.includes(s.name)?p.filter(x=>x!==s.name):[...p,s.name])}/>{s.name}</label>)}
          </div></fieldset>}

      {/* ---- filters ---- */}
      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>
          Filters
          <button type="button" className={styles.addBtn} onClick={addEmptyFilter}>+ Add</button>
        </legend>
        <div className={styles.filterList}>
          {filters.map((f, i) => (
            <FilterRow
              key={i}
              filter={f}
              fields={allFlds}
              onChange={updated => setFilters(prev => prev.map((x, idx) => idx === i ? updated : x))}
              onRemove={() => setFilters(prev => prev.filter((_, idx) => idx !== i))}
            />
          ))}
        </div>
      </fieldset>

      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>Aggregation</legend>
        <div className={styles.aggRow}>
          <select className={styles.select} value={aggFunction}
            onChange={e => setAggFunction(e.target.value as AggregationFunction)}>
            {SUPPORTED_AGGS.map(a => <option key={a} value={a}>{a}</option>)}
          </select>
          <select className={styles.select} value={aggField}
            onChange={e => setAggField(e.target.value)}>
            <option value="">No aggregation</option>
            {allFlds.map(f => <option key={f} value={f}>{f}</option>)}
          </select>
        </div>
      </fieldset>

      <fieldset className={styles.fieldset}>
        <legend className={styles.legend}>Window</legend>
        <div className={styles.windowRow}>
          <label className={styles.label}>Type
            <select className={styles.select} value={windowType}
              onChange={e => setWindowType(e.target.value as WindowType)}>
              {SUPPORTED_WINDOWS.map(w =>
                <option key={w} value={w}>{w}</option>
              )}
            </select>
          </label>
          <label className={styles.label}>Size (s)
            <input type="number" className={styles.input}
              value={windowSeconds} min={0}
              onChange={e => setWindowSeconds(Number(e.target.value))} />
          </label>
          {windowType === 'sliding' && (
            <label className={styles.label}>Slide (s)
              <input type="number" className={styles.input}
                value={slideSeconds} min={1}
                onChange={e => setSlideSeconds(Number(e.target.value))} />
            </label>
          )}
        </div>
      </fieldset>

      <button
        type="button"
        className={styles.submit}
        disabled={submitting || !source}
        onClick={doSubmit}
      >
        {submitting ? 'Running...' : 'Run Query'}
      </button>
    </div>
  );
}
