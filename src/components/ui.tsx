import { useEffect, useRef } from "react";
import { authorVar } from "../theme";

/** Ícones de traço (24×24) usados na navegação e nas mensagens. */
export const ICONS = {
  home: "M12 3v12m0 0-4-4m4 4 4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2",
  dash: "M4 4h7v7H4zM13 4h7v4h-7zM13 10h7v10h-7zM4 13h7v7H4z",
  viewer: "M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12z",
  settings: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M14 4v4M8 10v4M16 16v4",
  llm: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 15l.7 2.3L22 18l-2.3.7L19 21l-.7-2.3L16 18l2.3-.7z",
  chat: "M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12zM9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .8-1 1.5M12 16h.01",
  lock: "M8 11V8a4 4 0 0 1 8 0v3M5 11h14v10H5z",
  close: "M6 6l12 12M18 6 6 18",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-3.5-3.5",
  send: "M5 12h14M13 6l6 6-6 6",
  upload: "M12 4v11m0 0-4-4m4 4 4-4M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2",
  warn: "M12 8v5M12 16.5h.01M10.3 3.9 2.6 17.5A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
  sun: "M12 4V2M12 22v-2M4 12H2M22 12h-2M5.6 5.6 4.2 4.2M19.8 19.8l-1.4-1.4M5.6 18.4l-1.4 1.4M19.8 4.2l-1.4 1.4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
  moon: "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z",
  image: "M4 5h16v14H4zM4 15l4-4 5 5 3-3 4 4M15 9h.01",
  video: "M4 6h11v12H4zM15 10l5-3v10l-5-3",
  doc: "M7 3h7l5 5v13H7zM14 3v5h5",
  sticker: "M4 4h16v9l-7 7H4zM13 20v-7h7",
  play: "M8 5v14l11-7z",
  blocked: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM5.6 5.6l12.8 12.8",
} as const;

export function Icon({ name, size = 20, stroke = 1.8 }: { name: keyof typeof ICONS; size?: number; stroke?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={stroke}
      strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={ICONS[name]} />
    </svg>
  );
}

export function Logo({ size = 38 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden>
      <path d="M20 2c10 0 18 6 18 17s-7 19-18 19S2 31 2 20 10 2 20 2z" fill="var(--accent)" />
      <path d="M11 13h18a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3h-9l-5 4v-4h-4a3 3 0 0 1-3-3v-9a3 3 0 0 1 3-3z" fill="var(--accent-ink)" />
      <path d="M15 24v-4M20 24v-7M25 24v-5" stroke="var(--accent)" strokeWidth="2.4" strokeLinecap="round" />
    </svg>
  );
}

export function Seg<T extends string | number>({ value, options, onChange, label }: {
  value: T; options: [T, string][]; onChange: (v: T) => void; label?: string;
}) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map(([v, text]) => (
        <button key={String(v)} className={v === value ? "on" : ""} aria-pressed={v === value} onClick={() => onChange(v)} title={text}>
          {text}
        </button>
      ))}
    </div>
  );
}

/** Marcador da pessoa: além da cor, a forma distingue (quadrado para p1, círculo para p2). */
export function Dot({ i, style }: { i: number; style?: React.CSSProperties }) {
  return <span className={`dot${i === 1 ? " p2" : ""}`} style={{ background: authorVar(i), ...style }} aria-hidden />;
}

export function Who({ name, i }: { name: string; i: number }) {
  return <span className="who"><Dot i={i} />{name}</span>;
}

export function Legend({ authors }: { authors: string[] }) {
  return (
    <div className="legend">
      {authors.map((a, i) => <Who key={a} name={a} i={i} />)}
    </div>
  );
}

/**
 * Número que sobe de 0 ao valor ao montar. Atualiza o texto direto no DOM (sem re-render do
 * React a cada quadro) e respeita "reduzir movimento".
 */
export function CountUp({ value, format, ms = 1300 }: { value: number; format: (n: number) => string; ms?: number }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      el.textContent = format(value);
      return;
    }
    let raf = 0;
    const t0 = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / ms);
      el.textContent = format(value * (1 - Math.pow(1 - k, 3)));
      if (k < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [value, format, ms]);
  // `format` precisa ser estável (função de módulo), senão a animação reinicia a cada render.
  return <span ref={ref}>{format(0)}</span>;
}
