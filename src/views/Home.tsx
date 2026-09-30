import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { DotField } from "../components/DotField";
import { Icon } from "../components/ui";
import { fmtDate, fmtInt, fmtPct } from "../format";
import type { ConversationInfo, ImportProgressEvent } from "../types";

type Phase = ImportProgressEvent["phase"];

const PHASES: Record<Phase, string> = {
  detect: "Detectando formato…",
  parse: "Lendo mensagens…",
  index: "Criando índices…",
  media: "Processando figurinhas e áudios…",
  stats: "Calculando estatísticas…",
};

// Peso aproximado de cada fase no tempo total; sem mídia, a fase "media" não acontece.
const WEIGHTS: Record<Phase, number> = { detect: 0.05, parse: 0.5, index: 0.07, media: 0.32, stats: 0.06 };

function phasesFor(zip: boolean): Phase[] {
  return zip ? ["detect", "parse", "index", "media", "stats"] : ["detect", "parse", "index", "stats"];
}

/** Início de cada fase na barra (0–1), para o progresso nunca voltar. */
function cutsFor(phases: Phase[]) {
  const total = phases.reduce((s, p) => s + WEIGHTS[p], 0);
  let acc = 0;
  return phases.map((p) => {
    const start = acc / total;
    acc += WEIGHTS[p];
    return { phase: p, start, size: WEIGHTS[p] / total };
  });
}

const TEASERS = [
  { text: "Quem responde mais rápido?", dot: "var(--p1)", top: "4%", left: "-2%", d: 1.2, r: "18px 18px 18px 4px", delay: "0s" },
  { text: "Qual o horário de pico?", dot: "var(--accent)", top: "16%", left: "74%", d: -0.8, r: "18px 18px 4px 18px", delay: "-1.2s" },
  { text: "🥰 × quantas vezes?", dot: "var(--p2)", top: "84%", left: "4%", d: -1.4, r: "4px 18px 18px 18px", delay: "-2.4s" },
  { text: "Quantos dias conversando?", dot: "var(--lime)", top: "78%", left: "58%", d: 1, r: "18px 4px 18px 18px", delay: "-3.6s" },
];

const RADII = ["36px 36px 36px 10px", "10px 36px 36px 36px", "36px 10px 36px 36px", "36px 36px 10px 36px"];
const ROTS = [-2.2, 1.6, -1, 2.4];

interface Props {
  conversations: ConversationInfo[];
  onOpen: (id: string) => void;
  onChanged: () => void;
}

