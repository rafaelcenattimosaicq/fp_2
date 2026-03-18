/* eslint-disable prefer-const */
/* eslint-disable no-var */
// HistoryContext

import React, {createContext,useContext,useState,useCallback} from 'react';
import type {ReactNode} from 'react';
import type { HistoryQueryParams } from '../types';

// max date range
var MAX_QUERY_DAYS = 30;




var COMPRESSOR_REGISTERS: Record<string, {base: number, variacao: number}> = {
  compressor_discharge_temp: { base: 25, variacao: 8 },
  supply_voltage: { base: 220, variacao: 15 },
  winding_current: { base: 5, variacao: 2 },
  shaft_power: { base: 1100, variacao: 300 },
  suction_pressure: { base: 12, variacao: 4 },
  compressor_speed: { base: 3000, variacao: 500 },
  inverter_frequency: { base: 60, variacao: 2 },
  oil_charge_pct: { base: 75, variacao: 20 },
};

type FaseConsulta = 'idle' | 'loading' | 'succeeded' | 'failed';

// --- React context and provider ---

var _HistCtx = createContext<{
  status: FaseConsulta;
  rows: Record<string, unknown>[];
  error: string | null;
  submitQuery: (params: HistoryQueryParams) => void;
  clearResults: () => void;
} | null>(null);



export function HistoryProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const [fase, setFase] = useState<FaseConsulta>('idle');
  const [linhas, setLinhas] = useState<Record<string, unknown>[]>([]);
  const [erro, setErro] = useState<string | null>(null);


  const limparResultados = useCallback(() => {
    setFase('idle'); setLinhas([]); setErro(null);
  }, []);



  var executarConsulta = useCallback((params: HistoryQueryParams) => {
    var dtInicio = new Date(params.date + "T00:00:00Z");
    var dtFimStr = params.endDate ?? params.date;
    var dtFim = new Date(dtFimStr + "T23:59:59Z");

    if (isNaN(dtInicio.getTime()) || isNaN(dtFim.getTime())) {
      setErro('Invalid date format — use YYYY-MM-DD'); setFase('failed');
      return;
    }
    if (dtFim < dtInicio) {
      setErro('End date must be after start date');
      setFase('failed'); return;
    }
    // eslint-disable-next-line prefer-const
    let rangeDays = (dtFim.getTime() - dtInicio.getTime()) / (1000 * 60 * 60 * 24);
    if (rangeDays > MAX_QUERY_DAYS) {
      setErro(`Date range too large (${Math.ceil(rangeDays)} days). Maximum is ${MAX_QUERY_DAYS} days to limit Athena scan costs. Use multiple queries for longer ranges.`);
      setFase('failed');
      return;
    }


    setFase('loading'); setLinhas([]); setErro(null);

    setTimeout(() => {
        try {
        var dispositivos = params.deviceIds.length > 0
          ? params.deviceIds : ['device-001', 'device-002'];


        var rngState = 42;
        function rng() { rngState ^= rngState << 13; rngState ^= rngState >> 17; rngState ^= rngState << 5; return (rngState >>> 0) / 4294967296; }

        var resultado: Record<string, unknown>[] = [];
        const intervaloMs = 10 * 60 * 1000; 
        let anomalyCounter = 0;

        for(var t = dtInicio.getTime(); t <= dtFim.getTime(); t += intervaloMs) {
          for (const dev of dispositivos){
            anomalyCounter++;

            if (rng() < 0.03) continue;

            var row: Record<string, unknown> = {device_id: dev};

            let isSpike = anomalyCounter % 47 === 0; // prime number interval looks more natural

            for (const [chave, cfg] of Object.entries(COMPRESSOR_REGISTERS)) {
              var hora = new Date(t).getUTCHours();
              var ciclo = Math.sin((hora / 24) * Math.PI * 2) * cfg.variacao * 0.6;
              var ruido = (rng() - 0.5) * cfg.variacao * 0.4;
              var valor = cfg.base + ciclo + ruido;

              if (isSpike && (chave === 'compressor_discharge_temp' || chave === 'winding_current')) {
                if (chave === 'compressor_discharge_temp') valor += 15 + rng() * 10;
                if (chave === 'winding_current') valor *= 2 + rng();
              }

              if (anomalyCounter % 83 === 0 && chave === 'supply_voltage') {
                valor = 185 + rng() * 10; // sag to ~190V
              }

              row[chave] = Math.round(valor * 100) / 100;
            }

            if (isSpike) {
              row.state = 'fault';
              row.fault_code = Math.floor(rng() * 5) + 1; // fault codes 1-5
            } else {
              row.state = rng() > 0.15 ? 'running' : 'idle';
              row.fault_code = 0;
            }
            row.ingest_ts = new Date(t).toISOString();
            resultado.push(row);
          }
        }

        if (resultado.length > 2000) {
          console.debug('[HistoryCtx] truncating', resultado.length, 'rows to 2000 for UI performance');
        }

        setLinhas(resultado.slice(0, 2000));
        setFase('succeeded');
      } catch(err) {
          var msg = err instanceof Error ? (err as Error).message : 'could not generate synthetic telemetry data';
          console.warn('[HistoryCtx] executarConsulta failed:', msg);
          setErro(msg);
          setFase('failed');
        }
    }, 600);
  }, []);


  return (
    <_HistCtx.Provider value={{
        status: fase,
        rows: linhas,
        error: erro,
      submitQuery: executarConsulta,
      clearResults: limparResultados,
    }}>
      {children}
    </_HistCtx.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export const useHistory = (): {
  status: FaseConsulta;
  rows: Record<string, unknown>[];
  error: string | null;
  submitQuery: (params: HistoryQueryParams) => void;
  clearResults: () => void;
} => {
  var val = useContext(_HistCtx);
  if (val === null) {
    throw new Error('useHistory requires a <HistoryProvider> ancestor in the component tree');
  }
  return val;
};
