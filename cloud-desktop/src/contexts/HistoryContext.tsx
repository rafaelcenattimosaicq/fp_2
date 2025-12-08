import React, {createContext, useContext, useReducer, useCallback} from 'react';
import type {ReactNode} from 'react';
import type { HistoryQueryParams } from '../types';

type QueryPhase = 'idle' | 'loading' | 'succeeded' | 'failed';

type HistoryState = {
  phase: QueryPhase;
  rows: Record<string, unknown>[];
  error: string | null;
};

type HistoryAction =
  | { type: 'SUBMIT' }
  | { type: 'SUCCESS'; rows: Record<string, unknown>[] }
  | { type: 'FAILURE'; error: string }
  | { type: 'RESET' };

function historyReducer(_state: HistoryState, action: HistoryAction): HistoryState {
    if (action.type === 'SUBMIT') {
    return { phase: 'loading', rows: [], error: null };
  }
  else if(action.type === 'SUCCESS'){
      return { phase: 'succeeded', rows: action.rows, error: null };
  } else if (action.type === 'FAILURE') {
    return {phase: 'failed', rows: [], error: action.error};
  }
  return { phase: 'idle', rows: [], error: null };
}

const HistoryContext = createContext<{
  status: QueryPhase;
  rows: Record<string, unknown>[];
  error: string | null;
  submitQuery: (params: HistoryQueryParams) => void;
  clearResults: () => void;
} | null>(null);

function seededRandom(seed: number): () => number {
  let num = seed;
  return () => {
    num ^= num << 13;
    num ^= num >> 17;
    num ^= num << 5;
    return (num >>> 0) / 4294967296;
  };
}

// generates fake telemetry rows for the history view in demo mode
// the metrics here match what our modbus devices actually report
function generateFakeRows(
  startDate: string,
  endDate: string,
  deviceIds: string[]
): Record<string, unknown>[] {
  const tmp1 = new Date(startDate + "T00:00:00Z");
  const tmp2 = new Date(endDate + "T23:59:59Z");

  const myRand = seededRandom(42);
  const devices = deviceIds.length > 0 ? deviceIds : ['device-001', 'device-002'];

  const result: Record<string, unknown>[] = [];
  const interval = 10 * 60 * 1000;

  // these ranges are roughly based on the actual ESP32 sensor readings we get
  const metrics: Record<string, {base: number, drift: number}> = {
    temperature: { base: 25, drift: 8 },
    voltage: { base: 220, drift: 15 },
    current: { base: 5, drift: 2 },
    power: { base: 1100, drift: 300 },
    pressure: { base: 12, drift: 4 },
    compressor_speed: { base: 3000, drift: 500 },
    frequency: { base: 60, drift: 2 },
    state_of_charge: { base: 75, drift: 20 },
  };

  for(let t = tmp1.getTime(); t <= tmp2.getTime(); t += interval) {
    for (const dev of devices){
      const row: Record<string, unknown> = {device_id: dev};

      for (const [key, cfg] of Object.entries(metrics)) {
        // sinusoidal pattern to simulate daily cycles (temp goes up during day etc)
        const hour = new Date(t).getUTCHours();
        const sinVal = Math.sin((hour / 24) * Math.PI * 2) * cfg.drift * 0.6;
        const noise = (myRand() - 0.5) * cfg.drift * 0.4;
        row[key] = Math.round((cfg.base + sinVal + noise) * 100) / 100;
      }

      row.state = myRand() > 0.15 ? 'running' : 'idle';
      row.ingest_ts = new Date(t).toISOString();
      result.push(row);
    }
  }

  return result;
}

export function HistoryProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const [state, dispatch] = useReducer(historyReducer, {
      phase: 'idle' as QueryPhase,
      rows: [],
      error: null,
  });

  const clearResults = useCallback(() => {
      dispatch({ type: 'RESET' });
  }, []);

  // handle query submission - for now this just generates fake data
  // TODO: hook this up to the actual S3 history API once the lambda is deployed
  const submitQuery = useCallback((params: HistoryQueryParams) => {
    dispatch({type: 'SUBMIT'});

    setTimeout(() => {
        try {
        const endDate = params.endDate ?? params.date;
        const allRows = generateFakeRows(params.date, endDate, params.deviceIds);
        // console.log('generated', allRows.length, 'rows for history query');
        dispatch({ type: 'SUCCESS', rows: allRows.slice(0, 2000) });
      } catch(err) {
          const msg = err instanceof Error ? (err as Error).message : 'could not generate data';
        dispatch({ type: 'FAILURE', error: msg });
        }
    }, 600);
  }, []);

  return (
    <HistoryContext.Provider value={{
        status: state.phase,
        rows: state.rows,
        error: state.error,
      submitQuery,
      clearResults,
    }}>
      {children}
    </HistoryContext.Provider>
  );
}

export const useHistory = (): {
  status: QueryPhase;
  rows: Record<string, unknown>[];
  error: string | null;
  submitQuery: (params: HistoryQueryParams) => void;
  clearResults: () => void;
} => {
  const val = useContext(HistoryContext);
  if (val === null) {
    throw new Error('useHistory requires a <HistoryProvider> ancestor in the component tree');
  }
  return val;
};
