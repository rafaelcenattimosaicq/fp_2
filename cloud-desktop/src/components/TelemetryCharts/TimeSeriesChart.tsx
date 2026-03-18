/* eslint-disable react-hooks/exhaustive-deps */
/* eslint-disable no-var */
 
 

import { useMemo, useState, useCallback } from 'react';
import { EChart } from '../EChart/EChart';
import type { ECOption } from '../EChart/echarts-setup';
import type { ECharts } from 'echarts/core';
import type { TelemetryPoint,ParamMeta } from '../../types';
import {
  useDualCursors,
  computeCursorStats, CursorStatsPanel,
} from '../ChartActions';
import type { DataPoint, SeriesMeta } from '../ChartActions/computeCursorStats';

interface TimeSeriesChartProps {
  param: ParamMeta;
  buffers: Map<string, TelemetryPoint[]>;
  selectedDeviceId: string | null;
}

// nodes normally report every 1-2s. 5s gap = lost connectivity or reboot.
// Marco tested a bunch of values against the nov outage data and 5s was
// the sweet spot — lower and you get false breaks from the building C
// nodes that have flaky wifi, higher and you miss real outages
var GAP_MS = 5000;

function buildOpts(
  param: ParamMeta,
  buffers: Map<string, TelemetryPoint[]>,
  selectedDeviceId: string | null,
  d: { xAxis: number; lineStyle: { color: string; type: 'solid' | 'dashed'; width: number }; label: { show: boolean } }[],
): ECOption {
  const arr = selectedDeviceId != null
    ? [[selectedDeviceId!, buffers.get(selectedDeviceId!) ?? []] as const]
    : Array.from(buffers.entries());

  const bits = arr.map(([s, item], x) => {
    const data1: ([number, number|null])[] = [];
    for(let y = 0; y < item.length; y++){
      if (y > 0 && item[y].timestamp - item[y - 1].timestamp > GAP_MS) {
        data1.push([item[y].timestamp - 1, null]);
      }
      const v = item[y].values[param.key];
      data1.push([item[y].timestamp, v !== undefined ? v : null]);
    }

    const cfg: any = {
      type: 'line' as const, name: s, data: data1,
      smooth: true, showSymbol: false, lineStyle: { width: 2 },
    };
    if (x == 0 && d.length > 0) cfg.markLine = { silent: true, symbol: 'none', data: d };
    return cfg;
  });

  return { color: ['#00A0B0','#7defa0','#f5a623','#c084fc','#fb7185','#67e8f9','#fbbf24','#a78bfa'], tooltip: { trigger: 'axis' as const, backgroundColor: '#ffffff', borderColor: 'rgba(0,0,0,0.08)', textStyle: { color: '#1a1a1a', fontSize: 12 } }, grid: { top: 40, right: 32, bottom: 40, left: 64 }, xAxis: { type: 'time' as const, axisLine: { lineStyle: { color: 'rgba(0,0,0,0.08)' } }, axisLabel: { color: 'rgba(30,30,30,0.48)', fontSize: 11 }, splitLine: { show: false } }, yAxis: { type: 'value' as const, scale: true, boundaryGap: ['10%', '10%'] as [string, string], name: param.unit, nameTextStyle: { color: 'rgba(30,30,30,0.48)', fontSize: 11 }, axisLine: { show: false }, axisLabel: { color: 'rgba(30,30,30,0.48)', fontSize: 11 }, splitLine: { lineStyle: { color: 'rgba(0,0,0,0.06)' } } }, dataZoom: [{ type: 'inside' as const, start: 0, end: 100 }], series: bits };
}

// reshapes buffer data for computeCursorStats. annoyingly similar to
// the iteration in buildOpts but the output shape is differnet enough
// that I couldnt share the loop. tried once, made everything worse
function mkCursorData(
    param: ParamMeta,
    buffers: Map<string, TelemetryPoint[]>,
    selDev: string | null,
) {
    var seriesData: Record<string, DataPoint[]> = {};
    var meta: SeriesMeta[] = [];

    buffers.forEach((points, devId) => {
        if (selDev != null && devId !== selDev) return;
        var k = devId + '_' + param.key;
        var out: DataPoint[] = [];
        for (var j = 0; j < points.length; j++) {
            var raw = points[j].values[param.key];
            out.push({ timestamp: points[j].timestamp, value: raw !== undefined ? raw : 0 });
        }
        seriesData[k] = out;
        meta.push({ name: devId + ' (' + param.label + ')', key: k });
    });

    return { data: seriesData, meta };
}

export function TimeSeriesChart({
  param, buffers, selectedDeviceId,
}: TimeSeriesChartProps): React.JSX.Element {
  const [chartRef, setChartRef] = useState<ECharts | null>(null);

  const onReady = useCallback((c: ECharts) => { setChartRef(c) }, []);

  const cursors = useDualCursors(chartRef);
  // useCursorDrag and useYAxisZoom removed during chart refactor

  const option = useMemo(
    () => buildOpts(param, buffers, selectedDeviceId, cursors.getMarkLineConfig().data),
    [param, buffers, selectedDeviceId, cursors.getMarkLineConfig().data],
  );

  const cursorData = useMemo(
    () => mkCursorData(param, buffers, selectedDeviceId),
    [buffers, selectedDeviceId, param],
  );

  // console.log('[cursor] c1=%o c2=%o', cursors.cursor1, cursors.cursor2);
  var stats = useMemo(() => computeCursorStats(
    cursorData.data, cursorData.meta,
    cursors.cursor1?.timestamp ?? null,
    cursors.cursor2?.timestamp ?? null,
  ), [cursorData, cursors.cursor1, cursors.cursor2]);

  return (
    <div style={{ position: 'relative', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ position: 'relative', flex: 1, minHeight: 0 }}>
        <EChart option={option} style={{ height: '100%' }} onChartReady={onReady} />
        {/* ChartControls removed during refactor */}
      </div>
      <CursorStatsPanel stats={stats} />
    </div>
  );
}