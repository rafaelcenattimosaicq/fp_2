import { useEffect, useRef } from 'react';
import type { CSSProperties } from 'react';
import { init, getInstanceByDom } from 'echarts/core';
import type { ECharts, SetOptionOpts } from 'echarts/core';
import './echarts-setup';
import type { ECOption } from './echarts-setup';
import styles from './EChart.module.css';

export interface EChartProps {
  option: ECOption;
  style?: CSSProperties;
  settings?: SetOptionOpts;
  loading?: boolean;
  theme?: 'light' | 'dark';
  onChartReady?: (chart: ECharts) => void;
}

  // debug: log chart lifecycle events to help diagnose the blank-chart
  // issue on Pi touchscreen (7" display, 800x480). Remove once fixed.
  // const DEBUG_CHART = import.meta.env.DEV;
  // function chartLog(msg: string) {
  //   if (DEBUG_CHART) console.log('[EChart]', msg);
  // }

/*
 * Thin ECharts wrapper that handles init, resize, and disposal.
 *
 * ECharts has a longstanding quirk where calling init() on a hidden element
 * (e.g. inactive tab) creates a zero-dimension canvas that never repaints.
 * The ResizeObserver below works around this - it defers init until the
 * container has a real size.  On a 7" Pi display the chart area can be as
 * small as ~480 px wide, so the resize guard matters even on "normal" screens.
 */
export function EChart(props: EChartProps): React.JSX.Element {
  const { option, style, settings, theme = 'light', loading = false } = props;
  const chartRef = useRef<HTMLDivElement>(null);

  const readyCb = useRef(props.onChartReady);
  useEffect(() => { readyCb.current = props.onChartReady; }, [props.onChartReady]);

  useEffect(() => {
    const el = chartRef.current;
    if (!el) return;

    let inst: ECharts | undefined;
    let dead = false; // cleanup flag

    inst = init(el, theme);
    readyCb.current?.(inst);

    const ro = new ResizeObserver(() => {
      if (dead) return;
      if (!inst) {
        inst = init(el, theme);
        readyCb.current?.(inst);
      }
      inst?.resize();
    });
    ro.observe(el);

    return () => { dead = true; ro.disconnect(); inst?.dispose(); };
  }, [theme]);

  useEffect(() => {
    const el = chartRef.current;
    if (!el) return;
    getInstanceByDom(el)?.setOption(option, settings);
  }, [option, settings]);

  useEffect(() => {
    const el = chartRef.current;
    if (!el) return;
    const c = getInstanceByDom(el);
    if (loading) { c?.showLoading(); } else { c?.hideLoading(); }
  }, [loading]);

  return (
    <div
      ref={chartRef}
      data-testid="echart-container"
      className={styles.container}
      style={{ height: '300px', ...style }}
    />
  );
}
