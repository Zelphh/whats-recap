import { useEffect, useRef } from "react";
import * as echarts from "echarts/core";
import { BarChart, LineChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { ChartTheme } from "../theme";
import { fmtInt } from "../format";

echarts.use([BarChart, LineChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, CanvasRenderer]);

export type ChartOption = echarts.EChartsCoreOption;

interface Props {
  option: ChartOption;
  height?: number;
  ariaLabel: string;
}

export function Chart({ option, height = 260, ariaLabel }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    const c = echarts.init(ref.current, undefined, { renderer: "canvas" });
    chart.current = c;
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      c.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    chart.current?.setOption(option, { notMerge: true });
  }, [option]);

  return <div ref={ref} role="img" aria-label={ariaLabel} style={{ width: "100%", height }} />;
}

/** Opções comuns: grade e eixos recessivos, tooltip com os tokens do tema. */
export function baseOption(t: ChartTheme): ChartOption {
  return {
    backgroundColor: "transparent",
    textStyle: { fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif", color: t.text2 },
    grid: { left: 8, right: 12, top: 16, bottom: 8, containLabel: true },
    tooltip: {
      backgroundColor: t.surface,
      borderColor: t.axis,
      textStyle: { color: t.text, fontSize: 12 },
      extraCssText: "border-radius: 8px; box-shadow: 0 4px 16px rgba(0,0,0,.12);",
    },
  };
}

export function categoryAxis(t: ChartTheme, data: string[], extra: Record<string, unknown> = {}) {
  return {
    type: "category",
    data,
    axisLine: { lineStyle: { color: t.axis } },
    axisTick: { show: false },
    axisLabel: { color: t.muted, fontSize: 11 },
    ...extra,
  };
}

export function valueAxis(t: ChartTheme) {
  return {
    type: "value",
    splitLine: { lineStyle: { color: t.grid } },
    axisLabel: { color: t.muted, fontSize: 11, formatter: (v: number) => (Number.isInteger(v) ? fmtInt(v) : String(v).replace(".", ",")) },
  };
}

/** Barras finas com topo arredondado (4px) ancoradas na base. */
export function barSeries(name: string, data: number[], color: string, extra: Record<string, unknown> = {}) {
  return {
    name,
    type: "bar",
    data,
    barMaxWidth: 22,
    itemStyle: { color, borderRadius: [4, 4, 0, 0] },
    emphasis: { focus: "series" },
    ...extra,
  };
}
