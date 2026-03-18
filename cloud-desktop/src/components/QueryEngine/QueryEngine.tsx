
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

// view streaming results.  The coordinator runs on ECS Fargate so we
// occasionally see 502 errors during rolling deployments; the submit
export function QueryEngine(): React.JSX.Element {
  const { sources, queries, submitQuery, removeQuery, renameQuery, loadingSources, selectedDevices, setSelectedDevices } = useQuery();
  const { devices } = useTelemetry();
  const [submitting, setSubmitting] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [prefs, setPrefs] = useState<QueryPreferences>(DEFAULT_PREFERENCES);

  // fire-and-forget submit, errors from the coordinator (e.g. 502 during
  // eCS task replacement) are swallowed here because the QueryResultCard
  const handleSubmit = useCallback(
    async (req: QueryRequest) => {
      setSubmitting(true);
      try {
        await submitQuery({ ...req, devices: selectedDevices });
      } catch {
      } finally {
        setSubmitting(false);
      }
    },
    [submitQuery, selectedDevices],
  );

  // TODO: add pagination instead of just slicing, for long-running queries
  const visibleQueries = queries.slice(0, prefs.maxResults);

  const srcNames = sources.map((s) => s.name);

  return (
    <>
      {/* accent bar at top of query panel */}
      <div className={styles.accent} />
      <div className={styles.toolbar}>
        <DeviceSelector
          devices={devices}
          selectedIds={selectedDevices}
          onSelectionChange={setSelectedDevices}
        />

        <div className={styles.spacer} />

        <div className={styles.settingsWrapper}>
          <button
            type="button"
            className={`${styles.gearBtn} ${settingsOpen ? styles.gearActive : ''}`}
            onClick={() => setSettingsOpen((prev) => !prev)}
            aria-label="Query settings"
            title="Query settings"
          >
            ⚙
          </button>

          {settingsOpen && (
            <QuerySettings
              sourceNames={srcNames}
              preferences={prefs}
              onChange={setPrefs}
              onClose={() => setSettingsOpen(false)}
            />
          )}
        </div>
      </div>

      <div className={
        prefs.layout === 'vertical'
          ? `${styles.content} ${styles.vertical}`
          : styles.content
      }>
        {/* source list comes from the NES coordinator REST API - if the
            gateway hasn't connected yet we show a loader */}
        {loadingSources ? (
          <p className={styles.loading}>Loading sources…</p>
        ) : (
          <QueryForm
            sources={sources}
            onSubmit={handleSubmit}
            submitting={submitting}
            defaults={prefs}
          />
        )}
        <QueryResultsList queries={visibleQueries} onRemove={removeQuery} onRename={renameQuery} />
      </div>
    </>
  );
}
