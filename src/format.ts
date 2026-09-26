// Os timestamps do backend são o horário local do export codificado como UTC
// (sem fuso), então toda formatação usa timeZone: "UTC" para exibi-los como estão.

const intFmt = new Intl.NumberFormat("pt-BR");
const decFmt = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });
const dateFmt = new Intl.DateTimeFormat("pt-BR", { timeZone: "UTC", day: "2-digit", month: "2-digit", year: "numeric" });
const timeFmt = new Intl.DateTimeFormat("pt-BR", { timeZone: "UTC", hour: "2-digit", minute: "2-digit" });
const longDateFmt = new Intl.DateTimeFormat("pt-BR", {
  timeZone: "UTC", weekday: "long", day: "numeric", month: "long", year: "numeric",
});
const monthFmt = new Intl.DateTimeFormat("pt-BR", { timeZone: "UTC", month: "short", year: "numeric" });

export const fmtInt = (n: number) => intFmt.format(Math.round(n));
export const fmtDec = (n: number) => decFmt.format(n);
export const fmtPct = (n: number) => `${decFmt.format(n)}%`;
export const fmtDate = (ts: number) => dateFmt.format(ts * 1000);
export const fmtTime = (ts: number) => timeFmt.format(ts * 1000);
export const fmtDateTime = (ts: number) => `${fmtDate(ts)} ${fmtTime(ts)}`;
export const fmtLongDate = (ts: number) => longDateFmt.format(ts * 1000);
export const dayKey = (ts: number) => Math.floor(ts / 86400);

/** "2024-03-05" → timestamp (segundos, UTC). */
export const isoToTs = (iso: string) => Date.parse(`${iso}T00:00:00Z`) / 1000;

export function fmtMonth(isoMonth: string) {
  return monthFmt.format(Date.parse(`${isoMonth}-01T00:00:00Z`));
}

/** Duração legível: "42s", "3min 5s", "2h 10min", "14h32min". */
export function fmtDuration(secs: number) {
  const s = Math.round(secs);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return s % 60 ? `${m}min ${s % 60}s` : `${m}min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h}h ${m % 60}min` : `${h}h`;
}

/** Datas sucessivas (ISO) a partir de `start`, para eixos de séries diárias. */
export function dayLabels(start: string, n: number) {
  const t0 = isoToTs(start);
  return Array.from({ length: n }, (_, i) => new Date((t0 + i * 86400) * 1000).toISOString().slice(0, 10));
}

export function monthLabels(start: string, n: number) {
  const [y, m] = start.split("-").map(Number);
  return Array.from({ length: n }, (_, i) => {
    const idx = y * 12 + (m - 1) + i;
    return `${Math.floor(idx / 12)}-${String((idx % 12) + 1).padStart(2, "0")}`;
  });
}
