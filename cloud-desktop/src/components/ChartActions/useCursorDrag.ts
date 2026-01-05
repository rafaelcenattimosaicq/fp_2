/* eslint-disable no-var */
import { useState, useCallback, useEffect, useRef } from 'react';
import type { CursorPosition, ECharts } from './types';
import { CURSOR_COLORS } from './types';

/*
 * Dual measurement cursors for the sensor chart.
 *
 * Originally this was two separate hooks (useCursorPlacement +
 * useCursorDrag) but having two effects that both grabbed zr caused
 * a double-fire on chrome 120. Merged them here, kept the split
 * internally with the "placement" and "dragging" sections below.
 *
For the field tablet build we'll probably need to bump it
 * or switch to touch events entirely 
 */

// --- mark line generation (used by SensorChart to render the cursors) ---

interface MarkEntry {
  xAxis: number;
  lineStyle: { color: string; type: 'solid'|'dashed'; width: number };
  label: { show: boolean };
}

// this used to be inline in getMarkLineConfig but i kept getting the
// dashed/solid backwards so i pulled it out
function mkMark(ts: number, clr: string, dashed: boolean): MarkEntry {
  return {
    xAxis: ts,
    lineStyle: { color: clr, type: dashed ? 'dashed' : 'solid', width: 2 },
    label: { show: false },   // labels handled by SensorCursorOverlay instead
  };
}

// --- hit testing -----------------------------=---------------------------

const GRAB_PX = 20;

// exported because SensorCursorOverlay also uses it for hover styling
export function isWithinHitZone(mx: number, cx: number) {
  return cx >= 0 && Math.abs(mx - cx) <= GRAB_PX
}

// --- the actual hook --------------------------------------------------

export interface DualCursorsAPI {
  cursor1: CursorPosition | null;
  cursor2: CursorPosition | null;
  placeCursor: (timestamp: number) => void;
  clearCursors: () => void;
  moveCursor: (index: 0 | 1, timestamp: number) => void;
  getMarkLineConfig: () => { data: MarkEntry[] };
  // added in v2 when needed it for the delta readout badge
  selectionComplete: boolean;
}

export function useDualCursors(chart: ECharts | null): DualCursorsAPI {
  const [c1, setC1] = useState<CursorPosition|null>(null);
  const [c2, setC2] = useState<CursorPosition|null>(null);
  const[placingSecond, setPlacingSecond] = useState(false);

  // --- placement (alternates c1 → c2 → c1 ...) ---

  const place = useCallback((val: number) => {
    if(!placingSecond){
      setC1({ timestamp: val });
      setC2(null);               // wipe c2 when restarting
      setPlacingSecond(true);
    } else {
      setC2({ timestamp: val });
      setPlacingSecond(false);
    }
  }, [placingSecond]);

  // used by the toolbar "X" button and also when gateway changes

  const clear = useCallback(() => {
    setC1(null); setC2(null); setPlacingSecond(false)
  }, []);

  const move = useCallback((idx: 0|1, val: number) => {
    if(idx === 0) setC1({ timestamp: val });
    else setC2({ timestamp: val });
  }, []);

  // --- mark line config for echarts --------------------------------------

  const getMarkLineConfig = useCallback((): { data: MarkEntry[] } => {
    var out: MarkEntry[] = [];
    if(c1) out.push(mkMark(c1.timestamp, CURSOR_COLORS.cursor1, false));
    if(c2) out.push(mkMark(c2.timestamp, CURSOR_COLORS.cursor2, true));
    return { data: out };
  }, [c1, c2]);

  // --- zrender event wiring (click-to-place + drag-to-move) ----------------
  //

  const refs = useRef({ place, move, c1, c2 });
  refs.current = { place, move, c1, c2 };

  useEffect(() => {
    if (!chart) return;
    var zr = chart.getZr();
    var dragging: 0|1|null = null;
    // flag to distinguish drag-end from click
    var wasDrag = false;

    zr.on('mousedown', (e: any) => {
      if(e.offsetX == null) return;
      wasDrag = false;

      // convertToPixel throws if grid isnt ready (first render race,
      // found march 14)
      var px1 = chart.convertToPixel('grid', [refs.current.c1?.timestamp??0, 0]);
      var px2 = chart.convertToPixel('grid', [refs.current.c2?.timestamp??0, 0]);

      if(refs.current.c1 && px1 && isWithinHitZone(e.offsetX, px1[0])) dragging = 0;
      else if(refs.current.c2 && px2 && isWithinHitZone(e.offsetX, px2[0])) dragging = 1;

      if(dragging != null && zr.dom)
        (zr.dom as HTMLElement).style.cursor = 'ew-resize';
    });

    zr.on('mousemove', (e: any) => {
      if(dragging == null || e.offsetX == null) return;
      wasDrag = true;
      var pt = chart.convertFromPixel('grid', [e.offsetX, e.offsetY||0]);
      if(pt) refs.current.move(dragging, pt[0]);
    });

    var endDrag = () => {
      if(dragging == null) return;
      dragging = null;
      if(zr.dom)(zr.dom as HTMLElement).style.cursor = '';
    };
    zr.on('mouseup', endDrag);
    zr.on('globalout', endDrag);   // ew-resize sticks if you drag off edge

    zr.on('click', (e: any) => {
      if(wasDrag) { wasDrag = false; return; }   // was a drag, not a click
      if(e.offsetX == null) return;
      var pt = chart.convertFromPixel('grid', [e.offsetX, e.offsetY||0]);
      if(pt) refs.current.place(pt[0]);
    });

    return () => {
      // we re-bindwhen chart ref changes anyway
      zr.off('mousedown'); zr.off('mousemove');
      zr.off('mouseup'); zr.off('click'); zr.off('globalout');
    };
  }, [chart]);

  return {
    cursor1: c1, cursor2: c2,
    placeCursor: place,
    clearCursors: clear,
    moveCursor: move,
    getMarkLineConfig,
    selectionComplete: c1 !== null && c2 !== null,
  };
}