import { useEffect, useState } from "react";

/** Cores dos gráficos lidas dos tokens CSS, para acompanhar o modo claro/escuro. */
export interface ChartTheme {
  dark: boolean;
  series: [string, string];
  text: string;
  text2: string;
  muted: string;
  grid: string;
  axis: string;
  surface: string;
}

function read(): ChartTheme {
  const s = getComputedStyle(document.documentElement);
  const v = (name: string) => s.getPropertyValue(name).trim();
  return {
    dark: window.matchMedia("(prefers-color-scheme: dark)").matches,
    series: [v("--series-1"), v("--series-2")],
    text: v("--text"),
    text2: v("--text-2"),
    muted: v("--muted"),
    grid: v("--grid"),
    axis: v("--axis"),
    surface: v("--surface"),
  };
}

export function useChartTheme(): ChartTheme {
  const [theme, setTheme] = useState(read);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setTheme(read());
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return theme;
}

/** Cor do autor pelo índice (a cor segue a pessoa, nunca a posição no ranking). */
export const authorVar = (i: number) => `var(--series-${i + 1})`;
