import type { BlockType } from './types';
import { TelemetryCharts } from '../../components/TelemetryCharts/TelemetryCharts';
import { NesTelemetry } from '../../components/NesTelemetry/NesTelemetry';
import { AlertsFeed } from '../../components/AlertsFeed/AlertsFeed';
import { QueryEngine } from '../../components/QueryEngine/QueryEngine';
import { HistoryExplorer } from '../../components/HistoryExplorer/HistoryExplorer';

interface Props {
  type: BlockType
}

// renders the right component based on block type
export function Block({ type }: Props): React.JSX.Element {
  // console.log('rendering block:', type);
  if (type === 'telemetry') return <TelemetryCharts />
  if (type === 'nesTelemetry') return <NesTelemetry />
  if (type === 'alerts') return <AlertsFeed />
  if (type === 'queryEngine') return <QueryEngine />;
  return <HistoryExplorer />;
}
