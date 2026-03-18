/* eslint-disable no-var */
import { useState, useEffect, useMemo } from 'react';
import { useMqtt } from '../../contexts/MqttContext';
import { useNesStream } from '../../hooks/useNesStream';
import type { NesStreamStatus } from '../../hooks/useNesStream';
import { parseTelemetry } from '../../utils/parseTelemetry';
import { TimeSeriesChart } from '../TelemetryCharts/TimeSeriesChart';
import { DEFAULT_PARAMS, getParamMeta } from '../../types';
import type { TelemetryPoint } from '../../types';
import styles from './NesTelemetry.module.css';

const TOPICO_NES = 'nebulastream/telemetry';

// 5min rolling window. 
const TAMANHO_BUFFER_MS = 5 * 60 * 1000;

// fields that come through in every telemetry point but arent sensor
// data. DEVICE_ID is uppercase because the NES output schema uses
// caps, device_id is lowercase from the raw mqtt payloads. both show
// up depending on whether the data went through NES or not
const CHAVES_IGNORADAS = new Set(['DEVICE_ID', 'GATEWAY_ID', 'timestamp', 'device_id']);

const CLASSE_DO_PONTO: Record<NesStreamStatus, string> = {
  discovering: styles.dotConnecting,
  ready: styles.dotIdle,
  submitting: styles.dotConnecting,
  running: styles.dotRunning,
  error: styles.dotError,
};

const TEXTO_DO_STATUS: Record<NesStreamStatus, string> = {
  discovering: 'Discovering sources...',
  ready: 'Ready',
  submitting: 'Submitting query...',
  running: 'Streaming',
  error: 'Error',
};

// mutable buffer lives outside react state so the mqtt callback doesnt
// trigger a re-render per message. we copy into state periodically.
let buffersMutaveis = new Map<string, TelemetryPoint[]>();

