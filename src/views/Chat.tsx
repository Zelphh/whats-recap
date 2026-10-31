import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Dot, Icon } from "../components/ui";
import { fmtDate, fmtTime } from "../format";
import type { ChatAnswer, ImportSummary, LlmStatus, ToolCall } from "../types";

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
}

const TOOL_LABEL: Record<ToolCall["tool"], string> = {
  count_word: "Contagem de palavra",
  messages_by_hour: "Mensagens por hora do dia",
  messages_by_period: "Mensagens por período",
  first_occurrence: "Primeira ocorrência",
  response_time: "Tempo de resposta",
};

const GRANULARITY: Record<string, string> = {
  day: "dia", week: "semana", month: "mês", year: "ano", weekday: "dia da semana",
};

const ROUTE_NOTE: Partial<Record<ChatAnswer["route"], string>> = {
  retrieval: "Pergunta sobre o conteúdo · em breve",
  global: "Pergunta sobre a conversa inteira · fora do escopo",
};

// Depois disso sem resposta, a pergunta provavelmente foi para o modelo (as regras respondem em ms).
const SLOW_MS = 700;

const PENDING: Record<LlmStatus, string> = {
  off: "Calculando…",
  idle: "Carregando o modelo de linguagem… só na primeira pergunta livre, leva uns 10 segundos.",
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

function isoToBr(iso: string) {
  const [y, m, d] = iso.split("-");
  return `${d}/${m}/${y}`;
}

/** "Como calculei": a ferramenta e os parâmetros, para o usuário conferir a interpretação. */
function HowCalculated({ tool, by }: { tool: ToolCall; by: ChatAnswer["interpretedBy"] }) {
  const { word, author, from, to, granularity } = tool.args;
  const parts = [
    word && `palavra “${word}”`,
    granularity && `por ${GRANULARITY[granularity] ?? granularity}`,
    author && `de ${author}`,
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

export function Chat({ id, summary, turns, setTurns, onOpenMessage }: Props) {
  const [question, setQuestion] = useState("");
  const [busy, setBusy] = useState(false);
  const [examples, setExamples] = useState<string[]>([]);
  const [llm, setLlm] = useState<LlmStatus>("off");
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.chatExamples(id).then(setExamples).catch(() => setExamples([]));
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
    setTurns((t) => [...t, { question: text }]);
    try {
      const answer = await api.chatAsk(id, text);
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
              Pergunte sobre números da conversa entre {summary.authors.join(" e ")}. As respostas são calculadas a partir
              das mensagens; nada sai do seu computador.
            </p>
          </div>

          {turns.length === 0 && (
            <div className="card r-a reveal" style={{ "--i": 1 } as React.CSSProperties}>
              <div className="label">Experimente perguntar</div>
              <div className="suggest">
                {examples.map((ex) => <button key={ex} onClick={() => ask(ex)}>{ex}</button>)}
              </div>
              <p className="note">
                Por enquanto o chat entende perguntas de contagem (quantas vezes, quem mais, qual mês, horários, tempo de
                resposta). Perguntas sobre o conteúdo e perguntas livres chegam com o modelo de linguagem.
              </p>
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
                  {t.answer.tool && <HowCalculated tool={t.answer.tool} by={t.answer.interpretedBy} />}
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
