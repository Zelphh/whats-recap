import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { Dot, Icon } from "../components/ui";
import { fmtDate, fmtInt, fmtTime } from "../format";
import type {
  ChatAnswer, ImportSummary, ListFilter, LlmStatus, PreviousTurn, RagInfo, RagProgressEvent, ToolCall,
} from "../types";
import type { Listing } from "./Viewer";

export interface ChatTurn {
  question: string;
  answer?: ChatAnswer;
  error?: string;
}

interface Props {
  id: string;
  summary: ImportSummary;
  turns: ChatTurn[];
  setTurns: (update: (turns: ChatTurn[]) => ChatTurn[]) => void;
  onOpenMessage: (id: number) => void;
  onOpenListing: (listing: Listing) => void;
}

const TOOL_LABEL: Record<ToolCall["tool"], string> = {
  count_word: "Contagem de palavra",
  messages_by_hour: "Mensagens por hora do dia",
  messages_by_period: "Mensagens por período",
  first_occurrence: "Primeira ocorrência",
  response_time: "Tempo de resposta",
  list_messages: "Lista de mensagens",
};

const GRANULARITY: Record<string, string> = {
  day: "dia", week: "semana", month: "mês", year: "ano", weekday: "dia da semana",
};

const ROUTE_NOTE: Partial<Record<ChatAnswer["route"], string>> = {
  retrieval: "Pergunta sobre o conteúdo",
  global: "Pergunta sobre a conversa inteira · fora do escopo",
};

/** "uns 3 minutos", "menos de um minuto". */
function fmtEta(secs: number) {
  if (secs < 60) return "menos de um minuto";
  const min = Math.round(secs / 60);
  return min === 1 ? "cerca de 1 minuto" : `cerca de ${fmtInt(min)} minutos`;
}

/**
 * Perguntas sobre o conteúdo: o que falta (modelos, indexação) e o progresso da indexação, que
 * roda em segundo plano e continua de onde parou se for interrompida.
 */
function RagCard({ id }: { id: string }) {
  const [info, setInfo] = useState<RagInfo | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Início da medição da velocidade (para a estimativa de tempo).
  const start = useRef<{ t: number; done: number } | null>(null);
  const [eta, setEta] = useState<number | null>(null);

  const refresh = useCallback(() => {
    api.ragStatus(id).then(setInfo).catch((e) => setError(String(e)));
  }, [id]);

  useEffect(() => {
    refresh();
    const un = listen<RagProgressEvent>("rag-progress", (e) => {
      if (e.payload.id !== id) return;
      const { done, total } = e.payload;
      setProgress({ done, total });
      const now = performance.now();
      if (!start.current) start.current = { t: now, done };
      const s = start.current;
      if (done > s.done && now - s.t > 3000) setEta(((total - done) * (now - s.t)) / 1000 / (done - s.done));
    });
    return () => { un.then((f) => f()); };
  }, [id, refresh]);

  async function run() {
    setError(null);
    setEta(null);
    start.current = null;
    setInfo((i) => i && { ...i, running: true });
    try {
      await api.ragIndex(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setProgress(null);
      refresh();
    }
  }

  if (!info) return null;
  if (!info.llmAvailable || !info.embedderDownloaded) {
    return (
      <p className="note">
        Para perguntas sobre o conteúdo (“quando a gente combinou a viagem?”), baixe o modelo de linguagem e a busca por
        significado em <b>Configurações</b>.
      </p>
    );
  }
  if (info.ready && !info.running) {
    return (
      <p className="note">
        Conversa indexada: perguntas sobre o conteúdo são respondidas pelo modelo a partir das mensagens, com citações.
      </p>
    );
  }
  const done = progress?.done ?? info.embedded;
  const total = progress?.total ?? info.total;
  const started = total > 0 && !info.stale;
  return (
    <div className="card soft rag-card">
      <div className="spread" style={{ alignItems: "flex-start" }}>
        <div>
          <div className="label">Perguntas sobre o conteúdo</div>
          <p className="note">
            {info.running
              ? "Indexando a conversa… pode usar o app enquanto isso; as contagens continuam funcionando."
              : started
                ? `Indexação pausada em ${Math.floor((100 * done) / total)}%. Continua de onde parou.`
                : `Para responder perguntas como “o que ela disse sobre o emprego novo?”, a conversa precisa ser indexada uma vez (${fmtInt(info.messageCount)} mensagens; leva cerca de 1,5 minuto a cada 100 mil, na CPU).`}
          </p>
        </div>
        {info.running ? (
          <button className="btn sm" onClick={() => api.ragCancel()}>Pausar</button>
        ) : (
          <button className="btn sm primary" disabled={info.busyElsewhere} onClick={run}
            title={info.busyElsewhere ? "Outra conversa está sendo indexada." : undefined}>
            {started ? "Continuar" : "Indexar conversa"}
          </button>
        )}
      </div>
      {(info.running || started) && (
        <div className="llm-progress" role="status">
          <div className="meter"><div style={{ width: `${total ? (100 * done) / total : 0}%` }} /></div>
          <span className="note">
            {fmtInt(done)} de {fmtInt(total)} trechos{info.running && eta != null ? ` · falta ${fmtEta(eta)}` : ""}
          </span>
        </div>
      )}
      {error && <div className="error-box" role="alert">{error}</div>}
    </div>
  );
}

// Depois disso sem resposta, a pergunta provavelmente foi para o modelo (as regras respondem em ms).
const SLOW_MS = 700;

const PENDING: Record<LlmStatus, string> = {
  off: "Calculando…",
  idle: "Carregando o modelo de linguagem… só na primeira pergunta livre, leva uns 10 segundos.",
  loading: "Carregando o modelo de linguagem… só na primeira pergunta livre, leva uns 10 segundos.",
  ready: "Interpretando a pergunta com o modelo de linguagem…",
};

/** "Calculando…" e, se demorar, o que o modelo está fazendo. */
function Pending({ llm }: { llm: LlmStatus }) {
  const [slow, setSlow] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setSlow(true), SLOW_MS);
    return () => clearTimeout(t);
  }, []);
  return <div className="a-bubble pending" role="status">{slow ? PENDING[llm] : "Calculando…"}</div>;
}