export function NesTelemetry(): React.JSX.Element {
  const { subscribe } = useMqtt();
  var nes = useNesStream();

  const [activeTab, setActiveTab] = useState<string>('');
  const [data, setData] = useState<Map<string, TelemetryPoint[]>>(() => new Map());

  // subscribe to NES telemetry topic and accumulate into mutable buffer.
  // setData(new Map(...)) copies the buffer into state which triggers

  useEffect(() => {
    buffersMutaveis = new Map<string, TelemetryPoint[]>();

    var unsub = subscribe(TOPICO_NES, (_topic, payload) => {
      var item = parseTelemetry(payload);
      if(item == null) return;
      // console.log('nes telemetry point:', item.deviceId, Object.keys(item.values));

      var buf = buffersMutaveis.get(item.deviceId) ?? [];
      buf.push(item);

      var cutoff = Date.now() - TAMANHO_BUFFER_MS;
      buffersMutaveis.set(item.deviceId, buf.filter(p => p.timestamp >= cutoff));
      setData(new Map(buffersMutaveis));
    });

    return () => { unsub(); buffersMutaveis = new Map() };
  }, [subscribe]);

  // discover which params exist in the data, keeping DEFAULT_PARAMS
  var allParams = useMemo(() => {
    var seen = new Set<string>();
    for(var arr of data.values()) {
      for(var pt of arr) {
        for(var k of Object.keys(pt.values)) {
          if(!CHAVES_IGNORADAS.has(k)) seen.add(k);
        }
      }
    }

    var ordered = DEFAULT_PARAMS.filter(k => seen.has(k));
    var rest = new Set(DEFAULT_PARAMS);
    for(var k2 of seen){
      if(!rest.has(k2)) ordered.push(k2);
    }
    return ordered.length > 0 ? ordered : DEFAULT_PARAMS;
  }, [data]);

  var param = (activeTab !== '' && allParams.includes(activeTab)) ? activeTab : allParams[0];
  var paramMeta = getParamMeta(param);


  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    var id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  var filtered = useMemo(() => {
    var cutoff = now - TAMANHO_BUFFER_MS;
    var out = new Map<string, TelemetryPoint[]>();
    for(var [devId, pts] of data.entries()) {
      var kept = pts.filter(p => p.timestamp >= cutoff);
      if(kept.length > 0) out.set(devId, kept);
    }
    return out;
  }, [data, now]);

  var hasData = data.size > 0;

  // --- NES control button state -------------------------------------
  // this was originally a separate <NesControlButton> component but
  // it needed the nes hook star
  var nesBtnTitle = nes.status === 'running'
    ? `Query #${nes.queryId} - click to stop`
    : nes.status === 'error'
    ? `${nes.error} - click to retry`
    : 'Click to refresh sources';

  var nesBtnClick = nes.status === 'running' ? nes.stop : nes.refresh;
  var nesBtnClass = `${styles.nesBtn} ${nes.status === 'running' ? styles.nesBtnActive : ''} ${nes.status === 'error' ? styles.nesBtnError : ''}`;

  // --- query panel (hidden while running) 

  var queryPanel = null;
  if(nes.status !== 'running') {
    queryPanel = (
      <div className={styles.queryPanel}>
        <div className={styles.queryHeader}>
          <span className={`${styles.dot} ${CLASSE_DO_PONTO[nes.status]}`} />
          <span className={styles.statusLabel}>{TEXTO_DO_STATUS[nes.status]}</span>
          {nes.source && (
            <span className={styles.sourceLabel}>
              Source: <code>{nes.source}</code>
            </span>
          )}
        </div>

        {nes.dsl && <div className={styles.dslBlock}>
          <div className={styles.dslLabel}>NES C++ DSL QUERY</div>
          <pre className={styles.dslCode}>{nes.dsl}</pre>
        </div>}

        {nes.error && <div className={styles.errorMsg}>{nes.error}</div>}

        {nes.sources.length > 0 && (
          <div className={styles.sourceList}>
            <span className={styles.sourceListLabel}>
              {nes.sources.length} source{nes.sources.length !== 1 ? 's' : ''} available:
            </span>
            {nes.sources.map(s =>
              <code key={s.name} className={styles.sourceTag}>{s.name}</code>
            )}
          </div>
        )}

        <div className={styles.actions}>
          {nes.status === 'ready' ? (
            <button type="button" className={styles.runBtn} onClick={nes.start}>
              Run Query
            </button>
          ) : nes.status === 'error' ? (
            <button type="button" className={styles.retryBtn} onClick={nes.refresh}>Retry</button>
          ) : (nes.status === 'submitting' || nes.status === 'discovering') ? (
            <span className={styles.waitLabel}>{TEXTO_DO_STATUS[nes.status]}</span>
          ) : null}
        </div>
      </div>
    );
  }

  return (
    <div className={styles.container}>

      <div className={styles.tabBar} role="tablist">
        {allParams.map(p => (
          <button
            key={p} type="button" role="tab"
            aria-selected={p === param}
            className={`${styles.tab} ${p === param ? styles.active : ''}`}
            onClick={() => setActiveTab(p)}
          >
            {getParamMeta(p).label}
          </button>
        ))}
        <div className={styles.spacer} />

        <button
          type="button"
          className={nesBtnClass}
          onClick={nesBtnClick}
          title={nesBtnTitle}
        >
          <span className={`${styles.dot} ${CLASSE_DO_PONTO[nes.status]}`} />
          NES
        </button>
      </div>

      {queryPanel}

      <div className={styles.chartArea}>
        {hasData
          ? <TimeSeriesChart param={paramMeta} buffers={filtered} selectedDeviceId={null} />
          : <div className={styles.placeholder}>
              {nes.status === 'running'
                ? 'Waiting for NES query results...'
                : 'Run a query to see NES-processed telemetry'}
            </div>
        }
      </div>
    </div>
  );
}