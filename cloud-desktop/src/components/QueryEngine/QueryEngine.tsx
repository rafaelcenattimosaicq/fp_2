/* eslint-disable no-var */
import { useCallback, useState } from 'react';
import { useQuery } from '../../contexts/QueryContext';
import { useTelemetry } from '../../contexts/TelemetryContext';
import { DeviceSelector } from './DeviceSelector';
import { QueryForm } from './QueryForm';
import { QueryResultsList } from './QueryResultsList';
import { QuerySettings, DEFAULT_PREFERENCES } from './QuerySettings';
import type { QueryPreferences } from './QuerySettings';
import type { QueryRequest } from '../../types';
import styles from './QueryEngine.module.css';

// glue component for the query tab. wires DeviceSelector + QueryForm
// + results list + settings panel together. most of the real logic
// lives in QueryContext, this just manages the UI state

export function QueryEngine(): React.JSX.Element {
  var { sources, queries, submitQuery, removeQuery,
        selectedDevices, setSelectedDevices, loadingSources } = useQuery();
  var { devices } = useTelemetry();

  var [busy, setBusy] = useState(false)
  var [showSettings,setShowSettings] = useState(false)
  var [prefs, setPrefs] = useState<QueryPreferences>(DEFAULT_PREFERENCES)

  var doSubmit = useCallback(async (req: QueryRequest) => {
    setBusy(true)
    // console.log('submitting query', req, selectedDevices);
    try {
      await submitQuery({...req, devices: selectedDevices})
    } catch {

    } finally { setBusy(false) }
  }, [submitQuery, selectedDevices])

  // TODO: query timeout  now a bad NES query just hangs

  return <>
      <div className={styles.accent} />
      <div className={styles.toolbar}>
        <DeviceSelector devices={devices}
          selectedIds={selectedDevices}
          onSelectionChange={setSelectedDevices} />

        <div className={styles.spacer} />
        <div className={styles.settingsWrapper}>
          <button type="button"
            className={showSettings ? styles.gearBtn+' '+styles.gearActive : styles.gearBtn}
            onClick={() => setShowSettings(v => !v)}
            title="Query settings">⚙</button>

          {showSettings && <QuerySettings
              sourceNames={sources.map(s => s.name)}
              preferences={prefs}
              onChange={setPrefs}
              onClose={() => setShowSettings(false)} />}
        </div>
      </div>

      <div className={prefs.layout === 'vertical'
        ? styles.content+' '+styles.vertical
        : styles.content}>
        {loadingSources
          ? <p className={styles.loading}>Loading sources…</p>
          : <QueryForm sources={sources} onSubmit={doSubmit}
              submitting={busy} defaults={prefs} />}
        <QueryResultsList
          queries={queries.slice(0, prefs.maxResults)}
          onRemove={removeQuery} />
      </div>
  </>
}