/** Total de uma resposta `list_messages` (o resultado bruto vem do Rust). */
function listTotal(a: ChatAnswer) {
  return (a.result as { total?: number } | null)?.total ?? 0;
}

function isoToBr(iso: string) {
  const [y, m, d] = iso.split("-");
  return `${d}/${m}/${y}`;
}

/** "das 7h às 8h" (`hour_to` exclusiva; iguais = aquela hora), "a partir das 22h", "antes das 7h". */
function hoursLabel(from?: number, to?: number) {
  if (from === undefined && to === undefined) return "";
  if (to === undefined) return `a partir das ${from}h`;
  if (from === undefined) return `antes das ${to}h`;
  return `das ${from}h às ${(from === to ? from + 1 : to) % 24}h`;
}

/** Título do painel com a lista completa: "Ana · das 7h às 8h · “amor”". */
function listingLabel(f: ListFilter) {
  const parts = [
    f.author,
    hoursLabel(f.hour_from, f.hour_to),
    f.word && `“${f.word}”`,
    f.from && f.from === f.to ? isoToBr(f.from) : [f.from && `desde ${isoToBr(f.from)}`, f.to && `até ${isoToBr(f.to)}`].filter(Boolean).join(" "),
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : "Mensagens";
}

/** "Como calculei": a ferramenta e os parâmetros, para o usuário conferir a interpretação. */
function HowCalculated({ tool, by }: { tool: ToolCall; by: ChatAnswer["interpretedBy"] }) {
  const { word, author, from, to, granularity, hour_from, hour_to } = tool.args;
  const parts = [
    word && `palavra “${word}”`,
    granularity && `por ${GRANULARITY[granularity] ?? granularity}`,
    author && `de ${author}`,
    hoursLabel(hour_from, hour_to),
    from && `desde ${isoToBr(from)}`,
    to && `até ${isoToBr(to)}`,
  ].filter(Boolean);
  return (
    <div className="how">
      <span className="chip" style={{ fontSize: 12 }}>{TOOL_LABEL[tool.tool] ?? tool.tool}</span>
      {parts.length > 0 && <span>{parts.join(" · ")}</span>}
      {by === "llm" && <span title="A pergunta foi interpretada pelo modelo de linguagem; a contagem é exata.">· interpretado pelo modelo</span>}
    </div>
  );
}

export function Chat({ id, summary, turns, setTurns, onOpenMessage, onOpenListing }: Props) {
  const [question, setQuestion] = useState("");
  const [busy, setBusy] = useState(false);
  const [examples, setExamples] = useState<string[]>([]);
  const [llm, setLlm] = useState<LlmStatus>("off");
  const [me, setMe] = useState<string | null | undefined>(undefined);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.chatExamples(id).then(setExamples).catch(() => setExamples([]));
    api.getChatMe(id).then(setMe).catch(() => setMe(undefined));
    // Sobe o modelo e aquece os prompts desta conversa enquanto a pessoa escreve a pergunta.
    let alive = true;
    api.llmStatus().then((s) => alive && setLlm(s)).catch(() => {});
    api.llmPrepare(id)
      .catch(() => {}) // o erro aparece na pergunta (e nas configurações)
      .finally(() => api.llmStatus().then((s) => alive && setLlm(s)).catch(() => {}));
    return () => { alive = false; };
  }, [id]);

  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns]);

  async function ask(q: string) {
    const text = q.trim();
    if (!text || busy) return;
    setQuestion("");
    setBusy(true);
    api.llmStatus().then(setLlm).catch(() => setLlm("off"));
    const index = turns.length;
    const last = turns[index - 1];
    const previous: PreviousTurn | null = last?.answer
      ? {
          question: last.question,
          route: last.answer.route,
          answer: last.answer.text,
          citations: last.answer.citations.map((c) => c.id),
        }
      : null;
    setTurns((t) => [...t, { question: text }]);
    try {
      const answer = await api.chatAsk(id, text, previous);
      setTurns((t) => t.map((turn, i) => (i === index ? { ...turn, answer } : turn)));
    } catch (e) {
      setTurns((t) => t.map((turn, i) => (i === index ? { ...turn, error: String(e) } : turn)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="chat">
      <div className="chat-scroll">
        <div className="chat-inner">
          <div className="page-head reveal">
            <div className="kicker">pergunte sobre a conversa</div>
            <h1 className="title">Chat</h1>
            <p className="lead">
              Pergunte sobre a conversa entre {summary.authors.join(" e ")}: números são calculados a partir das
              mensagens, e perguntas sobre o conteúdo são respondidas com as mensagens citadas. Nada sai do seu computador.
            </p>
            <RagCard id={id} />
          </div>

          {turns.length === 0 && (
            <div className="card r-a reveal" style={{ "--i": 1 } as React.CSSProperties}>
              <div className="label">Experimente perguntar</div>
              <div className="suggest">
                {examples.map((ex) => <button key={ex} onClick={() => ask(ex)}>{ex}</button>)}
              </div>
              <p className="note">
                As regras fixas entendem perguntas de contagem (quantas vezes, quem mais, qual mês, horários, tempo de
                resposta). Perguntas em outras palavras e sobre o conteúdo usam o modelo de linguagem.
              </p>
              {llm === "off" && (
                <p className="note">
                  Para perguntas em outras palavras, baixe o modelo de linguagem em <b>Configurações</b>.
                </p>
              )}
              {me === null && (
                <p className="note">
                  Para o chat entender “eu” e “ela”/“ele”, diga quem é você em <b>Configurações</b>.
                </p>
              )}
            </div>
          )}

          {turns.map((t, i) => (
            <div key={i} className="turn">
              <div className="q-bubble">{t.question}</div>
              {!t.answer && !t.error && <Pending llm={llm} />}
              {t.error && <div className="a-bubble error" role="alert">{t.error}</div>}
              {t.answer && (
                <div className="a-bubble">
                  {ROUTE_NOTE[t.answer.route] && <span className="chip lime" style={{ fontSize: 12 }}>{ROUTE_NOTE[t.answer.route]}</span>}
                  <p className="a-text">{t.answer.text}</p>
                  {t.answer.citations.length > 0 && (
                    <div className="citations">
                      {t.answer.citations.map((c) => {
                        const a = c.author ? summary.authors.indexOf(c.author) : -1;
                        return (
                          <button key={c.id} className="result" onClick={() => onOpenMessage(c.id)}>
                            <span className="meta">
                              <b>#{c.id}</b>·{a >= 0 && <Dot i={a} />}{c.author ?? "sistema"} · {fmtDate(c.ts)} {fmtTime(c.ts)}
                            </span>
                            <span className="snip">{(c.text ?? "").slice(0, 160)}</span>
                          </button>
                        );
                      })}
                    </div>
                  )}
                  {t.answer.tool?.tool === "list_messages" && listTotal(t.answer) > t.answer.citations.length && (
                    <button className="btn ink sm"
                      onClick={() => onOpenListing({ filter: t.answer!.tool!.args, label: listingLabel(t.answer!.tool!.args) })}>
                      Ver as {fmtInt(listTotal(t.answer))} na conversa →
                    </button>
                  )}
                  {t.answer.tool && <HowCalculated tool={t.answer.tool} by={t.answer.interpretedBy} />}
                  {t.answer.route === "retrieval" && t.answer.interpretedBy === "llm" && (
                    <div className="how">
                      <span title="O modelo leu só os trechos da conversa mais parecidos com a pergunta; as citações foram conferidas.">
                        {t.answer.followup
                          ? "entendida como continuação da pergunta anterior; respondido pelo modelo a partir dos trechos dela e desta"
                          : "respondido pelo modelo a partir dos trechos mais parecidos com a pergunta"}
                      </span>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
          <div ref={end} />
        </div>
      </div>

      <form
        className="ask-bar"
        onSubmit={(e) => {
          e.preventDefault();
          ask(question);
        }}
      >
        <input
          placeholder="Ex.: quantas vezes a gente falou “saudade” em 2024?"
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          disabled={busy}
          aria-label="Pergunta"
        />
        <button className="send" type="submit" disabled={busy || !question.trim()} aria-label="Perguntar">
          <Icon name="send" size={18} stroke={2.2} />
        </button>
      </form>
    </div>
  );
}
