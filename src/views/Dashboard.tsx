import { useMemo, useState } from "react";
import { BucketBars, HourRadial, TimelineCard, WeekdayBars } from "../components/charts";
import { CountUp, Dot, Legend, Seg, Who } from "../components/ui";
import { Sticker } from "../components/Sticker";
import {
  dayLabels, fmtDate, fmtDec, fmtDuration, fmtInt, fmtMonth, fmtPct, fmtTime, isoToTs, monthLabels,
} from "../format";
import { authorSoft, authorVar } from "../theme";
import type { Count, ImportSummary, MediaStats, Stats, Totals } from "../types";

const WEEKDAYS = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
const WEEKDAYS_LONG = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];
const WEEKDAYS_AT = ["às segundas", "às terças", "às quartas", "às quintas", "às sextas", "aos sábados", "aos domingos"];
const WAVE = [30, 55, 80, 45, 90, 60, 35, 70, 95, 50, 40, 75, 85, 55, 30, 65, 90, 45, 60, 80, 35, 55, 70, 40];

interface Props {
  id: string;
  stats: Stats;
  summary: ImportSummary;
  onOpenMessage: (id: number) => void;
}

const plural = (n: number, one: string, many: string) => `${fmtInt(n)} ${n === 1 ? one : many}`;
const initial = (name: string) => (name.trim()[0] ?? "?").toUpperCase();

function spanText(days: number) {
  if (days >= 548) return plural(Math.round(days / 365), "ano", "anos");
  if (days >= 60) return plural(Math.round(days / 30.4), "mês", "meses");
  return plural(Math.max(1, days), "dia", "dias");
}

function peakOf(hourly: number[][]) {
  const all = hourly.reduce((acc, row) => acc.map((v, h) => v + row[h]), new Array(24).fill(0) as number[]);
  return all.indexOf(Math.max(...all));
}

/** O parágrafo "Em resumo", montado a partir dos números. */
function Summary({ stats, peak }: { stats: Stats; peak: number }) {
  const { authors, perAuthor, totals, days, fastestResponder: fastest } = stats;
  const tenths = Math.round(days.activePct / 10);
  const person = (i: number) => (
    <span className="nw"><Dot i={i} />{authors[i]}</span>
  );

  let who: React.ReactNode = null;
  if (authors.length === 2) {
    const top = perAuthor[0].messages >= perAuthor[1].messages ? 0 : 1;
    const share = perAuthor[top].messages / Math.max(1, totals.messages);
    const talk = share < 0.52 ? null : share < 0.58 ? "fala um pouco mais" : "fala bem mais";
    if (!talk) {
      who = fastest !== null ? <> Os dois falam praticamente o mesmo, e {authors[fastest]} responde mais rápido.</> : <> Os dois falam praticamente o mesmo.</>;
    } else if (fastest === null || fastest === top) {
      who = <> {authors[top]} {talk}{fastest === top ? " e responde mais rápido" : ""}.</>;
    } else {
      who = <> {authors[top]} {talk}, mas {authors[fastest]} responde mais rápido.</>;
    }
  }

  return (
    <p>
      Em {spanText(days.spanDays)}, {person(0)}
      {authors[1] && <> e {person(1)}</>} trocaram <mark>{plural(totals.messages, "mensagem", "mensagens")}</mark> e conversaram{" "}
      {tenths >= 1 ? `em ${tenths} de cada 10 dias` : `em ${fmtPct(days.activePct)} dos dias`}.{who} As conversas se concentram{" "}
      {WEEKDAYS_AT[stats.weekday.mostActive]}, perto das {peak}h.
    </p>
  );
}

