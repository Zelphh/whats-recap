import { useMemo, useState } from "react";
import { baseOption, barSeries, categoryAxis, Chart, type ChartOption, valueAxis } from "../components/Chart";
import {
  dayLabels, fmtDate, fmtDateTime, fmtDec, fmtDuration, fmtInt, fmtMonth, fmtPct, monthLabels,
} from "../format";
import { authorVar, type ChartTheme, useChartTheme } from "../theme";
import { Sticker } from "../components/Sticker";
import type { Count, ImportSummary, MediaStats, Stats } from "../types";

const WEEKDAYS = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
const WEEKDAYS_LONG = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];
const HOURS = Array.from({ length: 24 }, (_, h) => `${String(h).padStart(2, "0")}h`);

interface Props {
  id: string;
  stats: Stats;
  summary: ImportSummary;
  onOpenMessage: (id: number) => void;
}

function Seg<T extends string>({ value, options, onChange }: {
  value: T; options: [T, string][]; onChange: (v: T) => void;
}) {
  return (
    <div className="seg" role="group">
      {options.map(([v, label]) => (
        <button key={v} className={v === value ? "on" : ""} aria-pressed={v === value} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function Legend({ authors }: { authors: string[] }) {
  return (
    <div className="legend">
      {authors.map((a, i) => (
        <span key={a}><span className="swatch" style={{ background: authorVar(i) }} />{a}</span>
      ))}
    </div>
  );
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="card">
      <h3>{label}</h3>
      <div className="stat-value">{value}</div>
      {note && <div className="stat-note">{note}</div>}
    </div>
  );
}

function RankList({ items, color, big }: { items: Count[]; color: string; big?: boolean }) {
  if (items.length === 0) return <p className="hint">Nada por aqui.</p>;
  const max = items[0].count;
  return (
    <ol className="rank">
      {items.map((c, i) => (
        <li key={c.item}>
          <span className="pos">{i + 1}</span>
          <div style={{ minWidth: 0 }}>
            <div className={`label${big ? " emoji-big" : ""}`}>{c.item}</div>
            <div className="bar" style={{ width: `${(c.count / max) * 100}%`, background: color }} />
          </div>
          <span className="num">{fmtInt(c.count)}</span>
        </li>
      ))}
    </ol>
  );
}

/** Top 5 com seletor Geral / pessoa. A cor segue a pessoa; o geral usa tinta neutra. */
function TopCard({ title, all, byAuthor, authors, big }: {
  title: string; all: Count[]; byAuthor: Count[][]; authors: string[]; big?: boolean;
}) {
  const [who, setWho] = useState("all");
  const idx = who === "all" ? -1 : Number(who);
  return (
    <div className="card">
      <div className="card-head">
        <h2>{title}</h2>
        <Seg value={who} onChange={setWho} options={[["all", "Geral"], ...authors.map((a, i) => [String(i), a] as [string, string])]} />
      </div>
      <RankList items={(idx < 0 ? all : byAuthor[idx]).slice(0, 5)} color={idx < 0 ? "var(--muted)" : authorVar(idx)} big={big} />
    </div>
  );
}

function StickerCard({ id, media, authors }: { id: string; media: MediaStats; authors: string[] }) {
  const [who, setWho] = useState("all");
  const idx = who === "all" ? -1 : Number(who);
  const items = (idx < 0 ? media.topStickers : media.topStickersByAuthor[idx]).slice(0, 5);
  return (
    <div className="card">
      <div className="card-head">
        <h2>Figurinhas mais usadas</h2>
        <Seg value={who} onChange={setWho} options={[["all", "Geral"], ...authors.map((a, i) => [String(i), a] as [string, string])]} />
      </div>
      {items.length === 0 ? (
        <p className="hint">Nenhuma figurinha.</p>
      ) : (
        <ol className="sticker-rank">
          {items.map((s, i) => (
            <li key={s.file}>
              <span className="pos">{i + 1}</span>
              <Sticker id={id} file={s.file} label={`Figurinha ${i + 1}`} />
              <div style={{ minWidth: 0 }}>
                <div className="bar" style={{ width: `${(s.count / items[0].count) * 100}%`, background: idx < 0 ? "var(--muted)" : authorVar(idx) }} />
                {s.variants > 1 && <div className="hint">{s.variants} versões do arquivo agrupadas</div>}
              </div>
              <span className="num">{fmtInt(s.count)}</span>
            </li>
          ))}
        </ol>
      )}
      <p className="hint" style={{ marginTop: 10 }}>
        {fmtInt(media.distinctStickers)} figurinhas distintas
        {media.missingStickers > 0 && ` · ${fmtInt(media.missingStickers)} sem arquivo no .zip`}
      </p>
    </div>
  );
}

function AudioCard({ media, authors }: { media: MediaStats; authors: string[] }) {
  const max = Math.max(1, ...media.audio.map((a) => a.totalMs));
  const top = media.audio.reduce((best, a, i) => (a.totalMs > media.audio[best].totalMs ? i : best), 0);
  const missing = media.audio.reduce((n, a) => n + a.count - a.withDuration, 0);
  return (
    <div className="card">
      <div className="card-head"><h2>Áudios</h2></div>
      {media.audio[top]?.totalMs > 0 && (
        <p><strong>{authors[top]}</strong> é quem mais fala em áudios.</p>
      )}
      <div className="audio-grid">
        {authors.map((a, i) => {
          const s = media.audio[i];
          return (
            <div key={a} className="audio-row">
              <div>
                <span className="swatch" style={{ background: authorVar(i) }} />
                <strong>{a}</strong> enviou {fmtInt(s.count)} áudio{s.count === 1 ? "" : "s"}
                {s.withDuration > 0 && ` (${fmtDuration(s.totalMs / 1000)} no total, média de ${fmtDuration(s.avgMs / 1000)})`}
              </div>
              <div className="bar" style={{ width: `${(s.totalMs / max) * 100}%`, background: authorVar(i) }} />
            </div>
          );
        })}
      </div>
      {missing > 0 && <p className="hint">{fmtInt(missing)} áudio(s) sem arquivo no .zip ou ilegíveis: contam na quantidade, não na duração.</p>}
    </div>
  );
}

function timelineOption(t: ChartTheme, stats: Stats, gran: "day" | "month"): ChartOption {
  const series = gran === "day" ? stats.daily : stats.monthly;
  const n = series.values[0]?.length ?? 0;
  const labels = gran === "day" ? dayLabels(series.start, n) : monthLabels(series.start, n);
  const fmtLabel = (v: string) => (gran === "day" ? fmtDate(Date.parse(`${v}T00:00:00Z`) / 1000) : fmtMonth(v));
  return {
    ...baseOption(t),
    grid: { left: 8, right: 12, top: 16, bottom: gran === "day" ? 44 : 8, containLabel: true },
    tooltip: {
      ...(baseOption(t).tooltip as object),
      trigger: "axis",
      axisPointer: { type: "line", lineStyle: { color: t.axis } },
      valueFormatter: (v: number) => fmtInt(v),
    },
    xAxis: categoryAxis(t, labels, { axisLabel: { color: t.muted, fontSize: 11, formatter: fmtLabel }, boundaryGap: gran === "month" }),
    yAxis: valueAxis(t),
    dataZoom: gran === "day" && n > 120
      ? [
          { type: "inside", start: Math.max(0, 100 - (365 / n) * 100), end: 100 },
          { type: "slider", height: 18, bottom: 8, borderColor: t.axis, textStyle: { color: t.muted }, labelFormatter: (_: number, v: string) => fmtLabel(v) },
        ]
      : [],
    series: stats.authors.map((a, i) =>
      gran === "month"
        ? barSeries(a, series.values[i], t.series[i], { barGap: "8%" })
        : { name: a, type: "line", data: series.values[i], showSymbol: false, lineStyle: { width: 2, color: t.series[i] }, itemStyle: { color: t.series[i] } },
    ),
  };
}

function groupedBars(t: ChartTheme, labels: string[], authors: string[], data: number[][], fmt: (v: number) => string): ChartOption {
  return {
    ...baseOption(t),
    tooltip: { ...(baseOption(t).tooltip as object), trigger: "axis", axisPointer: { type: "shadow", shadowStyle: { color: t.grid, opacity: 0.4 } }, valueFormatter: fmt },
    xAxis: categoryAxis(t, labels),
    yAxis: valueAxis(t),
    series: data.map((d, i) => barSeries(authors[i] ?? "Total", d, authors[i] ? t.series[i] : t.muted, { barGap: "8%" })),
  };
}

export function Dashboard({ id, stats, summary, onOpenMessage }: Props) {
  const t = useChartTheme();
  const { authors, totals, perAuthor } = stats;
  const [gran, setGran] = useState<"day" | "month">("month");
  const [weekMode, setWeekMode] = useState<"avg" | "total">("avg");
  const [hourMode, setHourMode] = useState<"split" | "all">("split");
  const [copied, setCopied] = useState(false);

  const timeline = useMemo(() => timelineOption(t, stats, gran), [t, stats, gran]);

  const weekdayData = useMemo(() => {
    if (weekMode === "total") return stats.weekday.totals;
    // Média por ocorrência, por pessoa: total da pessoa naquele dia ÷ ocorrências do dia.
    return stats.weekday.totals.map((row) => row.map((v, d) => (stats.weekday.occurrences[d] ? v / stats.weekday.occurrences[d] : 0)));
  }, [stats, weekMode]);
  const weekdayOpt = useMemo(
    () => groupedBars(t, WEEKDAYS, authors, weekdayData, weekMode === "avg" ? fmtDec : fmtInt),
    [t, authors, weekdayData, weekMode],
  );

  const hourData = useMemo(
    () => (hourMode === "split" ? stats.hourly : [stats.hourly.reduce((acc, row) => acc.map((v, h) => v + row[h]), new Array(24).fill(0))]),
    [stats, hourMode],
  );
  const hourOpt = useMemo(
    () => groupedBars(t, HOURS, hourMode === "split" ? authors : [], hourData, fmtInt),
    [t, authors, hourData, hourMode],
  );
  const peakHour = useMemo(() => {
    const all = stats.hourly.reduce((acc, row) => acc.map((v, h) => v + row[h]), new Array(24).fill(0));
    return all.indexOf(Math.max(...all));
  }, [stats]);

  const bucketOpt = useMemo(
    () => groupedBars(t, ["< 1 min", "1–5 min", "5–30 min", `30 min–${fmtDuration(summary.sessionGapSecs)}`], authors,
      stats.responseTimes.map((r) => r.buckets.map((b) => (r.count ? (b * 100) / r.count : 0))), (v) => fmtPct(v)),
    [t, authors, stats, summary.sessionGapSecs],
  );

  const longest = stats.longestMessage;
  const mostMessages = perAuthor.length === 2 ? (perAuthor[0].messages >= perAuthor[1].messages ? 0 : 1) : 0;
  const fastest = stats.fastestResponder;

  async function copyChart() {
    await navigator.clipboard.writeText(stats.weekdayChartText);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>{authors.join(" & ")}</h1>
          <p className="subtitle">
            {stats.days.firstDate && `${fmtDate(summary.firstTs)} a ${fmtDate(summary.lastTs)}`} ·{" "}
            {summary.platform === "ios" ? "iOS" : "Android"}
          </p>
        </div>
        <span className={`badge${summary.hasMedia ? " accent" : ""}`}>{summary.hasMedia ? "Com mídia" : "Sem mídia"}</span>
      </div>

      <div className="grid cols-4">
        <Stat label="Mensagens" value={fmtInt(totals.messages)} note={`${fmtInt(summary.sessionCount)} conversas (sessões)`} />
        <Stat label="Palavras" value={fmtInt(totals.words)} />
        <Stat label="Palavras por mensagem" value={fmtDec(totals.avgWordsPerMessage)} note="média nas mensagens com texto" />
        <Stat
          label="Dias conversando"
          value={fmtInt(stats.days.activeDays)}
          note={`de ${fmtInt(stats.days.spanDays)} dias no período (${fmtPct(stats.days.activePct)})`}
        />
      </div>

      <div className="grid cols-2">
        <div className="card">
          <div className="card-head"><h2>Quem manda mais mensagens</h2></div>
          {authors.length === 2 && (
            <p>
              <strong>{authors[mostMessages]}</strong> enviou {fmtPct((perAuthor[mostMessages].messages * 100) / Math.max(1, totals.messages))} das mensagens.
            </p>
          )}
          <div className="share-bar" aria-hidden>
            {perAuthor.map((p, i) => (
              <div key={i} style={{ flex: p.messages, background: authorVar(i) }} />
            ))}
          </div>
          <table className="table">
            <thead>
              <tr><th /><th className="r">Mensagens</th><th className="r">Palavras</th><th className="r">Palavras/msg</th></tr>
            </thead>
            <tbody>
              {authors.map((a, i) => (
                <tr key={a}>
                  <td><span className="swatch" style={{ background: authorVar(i) }} />{a}</td>
                  <td className="r">{fmtInt(perAuthor[i].messages)}</td>
                  <td className="r">{fmtInt(perAuthor[i].words)}</td>
                  <td className="r">{fmtDec(perAuthor[i].avgWordsPerMessage)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="card">
          <div className="card-head"><h2>Mensagem mais longa</h2></div>
          {longest ? (
            <>
              <p className="hint">
                {authors[longest.author]} · {fmtDateTime(longest.ts)} · {fmtInt(longest.words)} palavras · {fmtInt(longest.chars)} caracteres
              </p>
              <p style={{ whiteSpace: "pre-wrap", maxHeight: 130, overflow: "hidden", margin: "8px 0" }}>
                {longest.preview}{longest.preview.length < longest.chars ? "…" : ""}
              </p>
              <button className="link" onClick={() => onOpenMessage(longest.id)}>Abrir na conversa</button>
            </>
          ) : <p className="hint">Nenhuma mensagem de texto.</p>}
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>Mensagens por {gran === "day" ? "dia" : "mês"}</h2>
          <div className="row">
            <Legend authors={authors} />
            <Seg value={gran} onChange={setGran} options={[["month", "Mês"], ["day", "Dia"]]} />
          </div>
        </div>
        <Chart option={timeline} height={280} ariaLabel={`Mensagens por ${gran === "day" ? "dia" : "mês"} de cada pessoa`} />
      </div>

      <div className="grid cols-2">
        <div className="card">
          <div className="card-head">
            <h2>Dia da semana</h2>
            <Seg value={weekMode} onChange={setWeekMode} options={[["avg", "Média por dia"], ["total", "Total"]]} />
          </div>
          <p className="hint">
            Mais ativo: <strong>{WEEKDAYS_LONG[stats.weekday.mostActive]}</strong> ({fmtDec(stats.weekday.avgPerOccurrence[stats.weekday.mostActive])} msgs em média) ·
            menos ativo: <strong>{WEEKDAYS_LONG[stats.weekday.leastActive]}</strong> ({fmtDec(stats.weekday.avgPerOccurrence[stats.weekday.leastActive])})
          </p>
          <Chart option={weekdayOpt} height={220} ariaLabel="Mensagens por dia da semana" />
          <div className="card-head" style={{ marginTop: 12 }}>
            <h3>Em texto, para compartilhar</h3>
            <button className="btn small" onClick={copyChart}>{copied ? "Copiado!" : "Copiar"}</button>
          </div>
          <pre className="text-chart">{stats.weekdayChartText}</pre>
        </div>

        <div className="card">
          <div className="card-head">
            <h2>Horário mais ativo</h2>
            <Seg value={hourMode} onChange={setHourMode} options={[["split", "Por pessoa"], ["all", "Juntos"]]} />
          </div>
          <p className="hint">Pico às <strong>{HOURS[peakHour]}</strong></p>
          <Chart option={hourOpt} height={220} ariaLabel="Mensagens por hora do dia" />
          <details style={{ marginTop: 8 }}>
            <summary className="hint">Ver como tabela</summary>
            <table className="table">
              <thead><tr><th>Hora</th>{authors.map((a) => <th key={a} className="r">{a}</th>)}</tr></thead>
              <tbody>
                {HOURS.map((h, i) => (
                  <tr key={h}><td>{h}</td>{stats.hourly.map((row, a) => <td key={a} className="r">{fmtInt(row[i])}</td>)}</tr>
                ))}
              </tbody>
            </table>
          </details>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>Tempo de resposta</h2>
          <Legend authors={authors} />
        </div>
        {fastest !== null && (
          <p>
            <strong>{authors[fastest]}</strong> responde mais rápido (mediana de {fmtDuration(stats.responseTimes[fastest].medianSecs)}).
          </p>
        )}
        <div className="grid cols-2">
          <table className="table">
            <thead>
              <tr><th /><th className="r">Mediana</th><th className="r">Média</th><th className="r">Respostas</th></tr>
            </thead>
            <tbody>
              {authors.map((a, i) => (
                <tr key={a}>
                  <td><span className="swatch" style={{ background: authorVar(i) }} />{a}</td>
                  <td className="r">{fmtDuration(stats.responseTimes[i].medianSecs)}</td>
                  <td className="r">{fmtDuration(stats.responseTimes[i].meanSecs)}</td>
                  <td className="r">{fmtInt(stats.responseTimes[i].count)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <Chart option={bucketOpt} height={200} ariaLabel="Distribuição dos tempos de resposta por faixa" />
        </div>
        <p className="hint">
          Média bem acima da mediana indica muitas respostas lentas pontuais. Intervalos maiores que{" "}
          {fmtDuration(summary.sessionGapSecs)} contam como nova conversa e são descartados.
        </p>
      </div>

      <div className="grid cols-2">
        <TopCard title="Palavras mais usadas" all={stats.topWords} byAuthor={stats.topWordsByAuthor} authors={authors} />
        <TopCard title="Emojis mais usados" all={stats.topEmojis} byAuthor={stats.topEmojisByAuthor} authors={authors} big />
      </div>

      {authors.length === 2 && (
        <div className="grid cols-2">
          {authors.map((a, i) => {
            const other = authors[1 - i];
            const ex = stats.exclusiveWords[i];
            return (
              <div className="card" key={a}>
                <div className="card-head">
                  <h2><span className="swatch" style={{ background: authorVar(i) }} />Palavras de {a}</h2>
                </div>
                <div className="grid cols-2">
                  <div>
                    <h3>Só {a} usa</h3>
                    <ul className="rank" style={{ marginTop: 8 }}>
                      {ex.onlyYou.slice(0, 8).map((w) => (
                        <li key={w.word} style={{ gridTemplateColumns: "1fr auto" }}>
                          <span className="label">{w.word}</span><span className="num">{fmtInt(w.count)}×</span>
                        </li>
                      ))}
                      {ex.onlyYou.length === 0 && <li className="hint">Nenhuma</li>}
                    </ul>
                  </div>
                  <div>
                    <h3>{a} usa muito mais</h3>
                    <ul className="rank" style={{ marginTop: 8 }}>
                      {ex.muchMore.slice(0, 8).map((w) => (
                        <li key={w.word} style={{ gridTemplateColumns: "1fr auto" }} title={`${a}: ${w.count} · ${other}: ${w.otherCount}`}>
                          <span className="label">{w.word}</span><span className="num">{fmtDec(w.ratio ?? 0)}× mais</span>
                        </li>
                      ))}
                      {ex.muchMore.length === 0 && <li className="hint">Nenhuma</li>}
                    </ul>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="grid cols-3">
        <div className="card">
          <div className="card-head"><h2>Mídias enviadas</h2></div>
          {totals.mediaHidden > 0 ? (
            <table className="table">
              <thead><tr><th /><th className="r">Mídias</th><th className="r">Apagadas</th></tr></thead>
              <tbody>
                {authors.map((a, i) => {
                  const p = perAuthor[i];
                  const media = p.stickers + p.audios + p.images + p.videos + p.docs + p.mediaHidden;
                  return (
                    <tr key={a}>
                      <td><span className="swatch" style={{ background: authorVar(i) }} />{a}</td>
                      <td className="r">{fmtInt(media)}</td>
                      <td className="r">{fmtInt(p.deleted)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            // O export identifica o tipo de cada mídia (com mídia, ou iOS sem mídia).
            <table className="table">
              <thead>
                <tr>
                  <th />
                  {authors.map((a, i) => (
                    <th key={a} className="r"><span className="swatch" style={{ background: authorVar(i) }} />{a}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {([
                  ["Figurinhas", (p) => p.stickers],
                  ["Áudios", (p) => p.audios],
                  ["Fotos e vídeos", (p) => p.images + p.videos],
                  ["Documentos", (p) => p.docs],
                  ["Apagadas", (p) => p.deleted],
                ] as [string, (p: Stats["totals"]) => number][]).map(([label, get]) => (
                  <tr key={label}>
                    <td>{label}</td>
                    {perAuthor.map((p, i) => <td key={i} className="r">{fmtInt(get(p))}</td>)}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {totals.mediaHidden > 0 && <p className="hint">Sem mídia no export, não dá para separar áudio, foto e figurinha.</p>}
        </div>
        {stats.media ? (
          <>
            <StickerCard id={id} media={stats.media} authors={authors} />
            <AudioCard media={stats.media} authors={authors} />
          </>
        ) : (
          <>
            <div className="card disabled">
              <div className="card-head"><h2>Figurinhas mais usadas</h2><span className="badge">requer mídia</span></div>
              <p className="hint">Exporte a conversa com mídia (.zip) para ver o ranking de figurinhas.</p>
            </div>
            <div className="card disabled">
              <div className="card-head"><h2>Áudios</h2><span className="badge">requer mídia</span></div>
              <p className="hint">Exporte a conversa com mídia (.zip) para ver quem mais fala em áudios.</p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
