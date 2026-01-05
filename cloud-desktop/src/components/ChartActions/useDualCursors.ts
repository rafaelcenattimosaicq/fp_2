/* eslint-disable no-var */
import { useState, useCallback, useEffect, useRef } from 'react';
import type { CursorPosition, ECharts } from './types';
import { CURSOR_COLORS } from './types';

/*
 * Dual measurement cursors for the sensor chart.
 *
 * Click once to drop cursor1 (the "from" marker), click again to drop
 * cursor2 (the "to" marker). Third click restarts from cursor1.
 * SensorDeltaReadout consumes both positions to show the time + value
 * difference between them.
 *
 * The mark line rendering was originally in SensorChart itself but got
 * messy when we added the delta badge, so moved it here during
 * the refactor. getMarkLineConfig() returns the echarts option
 */

interface MarkEntry {
  xAxis: number;
  lineStyle: { color: string; type: 'solid'|'dashed'; width: number };
  label: { show: boolean };
}

// factored out because i kept mixing up which cursor gets dashed vs
// solid and it was annoying to debug every time
function mkMark(ts: number, color: string, dashed: boolean): MarkEntry {
  return {
    xAxis: ts,
    lineStyle: { color, type: dashed ? 'dashed' : 'solid', width: 2 },
    label: { show: false },    // we draw our own labels in the overlay
  };
}

export interface DualCursorsAPI {
  cursor1: CursorPosition | null;
  cursor2: CursorPosition | null;
  placeCursor: (timestamp: number) => void;
  clearCursors: () => void;
  moveCursor: (index: 0 | 1, timestamp: number) => void;
  getMarkLineConfig: () => { data: MarkEntry[] };
  //added this for the delta badgetrue when both cursors
  // are down and you can actually compute a range
  hasPair: boolean;
}

export function useDualCursors(chart: ECharts | null): DualCursorsAPI {
  const [c1, setC1] = useState<CursorPosition|null>(null);
  const [c2, setC2] = useState<CursorPosition|null>(null);
  const[placingSecond, setPlacingSecond] = useState(false);

  // --- placement (alternates c1 → c2 → c1 ...) ---

  const place = useCallback((val: number) => {
    if(!placingSecond){
      setC1({ timestamp: val });
      setC2(null);                 // wipe c2 so delta badge hides
      setPlacingSecond(true);
    } else {
      setC2({ timestamp: val });
      setPlacingSecond(false);
    }
  }, [placingSecond]);

  const clear = useCallback(() => {
    setC1(null); setC2(null); setPlacingSecond(false)
  }, []);

  const move = useCallback((idx: 0|1, val: number) => {
    if(idx === 0) setC1({ timestamp: val });
    else setC2({ timestamp: val });
  }, []);

  // --- mark line config for echarts ---------------------------------

  const getMarkLineConfig = useCallback((): { data: MarkEntry[] } => {
    var out: MarkEntry[] = [];
    if(c1) out.push(mkMark(c1.timestamp, CURSOR_COLORS.cursor1, false));
    if(c2) out.push(mkMark(c2.timestamp, CURSOR_COLORS.cursor2, true));
    return { data: out };
  }, [c1, c2]);

  // --- zrender click handler -----------------------------------------------
  //
  // NOTE: if need drag-to-move, that lives in useCursorDrag
  // which takes moveCursor as a callback. both hooks grab zr separately
  // which worried me at first but it works fine 
  const placeRef = useRef(place);
  placeRef.current = place;     // sync in render, useEffectEvent when??

  useEffect(() => {
    if (chart == null) return;
    var zr = chart.getZr();

    // originally this was a named function but the .off() cleanup
    // wasnt working with the ref indirection arrow + closure fixed it
    var onClick = (e: any) => {
      if (e.offsetX == null) return;
      var pt = chart.convertFromPixel('grid', [e.offsetX, e.offsetY ?? 0]);
      if(pt) placeRef.current(pt[0]);
    };
    zr.on('click', onClick);

    return () => { zr.off('click', onClick) };
  }, [chart]);

  return {
    cursor1: c1, cursor2: c2,
    placeCursor: place,
    clearCursors: clear,
    moveCursor: move,
    getMarkLineConfig,
    hasPair: c1 !== null && c2 !== null,
  };
}