/** Fatos curtos derivados das estatísticas; o card troca de fato a cada clique. */
function curiosities(stats: Stats): string[] {
  const { authors, perAuthor, days } = stats;
  const out: string[] = [];

  const n = stats.daily.values[0]?.length ?? 0;
  if (n > 0) {
    let best = 0, bestV = -1;
    for (let i = 0; i < n; i++) {
      const v = stats.daily.values.reduce((s, row) => s + row[i], 0);
      if (v > bestV) { bestV = v; best = i; }
    }
    const date = dayLabels(stats.daily.start, best + 1)[best];
    out.push(`O dia mais movimentado foi ${fmtDate(isoToTs(date))}, com ${plural(bestV, "mensagem", "mensagens")}.`);
  }

  const m = stats.monthly.values[0]?.length ?? 0;
  if (m > 1) {
    let best = 0, bestV = -1;
    for (let i = 0; i < m; i++) {
      const v = stats.monthly.values.reduce((s, row) => s + row[i], 0);
      if (v > bestV) { bestV = v; best = i; }
    }
    out.push(`O mês mais animado foi ${fmtMonth(monthLabels(stats.monthly.start, best + 1)[best])}, com ${plural(bestV, "mensagem", "mensagens")}.`);
  }

  const silent = days.spanDays - days.activeDays;
  if (silent > 0) out.push(`Em ${plural(silent, "dia", "dias")} do período, ninguém mandou nada.`);

  if (authors.length === 2 && perAuthor[0].deleted + perAuthor[1].deleted > 0) {
    out.push(`${authors[0]} apagou ${plural(perAuthor[0].deleted, "mensagem", "mensagens")}; ${authors[1]}, ${fmtInt(perAuthor[1].deleted)}.`);
  }

  const audio = stats.media?.audio;
  if (audio && authors.length === 2 && audio[0].count + audio[1].count > 0 && audio[0].totalMs + audio[1].totalMs > 0) {
    const more = audio[0].count >= audio[1].count ? 0 : 1;
    const longer = audio[0].totalMs >= audio[1].totalMs ? 0 : 1;
    out.push(more === longer
      ? `${authors[more]} manda mais áudios e também fala por mais tempo neles.`
      : `${authors[more]} manda mais áudios, mas quem fala por mais tempo é ${authors[longer]}.`);
  }

  if (authors.length === 2) {
    const tops = stats.exclusiveWords.map((ex, i) => ({ i, w: ex.onlyYou[0] })).filter((x) => x.w);
    const top = tops.sort((a, b) => b.w.count - a.w.count)[0];
    if (top) out.push(`“${top.w.word}” aparece ${plural(top.w.count, "vez", "vezes")}, e todas são de ${authors[top.i]}.`);
  }

  if (stats.topEmojis[0]) out.push(`O emoji favorito da conversa é ${stats.topEmojis[0].item}, usado ${plural(stats.topEmojis[0].count, "vez", "vezes")}.`);
  return out;
}

function Curio({ facts }: { facts: string[] }) {
  const [k, setK] = useState(0);
  if (facts.length === 0) return null;
  return (
    <section className="s-curio wrap-flex reveal">
      <button className="curio" onClick={() => setK((x) => x + 1)} aria-label="Próxima curiosidade">
        <span className="top"><span>Curiosidade</span><span className="num" style={{ opacity: 0.7 }}>{(k % facts.length) + 1}/{facts.length}</span></span>
        <p aria-live="polite">{facts[k % facts.length]}</p>
        {facts.length > 1 && <span className="next">toque para a próxima →</span>}
      </button>
    </section>
  );
}

const authorTabs = (authors: string[]): [number, string][] => [[-1, "Geral"], ...authors.map((a, i) => [i, a] as [number, string])];

