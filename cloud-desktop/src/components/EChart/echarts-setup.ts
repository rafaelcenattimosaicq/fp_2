/* eslint-disable react-hooks/rules-of-hooks */
// ^ not actually using hooks, echarts' use() triggers the lint rule

import { use } from 'echarts/core';
import { LineChart,BarChart, GraphChart } from 'echarts/charts';
// import { ScatterChart } from 'echarts/charts';
import { CanvasRenderer } from 'echarts/renderers';
// tried SVGRenderer for better print quality but it breaks the
// png export in useSensorChartControls (getDataURL needs canvas)
import {
  TitleComponent,
  TooltipComponent,
  GridComponent,
  LegendComponent,
  DataZoomComponent,
  MarkLineComponent,
  GraphicComponent,
  ToolboxComponent,
  // VisualMapComponent,   // heatmap maybe ashould use in the future, in the report mentioned it as possivle futture work
                           // thermal view are cool for geospatial queries
} from 'echarts/components';

import type { ComposeOption } from 'echarts/core';
import type { LineSeriesOption, BarSeriesOption,GraphSeriesOption } from 'echarts/charts';

import type {
  TitleComponentOption, TooltipComponentOption, GridComponentOption,
  LegendComponentOption, DataZoomComponentOption, MarkLineComponentOption,
  GraphicComponentOption,ToolboxComponentOption,
} from 'echarts/components';

use([
  LineChart, BarChart, GraphChart,
  TitleComponent, TooltipComponent, GridComponent,
  LegendComponent, DataZoomComponent, MarkLineComponent,
  GraphicComponent, ToolboxComponent,
  CanvasRenderer,
]);

// ---- ECOption type -------------------------------------------------
// union of all chart + component options 

type ChartOpts =
  | LineSeriesOption
  | BarSeriesOption
  | GraphSeriesOption;

type CompOpts =
  | TitleComponentOption
  | TooltipComponentOption
  | GridComponentOption
  | LegendComponentOption
  | DataZoomComponentOption

  | MarkLineComponentOption
  | GraphicComponentOption
  | ToolboxComponentOption;

export type ECOption = ComposeOption<ChartOpts | CompOpts>;