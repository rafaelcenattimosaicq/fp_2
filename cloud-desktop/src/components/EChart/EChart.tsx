/* eslint-disable no-var */
import { useEffect, useRef,
  // useState,
} from 'react';
import type { CSSProperties } from 'react';
import { init,getInstanceByDom } from 'echarts/core';
import type { ECharts, SetOptionOpts } from 'echarts/core';
import './echarts-setup';      // registers bar, line, scatter, etc
import type { ECOption } from './echarts-setup';
import styles from './EChart.module.css';
// import { useChartTheme } from '../ThemeProvider';  // TODO: use up when dark

// replaced echarts-for-react

export interface EChartProps {
  option: ECOption
  style?: CSSProperties
  settings?: SetOptionOpts
  loading?: boolean
  theme?: 'light' | 'dark'
  onChartReady?: (chart: ECharts) => void
}

// was inline in EChart
function useChartInit(
  ref: React.RefObject<HTMLDivElement | null>,
  theme: string,
  onReady: React.MutableRefObject<EChartProps['onChartReady']>,
) {
  useEffect(function() {
    var el = ref.current; if(!el) return

    var inst: ECharts | undefined, gone = false

    if(el.clientWidth > 0 && el.clientHeight > 0) {
      inst = init(el, theme); onReady.current?.(inst)
    }

    // deferred init for 0x0 containers (sidebar collapsed on load).
    var ro = new ResizeObserver(function() {
      if(gone) return
      if(!inst && el.clientWidth > 0 && el.clientHeight > 0){
        inst = init(el, theme); onReady.current?.(inst) }
      if(inst && el.clientWidth > 0) inst.resize()
    })
    ro.observe(el)

    return function cleanup() { gone = true; ro.disconnect()
      inst?.dispose() }
  }, [theme])
}

// echarts tooltip uses !important so we need this to force our
// font. tried putting it in the CSS module but the tooltip renders
// outside the container div.
var _tooltipStyleInjected = false
function injectTooltipHack() {
  if(_tooltipStyleInjected) return
  var s = document.createElement('style')
  s.textContent = `.ec-tooltip { font-family: inherit !important; font-size: 12px !important }`
  document.head.appendChild(s)
  _tooltipStyleInjected = true
}

export function EChart({option, style, settings, loading, theme, onChartReady}: EChartProps): React.JSX.Element {
  var ref = useRef<HTMLDivElement>(null)
  var readyCb = useRef(onChartReady); readyCb.current = onChartReady

  useChartInit(ref, theme ?? 'light', readyCb)

  useEffect(function() {
    var el = ref.current; if(!el) return
    var c = getInstanceByDom(el); if(!c) return

    // setOption merges by default which is usually what we want, but
    // when switching between sensor types the old series config bleeds
    c.setOption(option, settings ?? {})
    if(loading) c.showLoading(); else c.hideLoading()
  }, [option, settings, loading])

  useEffect(function() { injectTooltipHack() }, [])    // one-time, whatever

  // stale options on sensor chart. leaving til source found
  if(process.env.NODE_ENV !== 'production')
    console.log("echart render", option?.series)

  return <div ref={ref} data-testid="echart-container"
    className={styles.container}
      style={{height: '300px', ...style}} />
}