/** Top 5 com seletor Geral / pessoa. A cor segue a pessoa; o geral usa o verde do app. */
function TopCard({ title, all, byAuthor, authors, emoji, className }: {
  title: string; all: Count[]; byAuthor: Count[][]; authors: string[]; emoji?: boolean; className: string;
}) {
  const [who, setWho] = useState(-1);
  const items = (who < 0 ? all : byAuthor[who]).slice(0, 5);
  const color = who < 0 ? "var(--accent)" : authorVar(who);
  return (
    <section className={`${className} reveal`}>
      <div className="label">{title}</div>
      <Seg value={who} onChange={setWho} options={authorTabs(authors)} label={`${title}: de quem`} />
      {items.length === 0 ? <p className="empty-note">Nada por aqui.</p> : (
        <ol className={`rank${emoji ? " emoji" : ""}`}>
          {items.map((c, i) => (
            <li key={c.item}>
              <span className="pos">{i + 1}</span>
              {emoji ? (
                <>
                  <span className="emo">{c.item}</span>
                  <div className="track2 emoji-track"><div className="fill" style={{ width: `${(c.count / items[0].count) * 100}%`, background: color }} /></div>
                </>
              ) : (
                <div style={{ minWidth: 0 }}>
                  <div className="term">{c.item}</div>
                  <div className="track2"><div className="fill" style={{ width: `${(c.count / items[0].count) * 100}%`, background: color }} /></div>
                </div>
              )}
              <span className="count">{fmtInt(c.count)}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function StickerCard({ id, media, authors }: { id: string; media: MediaStats; authors: string[] }) {
  const [who, setWho] = useState(-1);
  const items = (who < 0 ? media.topStickers : media.topStickersByAuthor[who]).slice(0, 5);
  return (
    <section className="card r-b s-fig reveal">
      <div className="label">Figurinhas mais usadas</div>
      <Seg value={who} onChange={setWho} options={authorTabs(authors)} label="Figurinhas: de quem" />
      {items.length === 0 ? <p className="empty-note">Nenhuma figurinha.</p> : (
        <ol className="rank stk-rank">
          {items.map((s, i) => (
            <li key={s.file}>
              <span className="pos">{i + 1}</span>
              <span style={{ transform: `rotate(${[-4, 3, -2, 5, -3][i]}deg)`, display: "inline-flex" }}>
                <Sticker id={id} file={s.file} size={52} label={`Figurinha ${i + 1}`} />
              </span>
              <div style={{ minWidth: 0 }}>
                <div className="track2" style={{ marginTop: 0 }}>
                  <div className="fill" style={{ width: `${(s.count / items[0].count) * 100}%`, background: who < 0 ? "var(--accent)" : authorVar(who) }} />
                </div>
                <div className="note" style={{ marginTop: 4 }}>{s.variants > 1 ? `${s.variants} versões do arquivo agrupadas` : "arquivo único"}</div>
              </div>
              <span className="count">{fmtInt(s.count)}×</span>
            </li>
          ))}
        </ol>
      )}
      <p className="note">
        {plural(media.distinctStickers, "figurinha distinta", "figurinhas distintas")}
        {media.missingStickers > 0 && ` · ${fmtInt(media.missingStickers)} sem arquivo no .zip`}
      </p>
    </section>
  );
}

function AudioCard({ media, authors }: { media: MediaStats; authors: string[] }) {
  const max = Math.max(1, ...media.audio.map((a) => a.totalMs));
  const top = media.audio.reduce((best, a, i) => (a.totalMs > media.audio[best].totalMs ? i : best), 0);
  const missing = media.audio.reduce((n, a) => n + a.count - a.withDuration, 0);
  return (
    <section className="card r-d s-aud reveal">
      <div className="label">Áudios</div>
      {media.audio[top]?.totalMs > 0 && <div className="headline">{authors[top]} é quem mais fala em áudios</div>}
      {authors.map((a, i) => {
        const s = media.audio[i];
        const wave = i === 0 ? WAVE : [...WAVE].reverse();
        return (
          <div key={a} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            <div className="wave" style={{ width: `${Math.max(8, (s.totalMs / max) * 100)}%` }} aria-hidden>
              {wave.map((h, k) => <i key={k} className="grow-y" style={{ height: `${h}%`, background: authorVar(i) }} />)}
            </div>
            <div className="small ink2">
              <b style={{ color: "var(--ink)" }}>{a}</b> enviou {plural(s.count, "áudio", "áudios")}
              {s.withDuration > 0 && ` (${fmtDuration(s.totalMs / 1000)} no total, média de ${fmtDuration(s.avgMs / 1000)})`}
            </div>
          </div>
        );
      })}
      {missing > 0 && <p className="note">{fmtInt(missing)} áudio(s) sem arquivo no .zip ou ilegíveis: contam na quantidade, não na duração.</p>}
    </section>
  );
}

function OffCard({ title, text, className, children }: { title: string; text: string; className: string; children: React.ReactNode }) {
  return (
    <section className={`card off ${className} reveal`}>
      <div className="spread"><span className="label">{title}</span><span className="chip" style={{ fontSize: 12 }}>requer mídia</span></div>
      {children}
      <p className="small ink2">{text}</p>
    </section>
  );
}

function MediaCard({ stats }: { stats: Stats }) {
  const { authors, perAuthor, totals } = stats;
  const cols = `1.4fr ${authors.map(() => "1fr").join(" ")}`;
  const head = (
    <>
      <span className="h" />
      {authors.map((a, i) => <span key={a} className="h r"><Dot i={i} style={{ width: 8, height: 8 }} />{a}</span>)}
    </>
  );
  const rows: [string, (p: Totals) => number][] = totals.mediaHidden > 0
    ? [
        ["Mídias (total)", (p) => p.stickers + p.audios + p.images + p.videos + p.docs + p.mediaHidden],
        ["Apagadas", (p) => p.deleted],
      ]
    : [
        ["Figurinhas", (p) => p.stickers],
        ["Áudios", (p) => p.audios],
        ["Fotos e vídeos", (p) => p.images + p.videos],
        ["Documentos", (p) => p.docs],
        ["Apagadas", (p) => p.deleted],
      ];
  return (
    <section className="card s-midia reveal">
      <div className="label">Mídias enviadas</div>
      <div className="tbl" style={{ gridTemplateColumns: cols }}>
        {head}
        {rows.map(([label, get]) => (
          <Row key={label} label={label} values={perAuthor.map((p) => fmtInt(get(p)))} />
        ))}
      </div>
      {totals.mediaHidden > 0 && (
        <p className="note">Sem mídia, o export marca tudo como “mídia oculta”: não dá para separar áudio, foto e figurinha.</p>
      )}
    </section>
  );
}

function Row({ label, values }: { label: React.ReactNode; values: string[] }) {
  return (
    <>
      <span className="first">{label}</span>
      {values.map((v, i) => <span key={i}>{v}</span>)}
    </>
  );
}

export function Dashboard({ id, stats, summary, onOpenMessage }: Props) {
  const { authors, totals, perAuthor } = stats;
  const [weekMode, setWeekMode] = useState<"avg" | "total">("avg");
  const [hourSplit, setHourSplit] = useState(true);
  const [hourTable, setHourTable] = useState(false);
  const [copied, setCopied] = useState(false);

  const peak = useMemo(() => peakOf(stats.hourly), [stats]);
  const facts = useMemo(() => curiosities(stats), [stats]);
  const weekdayData = useMemo(() => {
    if (weekMode === "total") return stats.weekday.totals;
    // Média por ocorrência, por pessoa: total da pessoa naquele dia ÷ ocorrências do dia.
    return stats.weekday.totals.map((row) => row.map((v, d) => (stats.weekday.occurrences[d] ? v / stats.weekday.occurrences[d] : 0)));
  }, [stats, weekMode]);
  const bucketPct = useMemo(
    () => stats.responseTimes.map((r) => r.buckets.map((b) => (r.count ? (b * 100) / r.count : 0))),
    [stats],
  );

  const longest = stats.longestMessage;
  const mostMessages = perAuthor.length === 2 ? (perAuthor[0].messages >= perAuthor[1].messages ? 0 : 1) : 0;
  const fastest = stats.fastestResponder;
  const silent = stats.days.spanDays - stats.days.activeDays;
  const sharePct = perAuthor.map((p) => (p.messages * 100) / Math.max(1, totals.messages));

  async function copyChart() {
    await navigator.clipboard.writeText(stats.weekdayChartText);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  }

  return (
    <div className="page">
      <header className="dash-head reveal">
        <div className="dash-id">
          <div className="blobs" aria-hidden>
            {authors.map((a) => <span key={a} className="blob-shape">{initial(a)}</span>)}
          </div>
          <div style={{ minWidth: 0 }}>
            <h1 className="dash-title">
              {authors[0]}{authors[1] && <> <em>&amp;</em> {authors[1]}</>}
            </h1>
            <div className="row" style={{ marginTop: 12, gap: 8 }}>
              <span className="chip outline">{fmtDate(summary.firstTs)} a {fmtDate(summary.lastTs)}</span>
              <span className="chip outline">{summary.platform === "ios" ? "iOS" : "Android"}</span>
              <span className={`chip${summary.hasMedia ? " lime" : ""}`}>{summary.hasMedia ? "Com mídia" : "Sem mídia"}</span>
            </div>
          </div>
        </div>
        <Legend authors={authors} />
      </header>

      <div className="bento">
        <section className="resumo s-resumo reveal">
          <div className="over">Em resumo</div>
          <Summary stats={stats} peak={peak} />
          <div className="row" style={{ gap: 8 }}>
            <span className="chip">{plural(summary.sessionCount, "conversa (sessão)", "conversas (sessões)")}</span>
            {silent > 0 && <span className="chip">{plural(silent, "dia", "dias")} em silêncio</span>}
          </div>
        </section>

        <section className="nums s-nums">
          <div className="numcard reveal" style={{ "--i": 1 } as React.CSSProperties}>
            <span className="label">Mensagens</span>
            <span className="big"><CountUp value={totals.messages} format={fmtInt} /></span>
            <span className="sub">{plural(summary.sessionCount, "conversa (sessão)", "conversas (sessões)")}</span>
          </div>
          <div className="numcard reveal" style={{ "--i": 2 } as React.CSSProperties}>
            <span className="label">Palavras</span>
            <span className="big"><CountUp value={totals.words} format={fmtInt} /></span>
            <span className="sub">em {plural(totals.textMessages, "mensagem", "mensagens")} com texto</span>
          </div>
          <div className="numcard reveal" style={{ "--i": 3 } as React.CSSProperties}>
            <span className="label">Palavras por mensagem</span>
            <span className="big serif"><CountUp value={totals.avgWordsPerMessage} format={fmtDec} /></span>
            <span className="sub">média nas mensagens com texto</span>
          </div>
          <div className="numcard reveal" style={{ "--i": 4 } as React.CSSProperties}>
            <span className="label">Dias conversando</span>
            <span className="big"><CountUp value={stats.days.activeDays} format={fmtInt} /></span>
            <div className="meter"><div className="grow-x" style={{ width: `${stats.days.activePct}%` }} /></div>
            <span className="sub">de {plural(stats.days.spanDays, "dia", "dias")} no período ({fmtPct(stats.days.activePct)})</span>
          </div>
        </section>

        <TimelineCard stats={stats} />

        <section className="card r-c s-quem reveal">
          <div className="label">Quem manda mais mensagens</div>
          {authors.length === 2 && (
            <div className="headline">
              {authors[mostMessages]} enviou <span className="serif-i" style={{ fontSize: "1.3em" }}>{fmtPct(sharePct[mostMessages])}</span> das mensagens
            </div>
          )}
          <div className="share" aria-hidden>
            {perAuthor.map((p, i) => (
              <div key={i} className={`grow-x${i === 1 ? " from-right" : ""}`} style={{ flex: Math.max(p.messages, 1) }}>
                {i === 0 ? `${authors[0]} ${fmtPct(sharePct[0])}` : `${fmtPct(sharePct[1])} ${authors[1]}`}
              </div>
            ))}
          </div>
          <div className="tbl" style={{ gridTemplateColumns: "1.2fr 1fr 1fr 1fr" }}>
            <span className="h" /><span className="h">mensagens</span><span className="h">palavras</span><span className="h">pal./msg</span>
            {authors.map((a, i) => (
              <Row key={a} label={<Who name={a} i={i} />}
                values={[fmtInt(perAuthor[i].messages), fmtInt(perAuthor[i].words), fmtDec(perAuthor[i].avgWordsPerMessage)]} />
            ))}
          </div>
        </section>

        <section className="s-longa wrap-flex reveal">
          <div className={`longa${longest?.author === 1 ? " by-p2" : ""}`}>
            <div className="label">Mensagem mais longa</div>
            {longest ? (
              <>
                <blockquote>“{longest.preview}{longest.preview.length < longest.chars ? "…" : ""}”</blockquote>
                <div className="row small ink2">
                  <b style={{ color: "var(--ink)" }}><Who name={authors[longest.author]} i={longest.author} /></b>
                  {fmtDate(longest.ts)} às {fmtTime(longest.ts)}
                </div>
                <div className="spread">
                  <span className="tiny ink2 num">{plural(longest.words, "palavra", "palavras")} · {plural(longest.chars, "caractere", "caracteres")}</span>
                  <button className="btn ink sm" onClick={() => onOpenMessage(longest.id)}>Abrir na conversa →</button>
                </div>
              </>
            ) : <p className="empty-note">Nenhuma mensagem de texto.</p>}
          </div>
        </section>

        <Curio facts={facts} />

        <section className="card s-hora reveal">
          <div className="spread">
            <div>
              <div className="label">Horário mais ativo</div>
              <div className="headline">Pico às {peak}h</div>
            </div>
            <Seg value={hourSplit ? "split" : "all"} onChange={(v) => setHourSplit(v === "split")}
              options={[["split", "Por pessoa"], ["all", "Juntos"]]} label="Horário: agrupamento" />
          </div>
          {hourTable ? (
            <div className="hour-grid">
              {stats.hourly[0].map((_, h) => (
                <div key={h}>
                  <b>{h}h</b>
                  {hourSplit
                    ? authors.map((a, i) => <span key={a}>{a.split(/\s+/)[0]} {fmtInt(stats.hourly[i][h])}</span>)
                    : <span>{fmtInt(stats.hourly.reduce((s, row) => s + row[h], 0))}</span>}
                </div>
              ))}
            </div>
          ) : (
            <HourRadial hourly={stats.hourly} authors={authors} split={hourSplit} peak={peak} />
          )}
          <div className="spread">
            {hourSplit ? <Legend authors={authors} /> : <span />}
            <button className="link" onClick={() => setHourTable((t) => !t)}>{hourTable ? "Ver como gráfico" : "Ver como tabela"}</button>
          </div>
        </section>

        <section className="card r-d s-semana reveal">
          <div className="spread" style={{ alignItems: "flex-start" }}>
            <div>
              <div className="label">Dia da semana</div>
              <div style={{ marginTop: 6, lineHeight: 1.4, maxWidth: 460 }}>
                Mais ativo: <b>{WEEKDAYS_LONG[stats.weekday.mostActive]}</b> ({fmtDec(stats.weekday.avgPerOccurrence[stats.weekday.mostActive])} msgs em média) ·
                menos ativo: <b>{WEEKDAYS_LONG[stats.weekday.leastActive]}</b> ({fmtDec(stats.weekday.avgPerOccurrence[stats.weekday.leastActive])})
              </div>
            </div>
            <Seg value={weekMode} onChange={setWeekMode} options={[["avg", "Média por dia"], ["total", "Total"]]} label="Dia da semana: medida" />
          </div>
          <div className="week">
            <WeekdayBars data={weekdayData} authors={authors} labels={WEEKDAYS} hot={stats.weekday.mostActive} avg={weekMode === "avg"} />
            <div className="share-text">
              <div className="spread">
                <span className="tiny" style={{ fontWeight: 800, opacity: 0.8 }}>Para compartilhar</span>
                <button className="btn lime sm" onClick={copyChart}>{copied ? "Copiado!" : "Copiar"}</button>
              </div>
              <pre>{stats.weekdayChartText}</pre>
            </div>
          </div>
        </section>

        <section className="card r-b s-resp reveal">
          <div className="label">Tempo de resposta</div>
          {fastest !== null && (
            <div className="headline">
              <Dot i={fastest} style={{ width: 12, height: 12, marginRight: 8 }} />
              {authors[fastest]} responde mais rápido{" "}
              <span className="quiet">(mediana de {fmtDuration(stats.responseTimes[fastest].medianSecs)})</span>
            </div>
          )}
          <div className="resp-pair">
            {authors.map((a, i) => (
              <div key={a} style={{ background: authorSoft(i) }}>
                <b className="name">{a}</b>
                <span>mediana <b className="med">{fmtDuration(stats.responseTimes[i].medianSecs)}</b></span>
                <span>média {fmtDuration(stats.responseTimes[i].meanSecs)}</span>
                <span className="note">{plural(stats.responseTimes[i].count, "resposta", "respostas")}</span>
              </div>
            ))}
          </div>
          <BucketBars
            labels={["< 1 min", "1–5 min", "5–30 min", `30 min–${fmtDuration(summary.sessionGapSecs)}`]}
            pct={bucketPct}
            authors={authors}
          />
          <p className="note">
            Quando a média fica muito acima da mediana, é sinal de algumas respostas bem demoradas, não de lentidão no dia a dia.
            Intervalos maiores que {fmtDuration(summary.sessionGapSecs)} contam como nova conversa e são descartados.
          </p>
        </section>

        <TopCard className="card r-a s-words" title="Palavras mais usadas" all={stats.topWords} byAuthor={stats.topWordsByAuthor} authors={authors} />
        <TopCard className="card soft r-c s-emo" title="Emojis mais usados" all={stats.topEmojis} byAuthor={stats.topEmojisByAuthor} authors={authors} emoji />

        {authors.length === 2 && authors.map((a, i) => {
          const other = authors[1 - i];
          const ex = stats.exclusiveWords[i];
          return (
            <section key={a} className={`card ${i === 0 ? "r-a" : "r-b"} s-ex reveal`}>
              <div className="ex-title"><Dot i={i} /><span style={{ overflowWrap: "anywhere" }}>Palavras de {a}</span></div>
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                <span className="label">Só {a} usa</span>
                <div className="word-chips">
                  {ex.onlyYou.slice(0, 8).map((w) => (
                    <span key={w.word} title={`${a}: ${fmtInt(w.count)}× · ${other}: 0×`}
                      style={{ background: authorSoft(i), fontSize: `${(13 + Math.log10(Math.max(1, w.count)) * 3.2).toFixed(1)}px` }}>
                      {w.word}<small>{fmtInt(w.count)}×</small>
                    </span>
                  ))}
                  {ex.onlyYou.length === 0 && <span className="empty-note">Nenhuma</span>}
                </div>
              </div>
              <div className="more-list" style={{ display: "flex", flexDirection: "column", gap: 0 }}>
                <span className="label" style={{ marginBottom: 8 }}>{a} usa muito mais</span>
                {ex.muchMore.slice(0, 8).map((w) => (
                  <div key={w.word} title={`${a}: ${fmtInt(w.count)}× · ${other}: ${fmtInt(w.otherCount)}×`}>
                    <b>{w.word}</b><span>{fmtDec(w.ratio ?? 0)}× mais</span>
                  </div>
                ))}
                {ex.muchMore.length === 0 && <span className="empty-note">Nenhuma</span>}
              </div>
            </section>
          );
        })}

        <MediaCard stats={stats} />
        {stats.media ? (
          <>
            <StickerCard id={id} media={stats.media} authors={authors} />
            <AudioCard media={stats.media} authors={authors} />
          </>
        ) : (
          <>
            <OffCard className="r-b s-fig" title="Figurinhas mais usadas" text="Exporte a conversa com mídia (.zip) para ver as figurinhas mais usadas.">
              <div className="ghosts" aria-hidden>{[0, 1, 2, 3].map((k) => <i key={k} />)}</div>
            </OffCard>
            <OffCard className="r-d s-aud" title="Áudios" text="Exporte a conversa com mídia (.zip) para ver quem manda mais áudios e por quanto tempo.">
              <div className="wave ghost" aria-hidden>{WAVE.slice(0, 18).map((h, k) => <i key={k} style={{ height: `${h}%` }} />)}</div>
            </OffCard>
          </>
        )}
      </div>
    </div>
  );
}
