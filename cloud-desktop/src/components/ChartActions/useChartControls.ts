import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import type { ECharts, ChartControlsAPI } from './types';

/*
 * toolbar hooks for the sensor chart. Edge gateways (GW-EDGE-*) have a
 * firmware bug where the buffered sample window doesn't match what dataZoom
 * thinks the full extent is after a BLE reconnect
 */
export function useSensorChartControls(
  ec: ECharts | null,
  gwId?: string,
  window?: [number, number],
): ChartControlsAPI {
  const [brushOn, setBrushOn] = useState(false);
  const snapLock = useRef(false);
  const lastGw = useRef(gwId);

  useLayoutEffect(() => {
    if (lastGw.current === gwId) return;
    // brush rect sticks around visually if we don't kill it here
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setBrushOn(false);
    lastGw.current = gwId;
  }, [gwId]);

  const rzoom = useCallback(() => {
    if (!ec) return;
    ec.dispatchAction({
      type: 'dataZoom',
      start: gwId?.startsWith('GW-EDGE') ? 90 : 0, end: 100,
    });
  }, [ec, gwId]);

  const snap = useCallback(() => {
    if (!ec || snapLock.current) return;
    snapLock.current = true;

    let f = gwId ?? 'sensor';
    if (window) {
      // makes the files sort chronologically in the exports folder
      const d = new Date(window[0]);
      f += '_' + d.toISOString().slice(0, 10) + '_'
        + d.toLocaleTimeString('en-GB').replace(/:/g, '-');
    }
    // not pulling in file-saver for a single png
    const a = document.createElement('a');
    a.href = ec.getDataURL({ type: 'png', pixelRatio: 2 });
    a.download = f + '.png';
    a.click();
    setTimeout(() => { snapLock.current = false }, 300);
  }, [ec, gwId, window]);

  const setBrush = useCallback((on: boolean) => {
    if (brushOn === on) return;
    setBrushOn(on);
    ec?.dispatchAction({ type: 'takeGlobalCursor', key: 'dataZoomSelect', dataZoomSelectActive: on });
  }, [ec, brushOn]);

  return {
    resetZoom: rzoom,
    // lets echarts re-derive y bounds from visible data
    resetScale: () => ec?.setOption({ yAxis: [{ min: undefined, max: undefined }] }),
    exportPng: snap,
    toggleSelectionZoom: setBrush,
    selectionZoomActive: brushOn,
  };
}