export function Home({ conversations, onOpen, onChanged }: Props) {
  const [progress, setProgress] = useState<ImportProgressEvent | null>(null);
  const [importing, setImporting] = useState<{ name: string; zip: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [over, setOver] = useState(false);
  const busy = useRef(false);
  const dropWrap = useRef<HTMLDivElement>(null);

  // Parallax dos balões: só escreve duas variáveis CSS, sem re-render.
  function onPointerMove(e: React.PointerEvent<HTMLDivElement>) {
    const el = dropWrap.current;
    if (!el) return;
    const r = e.currentTarget.getBoundingClientRect();
    el.style.setProperty("--mx", ((e.clientX - r.left) / r.width - 0.5).toFixed(3));
    el.style.setProperty("--my", ((e.clientY - r.top) / r.height - 0.5).toFixed(3));
  }

  async function importPath(path: string) {
    if (busy.current) return;
    const lower = path.toLowerCase();
    if (!lower.endsWith(".txt") && !lower.endsWith(".zip")) {
      setError("Selecione o arquivo .txt (sem mídia) ou .zip (com mídia) exportado pelo WhatsApp.");
      return;
    }
    busy.current = true;
    setError(null);
    setProgress(null);
    setImporting({ name: path.split(/[\\/]/).pop() ?? path, zip: lower.endsWith(".zip") });
    try {
      const info = await api.importConversation(path);
      onChanged();
      onOpen(info.id);
    } catch (e) {
      const msg = String(e);
      setError(msg.includes("cancelada") ? "Importação cancelada." : msg);
    } finally {
      busy.current = false;
      setImporting(null);
    }
  }

  // Os ouvintes leem `busy` pela ref, então são registrados uma única vez.
  const importRef = useRef(importPath);
  importRef.current = importPath;
  useEffect(() => {
    const unProgress = listen<ImportProgressEvent>("import-progress", (e) => setProgress(e.payload));
    const unDrop = getCurrentWebview().onDragDropEvent((e) => {
      if (busy.current) return;
      if (e.payload.type === "over" || e.payload.type === "enter") setOver(true);
      else if (e.payload.type === "leave") setOver(false);
      else if (e.payload.type === "drop") {
        setOver(false);
        if (e.payload.paths[0]) importRef.current(e.payload.paths[0]);
      }
    });
    return () => {
      unProgress.then((f) => f());
      unDrop.then((f) => f());
    };
  }, []);

  async function pick() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Export do WhatsApp", extensions: ["txt", "zip"] }],
    });
    if (typeof path === "string") importPath(path);
  }

  async function remove(c: ConversationInfo) {
    const ok = await ask(`Remover a análise de ${c.summary.authors.join(" e ")}? O arquivo original não é afetado.`, {
      title: "Remover conversa",
      kind: "warning",
    });
    if (!ok) return;
    await api.deleteConversation(c.id);
    onChanged();
  }

  return (
    <div className="home" onPointerMove={onPointerMove}>
      <DotField />
      <div className="home-inner">
        <div className="hero">
          <div className="hero-copy">
            <div className="pill-local reveal">
              <b><Icon name="lock" size={12} stroke={2.6} /></b>
              100% local · nada vai para a nuvem
            </div>
            <h1 className="reveal" style={{ "--i": 1 } as React.CSSProperties}>
              Suas conversas,{" "}
              <em>
                contadas
                <svg viewBox="0 0 300 24" preserveAspectRatio="none" aria-hidden>
                  <path d="M4 16 C 60 4, 120 22, 180 10 S 270 6, 296 14" fill="none" stroke="var(--lime)" strokeWidth="7" strokeLinecap="round" />
                </svg>
              </em>{" "}
              de volta.
            </h1>
            <p className="hero-lead reveal" style={{ "--i": 2 } as React.CSSProperties}>
              O WhatsRecap lê o export de uma conversa entre duas pessoas e mostra quem fala mais, quem responde mais rápido,
              as palavras, os emojis e os horários de vocês. Tudo acontece aqui, no seu computador.
            </p>
            <div className="steps">
              {[
                ["1", "Exporte", "No WhatsApp: abra a conversa → ⋮ → Mais → Exportar conversa."],
                ["2", "Solte aqui", ".txt sem mídia, ou .zip com figurinhas e áudios."],
                ["3", "Descubra", "Números, gráficos e a conversa inteira para navegar."],
              ].map(([n, title, body], i) => (
                <div key={n} className="step reveal" style={{ "--i": 3 + i } as React.CSSProperties}>
                  <span className="n">{n}</span>
                  <b>{title}</b>
                  <span>{body}</span>
                </div>
              ))}
            </div>
          </div>

          <div className="drop-wrap" ref={dropWrap}>
            <div className="drop-back blob-shape" aria-hidden />
            <div className={`dropzone blob-shape reveal${over ? " over" : ""}`}>
              {importing ? (
                <Progress name={importing.name} zip={importing.zip} progress={progress} />
              ) : error ? (
                <div className="dz-err" role="alert">
                  <div className="dz-err-icon"><Icon name="warn" size={28} stroke={2.2} /></div>
                  <div style={{ fontSize: 15, fontWeight: 800, color: "var(--err)" }}>Não deu para importar</div>
                  <div className="dz-err-msg">{error}</div>
                  <button className="btn ink" onClick={pick}>Tentar outro arquivo</button>
                </div>
              ) : (
                <div className="dz-idle">
                  <div className="dz-icon"><Icon name="upload" size={30} stroke={2} /></div>
                  <div className="dz-title">{over ? "Pode soltar, a gente conta tudo" : "Arraste aqui o arquivo exportado do WhatsApp"}</div>
                  <div className="dz-chips">
                    <span><b>.txt</b> sem mídia</span>
                    <span><b>.zip</b> com figurinhas e áudios</span>
                  </div>
                  <button className="btn primary" onClick={pick}>Escolher arquivo</button>
                </div>
              )}
            </div>
            {TEASERS.map((t) => (
              <div key={t.text} className="parallax" style={{ top: t.top, left: t.left, "--d": t.d } as React.CSSProperties} aria-hidden>
                <div className="teaser" style={{ animationDelay: t.delay }}>
                  <div style={{ borderRadius: t.r }}><i style={{ background: t.dot }} />{t.text}</div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {conversations.length > 0 ? (
          <section style={{ display: "flex", flexDirection: "column", gap: 26 }}>
            <div className="section-head reveal">
              <h2>Conversas importadas</h2>
              <span className="serif-i muted" style={{ fontSize: 24 }}>
                {conversations.length} {conversations.length === 1 ? "conversa" : "conversas"}
              </span>
            </div>
            <div className="history">
              {conversations.map((c, i) => {
                const s = c.summary;
                return (
                  <div key={c.id} className="hcard-wrap reveal" style={{ "--i": i } as React.CSSProperties}>
                    <div className="hcard" style={{ borderRadius: RADII[i % 4], "--rot": `${ROTS[i % 4]}deg` } as React.CSSProperties}>
                      <button className="hcard-open" onClick={() => onOpen(c.id)}>
                        <div className="avatars" aria-hidden>
                          {s.authors.map((a, k) => (
                            <span key={a} style={{ background: `var(--p${k + 1})` }}>{(a.trim()[0] ?? "?").toUpperCase()}</span>
                          ))}
                        </div>
                        <div>
                          <div className="hcard-names">{s.authors.join(" & ")}</div>
                          <div className="hcard-meta"><b>{fmtInt(s.messageCount)}</b> mensagens · {fmtDate(s.firstTs)} – {fmtDate(s.lastTs)}</div>
                        </div>
                        <div className="chips">
                          <span className="chip">{s.platform === "ios" ? "iOS" : "Android"}</span>
                          <span className={`chip${s.hasMedia ? " lime" : ""}`}>{s.hasMedia ? "Com mídia" : "Sem mídia"}</span>
                          {s.orphanLines > 0 && (
                            <span className="chip err">
                              {fmtInt(s.orphanLines)} linhas não reconhecidas ({fmtPct((s.orphanLines * 100) / Math.max(1, s.messageCount))})
                            </span>
                          )}
                        </div>
                      </button>
                      <button className="hcard-remove" onClick={() => remove(c)} title="Remover" aria-label={`Remover conversa de ${s.authors.join(" e ")}`}>
                        <Icon name="close" size={14} stroke={2.4} />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ) : (
          <section className="empty-card reveal">
            <div className="kicker">Ainda nenhuma conversa por aqui</div>
            <h2>Como exportar a conversa no WhatsApp</h2>
            <div className="export-steps">
              {["abra a conversa", "⋮", "Mais", "Exportar conversa"].map((label, i) => (
                <div key={label} className="row">
                  <span>{label}</span>
                  {i < 3 && (
                    <svg width="26" height="16" viewBox="0 0 26 16" fill="none" stroke="var(--mute)" strokeWidth="2" strokeLinecap="round" aria-hidden>
                      <path d="M1 8c6-6 12 6 22 0M18 3l5 5-5 5" />
                    </svg>
                  )}
                </div>
              ))}
            </div>
            <p className="ink2" style={{ fontSize: 16, lineHeight: 1.5, maxWidth: 620 }}>
              Escolha <b>Incluir mídia</b> para gerar um <b>.zip</b> com figurinhas e áudios. Sem mídia, o WhatsApp gera um <b>.txt</b> e o
              app mostra todo o resto.
            </p>
          </section>
        )}
      </div>
    </div>
  );
}

function Progress({ name, zip, progress }: { name: string; zip: boolean; progress: ImportProgressEvent | null }) {
  const cuts = cutsFor(phasesFor(zip));
  const cur = progress ? cuts.findIndex((c) => c.phase === progress.phase) : 0;
  const c = cuts[Math.max(0, cur)];
  const overall = progress && cur >= 0 ? c.start + progress.fraction * c.size : 0;
  return (
    <div className="dz-busy">
      <div className="dz-file" title={name}>{name}</div>
      <div className="spread" style={{ alignItems: "baseline" }}>
        <span className="dz-phase">{progress ? PHASES[progress.phase] : "Preparando…"}</span>
        <span className="dz-pct num">{Math.round(overall * 100)}%</span>
      </div>
      <div className="pbar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(overall * 100)}>
        <div style={{ width: `${overall * 100}%` }} />
        {cuts.slice(1).map((k) => <i key={k.phase} style={{ left: `${k.start * 100}%` }} />)}
      </div>
      <div className="mono small ink2">
        {progress
          ? progress.phase === "media"
            ? `${fmtInt(progress.messages)} arquivos de mídia`
            : `${fmtInt(progress.messages)} mensagens`
          : " "}
      </div>
      <div className="phases">
        {cuts.map((k, i) => (
          <div key={k.phase} className={i < cur ? "done" : i === cur ? "now" : ""}><i />{PHASES[k.phase]}</div>
        ))}
      </div>
      <button className="btn sm" style={{ alignSelf: "flex-start", background: "transparent" }} onClick={() => api.cancelImport()}>Cancelar</button>
    </div>
  );
}
