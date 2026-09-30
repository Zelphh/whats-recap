// Gráficos desenhados à mão em SVG/CSS (substituem o ECharts: menos JS e nada de canvas).
// Os caminhos são memorizados; o hover só move uma guia e troca um texto.

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { dayLabels, fmtDate, fmtDec, fmtInt, fmtMonth, isoToTs, monthLabels } from "../format";
import type { Stats } from "../types";
import { authorVar } from "../theme";
import { Dot, Seg } from "./ui";

const H = 260;
const MID = 130;
const AMP = 118;

/** Curva suave (Catmull-Rom → Bézier) pelos pontos. */
function smooth(pts: [number, number][]) {
  let d = `M${pts[0][0].toFixed(1)},${pts[0][1].toFixed(1)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] ?? pts[i], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] ?? p2;
    d += `C${(p1[0] + (p2[0] - p0[0]) / 6).toFixed(1)},${(p1[1] + (p2[1] - p0[1]) / 6).toFixed(1)} ` +
      `${(p2[0] - (p3[0] - p1[0]) / 6).toFixed(1)},${(p2[1] - (p3[1] - p1[1]) / 6).toFixed(1)} ${p2[0].toFixed(1)},${p2[1].toFixed(1)}`;
  }
  return d;
}

function seriesMax(values: number[][]) {
  let m = 1;
  for (const row of values) for (const v of row) if (v > m) m = v;
  return m;
}

const shortName = (name: string) => {
  const first = name.split(/\s+/)[0] ?? name;
  return first.length > 12 ? `${first.slice(0, 11)}…` : first;
};

interface Tick { at: number; label: string }

/** Marcas do eixo: viradas de ano quando houver pelo menos duas; senão, meses espaçados. */
function ticksFor(labels: string[], gran: "day" | "month"): Tick[] {
  const yearStart = gran === "day" ? "-01-01" : "-01";
  const years = labels.flatMap((l, i) => (l.endsWith(yearStart) && (gran === "day" || l.length === 7) ? [{ at: i, label: l.slice(0, 4) }] : []));
  if (years.length >= 2) return years;
  const months = labels.flatMap((l, i) => (gran === "month" || l.endsWith("-01") ? [{ at: i, label: fmtMonth(l.slice(0, 7)) }] : []));
  const step = Math.max(1, Math.ceil(months.length / 6));
  return months.filter((_, i) => i % step === 0);
}

/** "Rio": cada pessoa de um lado do eixo, na mesma escala. Mês (áreas) ou dia (traços, com zoom). */
export function TimelineCard({ stats }: { stats: Stats }) {
  const { authors } = stats;
  const [gran, setGran] = useState<"month" | "day">("month");
  const [zoom, setZoom] = useState(2);
  const [hover, setHover] = useState<number | null>(null);

  const series = gran === "day" ? stats.daily : stats.monthly;
  const n = series.values[0]?.length ?? 0;
  const labels = useMemo(
    () => (gran === "day" ? dayLabels(series.start, n) : monthLabels(series.start, n)),
    [gran, series.start, n],
  );
  const max = useMemo(() => seriesMax(series.values), [series]);
  const ticks = useMemo(() => ticksFor(labels, gran), [labels, gran]);

  const fmtLabel = (i: number) => (gran === "day" ? fmtDate(isoToTs(labels[i])) : fmtMonth(labels[i]));
  const info = hover !== null && hover < n
    ? `${fmtLabel(hover)} · ${authors.map((a, i) => `${shortName(a)} ${fmtInt(series.values[i][hover])}`).join(" · ")}`
    : gran === "day" ? "Passe o mouse sobre um dia · arraste para os lados" : "Passe o mouse para ver cada mês";

  return (
    <section className="card s-rio reveal">
      <div className="spread" style={{ alignItems: "flex-start" }}>
        <div>
          <h3 className="card-title">Mensagens por {gran === "day" ? "dia" : "mês"}</h3>
          <div className="chart-info" aria-live="polite">{info}</div>
        </div>
        <div className="row">
          {gran === "day" && (
            <div className="zoom">
              <button onClick={() => setZoom((z) => Math.max(1, z - 1))} aria-label="Diminuir zoom" disabled={zoom <= 1}>−</button>
              <span>{zoom}px/dia</span>
              <button onClick={() => setZoom((z) => Math.min(8, z + 1))} aria-label="Aumentar zoom" disabled={zoom >= 8}>+</button>
            </div>
          )}
          <Seg value={gran} onChange={(g) => { setHover(null); setGran(g); }} options={[["month", "Mês"], ["day", "Dia"]]} label="Granularidade" />
        </div>
      </div>
      <div className="river">
        <div className="river-axis" aria-hidden>
          <span>{fmtInt(max)}</span>
          <span><Dot i={0} />{shortName(authors[0] ?? "")}</span>
          <span>0</span>
          {authors[1] && <span><Dot i={1} />{shortName(authors[1])}</span>}
          {authors[1] && <span>{fmtInt(max)}</span>}
        </div>
        {gran === "month"
          ? <MonthRiver values={series.values} max={max} ticks={ticks} hover={hover} setHover={setHover} n={n} />
          : <DayRiver values={series.values} max={max} ticks={ticks} hover={hover} setHover={setHover} n={n} zoom={zoom} />}
      </div>
      {authors.length === 2 && (
        <p className="note">
          {authors[0]} para cima, {authors[1]} para baixo, na mesma escala.
          {gran === "day" && " Buracos são dias sem nenhuma mensagem."}
        </p>
      )}
    </section>
  );
}

interface RiverProps {
  values: number[][];
  max: number;
  ticks: Tick[];
  hover: number | null;
  setHover: (i: number | null) => void;
  n: number;
}

function MonthRiver({ values, max, ticks, hover, setHover, n }: RiverProps) {
  const areas = useMemo(() => values.map((row, a) => {
    const sign = a === 0 ? -1 : 1;
    const y = (v: number) => MID + (sign * v * AMP) / max;
    const pts: [number, number][] = n > 1 ? row.map((v, i) => [(i / (n - 1)) * 1000, y(v)]) : [[0, y(row[0] ?? 0)], [1000, y(row[0] ?? 0)]];
    return `M0,${MID} L${smooth(pts).slice(1)} L1000,${MID} Z`;
  }), [values, max, n]);
  const xOf = (i: number) => (n > 1 ? (i / (n - 1)) * 1000 : 500);

  return (
    <div className="river-plot">
      <svg viewBox={`0 0 1000 ${H}`} preserveAspectRatio="none" style={{ width: "100%", height: 280 }} role="img"
        aria-label="Mensagens por mês de cada pessoa"
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setHover(Math.max(0, Math.min(n - 1, Math.round(((e.clientX - r.left) / r.width) * (n - 1)))));
        }}
        onMouseLeave={() => setHover(null)}>
        <line x1="0" x2="1000" y1={MID} y2={MID} stroke="var(--grid)" vectorEffect="non-scaling-stroke" />
        <line x1="0" x2="1000" y1="10" y2="10" stroke="var(--grid)" strokeDasharray="3 5" vectorEffect="non-scaling-stroke" />
        <line x1="0" x2="1000" y1="250" y2="250" stroke="var(--grid)" strokeDasharray="3 5" vectorEffect="non-scaling-stroke" />
        <g className="grow-y" style={{ transformOrigin: "50% 50%", transformBox: "fill-box" }}>
          {areas.map((d, a) => <path key={a} d={d} fill={authorVar(a)} opacity={0.85} />)}
        </g>
        {hover !== null && (
          <line x1={xOf(hover)} x2={xOf(hover)} y1="0" y2={H} stroke="var(--ink)" strokeWidth="1.5" opacity="0.5" vectorEffect="non-scaling-stroke" />
        )}
      </svg>
      <div className="ticks" aria-hidden>
        {ticks.map((t) => (
          <span key={t.at} style={{ left: `${(xOf(t.at) / 1000) * 100}%`, transform: t.at === 0 ? undefined : "translateX(-50%)" }}>{t.label}</span>
        ))}
      </div>
    </div>
  );
}

function DayRiver({ values, max, ticks, hover, setHover, n, zoom }: RiverProps & { zoom: number }) {
  const box = useRef<HTMLDivElement>(null);
  const ratio = useRef(1); // posição relativa da rolagem, mantida ao trocar o zoom
  const drag = useRef<{ x: number; left: number; moved: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);
  const w = n * zoom;

  const paths = useMemo(() => values.map((row, a) => {
    const sign = a === 0 ? -1 : 1;
    let d = "";
    for (let i = 0; i < row.length; i++) {
      if (!row[i]) continue;
      d += `M${(i * zoom + zoom / 2).toFixed(1)} ${MID}V${(MID + (sign * row[i] * AMP) / max).toFixed(1)}`;
    }
    return d;
  }), [values, max, zoom]);

  useLayoutEffect(() => {
    const el = box.current;
    if (el) el.scrollLeft = ratio.current * (el.scrollWidth - el.clientWidth);
  }, [zoom]);

  return (
    <div
      ref={box}
      className={`day-scroll${dragging ? " dragging" : ""}`}
      onScroll={(e) => {
        const el = e.currentTarget;
        const span = el.scrollWidth - el.clientWidth;
        ratio.current = span > 0 ? el.scrollLeft / span : 1;
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        drag.current = { x: e.clientX, left: e.currentTarget.scrollLeft, moved: false };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const dx = e.clientX - d.x;
        if (!d.moved && Math.abs(dx) > 3) {
          d.moved = true;
          e.currentTarget.setPointerCapture(e.pointerId);
          setDragging(true);
        }
        if (d.moved) e.currentTarget.scrollLeft = d.left - dx;
      }}
      onPointerUp={() => { drag.current = null; setDragging(false); }}
      onPointerCancel={() => { drag.current = null; setDragging(false); }}
    >
      <svg width={w} height={H} viewBox={`0 0 ${w} ${H}`} style={{ display: "block" }} role="img"
        aria-label="Mensagens por dia de cada pessoa"
        onMouseMove={(e) => {
          if (drag.current?.moved) return;
          const r = e.currentTarget.getBoundingClientRect();
          setHover(Math.max(0, Math.min(n - 1, Math.floor((e.clientX - r.left) / zoom))));
        }}
        onMouseLeave={() => setHover(null)}>
        <line x1="0" x2={w} y1={MID} y2={MID} stroke="var(--grid)" />
        {ticks.map((t) => <line key={t.at} x1={t.at * zoom} x2={t.at * zoom} y1="0" y2={H} stroke="var(--grid)" strokeDasharray="3 4" />)}
        {paths.map((d, a) => <path key={a} d={d} stroke={authorVar(a)} strokeWidth={Math.max(1, zoom * 0.72)} fill="none" />)}
        {hover !== null && (
          <line x1={hover * zoom + zoom / 2} x2={hover * zoom + zoom / 2} y1="0" y2={H} stroke="var(--ink)" strokeWidth="1.2" opacity="0.5" />
        )}
      </svg>
      <div className="ticks" style={{ width: w }} aria-hidden>
        {ticks.map((t) => <span key={t.at} style={{ left: t.at * zoom, paddingLeft: 4 }}>{t.label}</span>)}
      </div>
    </div>
  );
}

/** Relógio de 24h: uma cunha por hora, comprimento proporcional às mensagens. */
export function HourRadial({ hourly, authors, split, peak }: {
  hourly: number[][]; authors: string[]; split: boolean; peak: number;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const totals = useMemo(() => hourly.reduce((acc, row) => acc.map((v, h) => v + row[h]), new Array(24).fill(0) as number[]), [hourly]);

  const wedges = useMemo(() => {
    const deg = Math.PI / 180;
    const P = (r: number, a: number) => `${(150 + r * Math.cos(a)).toFixed(2)},${(150 + r * Math.sin(a)).toFixed(2)}`;
    const wedge = (r1: number, a1: number, a2: number) =>
      `M${P(60, a1)}L${P(r1, a1)}A${r1},${r1} 0 0 1 ${P(r1, a2)}L${P(60, a2)}A60,60 0 0 0 ${P(60, a1)}Z`;
    const maxP = seriesMax(hourly);
    const maxT = Math.max(1, ...totals);
    const out: { h: number; d: string; fill: string }[] = [];
    for (let h = 0; h < 24; h++) {
      const c = h * 15 * deg - Math.PI / 2;
      if (split && hourly.length === 2) {
        out.push({ h, d: wedge(60 + (hourly[0][h] / maxP) * 78, c - 6.5 * deg, c - 0.5 * deg), fill: authorVar(0) });
        out.push({ h, d: wedge(60 + (hourly[1][h] / maxP) * 78, c + 0.5 * deg, c + 6.5 * deg), fill: authorVar(1) });
      } else {
        out.push({ h, d: wedge(60 + (totals[h] / maxT) * 78, c - 6 * deg, c + 6 * deg), fill: split ? authorVar(0) : "var(--accent)" });
      }
    }
    return out;
  }, [hourly, totals, split]);

  const tip = (h: number) => `${h}h · ${authors.map((a, i) => `${a} ${fmtInt(hourly[i][h])}`).join(" · ")}`;
  const h = hover ?? peak;
  const sub = hover === null
    ? "horário de pico"
    : split && authors.length === 2
      ? authors.map((a, i) => `${shortName(a)} ${fmtInt(hourly[i][h])}`).join(" · ")
      : `${fmtInt(totals[h])} msgs`;

  return (
    <div className="radial">
      <svg viewBox="0 0 300 300" role="img" aria-label={`Mensagens por hora do dia; pico às ${peak}h`} onMouseLeave={() => setHover(null)}>
        <circle cx="150" cy="150" r="56" fill="var(--surf2)" />
        <circle cx="150" cy="150" r="96" fill="none" stroke="var(--grid)" strokeDasharray="2 5" />
        <circle cx="150" cy="150" r="138" fill="none" stroke="var(--grid)" strokeDasharray="2 5" />
        <g className="wedges">
          {wedges.map((w, i) => (
            <path key={i} d={w.d} fill={w.fill} opacity={hover !== null && hover !== w.h ? 0.3 : 1} onMouseEnter={() => setHover(w.h)}>
              <title>{tip(w.h)}</title>
            </path>
          ))}
        </g>
        {[0, 6, 12, 18].map((hh) => {
          const a = hh * 15 * (Math.PI / 180) - Math.PI / 2;
          return (
            <text key={hh} x={150 + 146 * Math.cos(a)} y={150 + 146 * Math.sin(a)} textAnchor="middle" dominantBaseline="middle" fontSize="11" fill="var(--mute)">
              {hh}h
            </text>
          );
        })}
        <text x="150" y="143" textAnchor="middle" fontSize="24" fontWeight="800" fill="var(--ink)">{h}h</text>
        <text x="150" y="165" textAnchor="middle" fontSize="10.5" fill="var(--ink2)">{sub}</text>
      </svg>
    </div>
  );
}

/** Barras verticais por dia da semana, uma por pessoa. */
export function WeekdayBars({ data, authors, labels, hot, avg }: {
  data: number[][]; authors: string[]; labels: string[]; hot: number; avg: boolean;
}) {
  const max = seriesMax(data);
  const fmt = avg ? fmtDec : fmtInt;
  return (
    <div className="week-bars">
      <div className="week-cols" role="img" aria-label="Mensagens por dia da semana">
        {labels.map((l, d) => (
          <div key={l}>
            {data.map((row, a) => (
              <i key={a} className="grow-y" title={`${l} · ${authors[a]} ${fmt(row[d])}`}
                style={{ height: `${(row[d] / max) * 100}%`, background: authorVar(a) }} />
            ))}
          </div>
        ))}
      </div>
      <div className="week-labels" aria-hidden>
        {labels.map((l, d) => <span key={l} className={d === hot ? "hot" : ""}>{l}</span>)}
      </div>
    </div>
  );
}

/** Distribuição das respostas por faixa de tempo (% das respostas de cada pessoa). */
export function BucketBars({ labels, pct, authors }: { labels: string[]; pct: number[][]; authors: string[] }) {
  return (
    <div className="buckets" role="img" aria-label="Distribuição dos tempos de resposta por faixa">
      {labels.map((l, b) => (
        <div key={l}>
          <span className="blabel">{l}</span>
          <div className="bars">
            {pct.map((row, a) => (
              <div key={a} title={`${authors[a]}: ${fmtDec(row[b])}%`}>
                <i className="grow-x" style={{ width: `calc(${row[b].toFixed(1)}% * .8)`, background: authorVar(a) }} />
                {fmtDec(row[b])}%
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
