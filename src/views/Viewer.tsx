import { useCallback, useEffect, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { api } from "../api";
import { dayKey, fmtDate, fmtLongDate, fmtTime } from "../format";
import { Sticker } from "../components/Sticker";
import { Dot, Icon, type ICONS } from "../components/ui";
import type { ImportSummary, MessageKind, MessageRow } from "../types";

const PAGE = 200;
const SEARCH_LIMIT = 200;

// Folga no topo (para a data flutuante) e no fim da lista.
const LIST_COMPONENTS = {
  Header: () => <div style={{ height: 52 }} />,
  Footer: () => <div style={{ height: 24 }} />,
};

const FILE_KIND: Partial<Record<MessageKind, [string, keyof typeof ICONS]>> = {
  image: ["Imagem", "image"],
  video: ["Vídeo", "video"],
  doc: ["Documento", "doc"],
  sticker: ["Figurinha", "sticker"],
};

interface Props {
  id: string;
  summary: ImportSummary;
  jumpTo: number | null;
  onJumpHandled: () => void;
}

/** Trecho do texto em volta da primeira ocorrência da busca, com o termo destacado. */
function Snippet({ text, query }: { text: string; query: string }) {
  const q = query.trim().toLowerCase();
  const i = q ? text.toLowerCase().indexOf(q) : -1;
  if (i < 0) return <>{text.slice(0, 160)}</>;
  const pre = i > 40 ? `…${text.slice(i - 36, i)}` : text.slice(0, i);
  return (
    <>
      {pre}<mark>{text.slice(i, i + q.length)}</mark>{text.slice(i + q.length, i + q.length + 120)}
    </>
  );
}

/** A conversa inteira em lista virtualizada. As mensagens são buscadas em páginas sob demanda. */
export function Viewer({ id, summary, jumpTo, onJumpHandled }: Props) {
  const pages = useRef(new Map<number, MessageRow[]>());
  const loading = useRef(new Set<number>());
  const [, setVersion] = useState(0);
  const list = useRef<VirtuosoHandle>(null);
  const [target, setTarget] = useState<number | null>(jumpTo);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<MessageRow[] | null>(null);
  const [searched, setSearched] = useState("");
  const [date, setDate] = useState("");
  const [msgNo, setMsgNo] = useState("");
  const [topDay, setTopDay] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchSeq = useRef(0);

  useEffect(() => {
    pages.current.clear();
    loading.current.clear();
    setVersion((v) => v + 1);
  }, [id]);

  const loadPage = useCallback(
    (p: number) => {
      if (p < 0 || pages.current.has(p) || loading.current.has(p)) return;
      loading.current.add(p);
      api
        .getMessages(id, p * PAGE + 1, PAGE)
        .then((rows) => {
          pages.current.set(p, rows);
          setVersion((v) => v + 1);
        })
        .catch((e) => setError(String(e)))
        .finally(() => loading.current.delete(p));
    },
    [id],
  );

  // Os ids são sequenciais a partir de 1, então o índice na lista é `id - 1`.
  const row = (index: number) => pages.current.get(Math.floor(index / PAGE))?.[index % PAGE];

  const jump = useCallback((msgId: number) => {
    const index = Math.min(Math.max(msgId - 1, 0), summary.messageCount - 1);
    setTarget(index + 1);
    list.current?.scrollToIndex({ index, align: "center" });
  }, [summary.messageCount]);

  useEffect(() => {
    if (jumpTo !== null) {
      // Espera o Virtuoso montar antes de rolar.
      requestAnimationFrame(() => jump(jumpTo));
      onJumpHandled();
    }
  }, [jumpTo, jump, onJumpHandled]);

  const runSearch = useCallback(async (q: string) => {
    const seq = ++searchSeq.current;
    if (!q.trim()) {
      setResults(null);
      setSearched("");
      return;
    }
    try {
      const rows = await api.searchMessages(id, q, SEARCH_LIMIT);
      if (seq !== searchSeq.current) return; // chegou uma busca mais nova
      setResults(rows);
      setSearched(q);
    } catch (err) {
      setError(String(err));
    }
  }, [id]);

  // Busca enquanto digita, com uma pausa curta para não disparar a cada tecla.
  useEffect(() => {
    if (query.trim().length < 2) {
      if (!query.trim()) runSearch("");
      return;
    }
    const t = setTimeout(() => runSearch(query), 300);
    return () => clearTimeout(t);
  }, [query, runSearch]);

  async function goToDate(value: string) {
    setDate(value);
    if (!value) return;
    const msgId = await api.locateDate(id, value);
    if (msgId !== null) jump(msgId);
    else setError("Nenhuma mensagem nessa data ou depois dela.");
  }

  const firstIso = new Date(summary.firstTs * 1000).toISOString().slice(0, 10);
  const lastIso = new Date(summary.lastTs * 1000).toISOString().slice(0, 10);
  const count = results ? (results.length >= SEARCH_LIMIT ? `${SEARCH_LIMIT}+ resultados` : `${results.length} resultado${results.length === 1 ? "" : "s"}`) : "";

  return (
    <div className="viewer">
      <div className="viewer-bar">
        <h2>Conversa</h2>
        <form className="pill-field search" role="search" onSubmit={(e) => { e.preventDefault(); runSearch(query); }}>
          <span className="muted" style={{ display: "flex" }}><Icon name="search" size={17} stroke={2.2} /></span>
          <input placeholder="Buscar na conversa" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Buscar na conversa" />
          <span className="tiny muted" style={{ whiteSpace: "nowrap", paddingRight: 8 }}>{count}</span>
        </form>
        <label className="pill-field date">
          Ir para data
          <input type="date" min={firstIso} max={lastIso} value={date} onChange={(e) => goToDate(e.target.value)} />
        </label>
        <form
          className="pill-field jump"
          onSubmit={(e) => {
            e.preventDefault();
            const v = Number(msgNo);
            if (v > 0) jump(v);
          }}
        >
          <span aria-hidden>#</span>
          <input inputMode="numeric" placeholder="nº da mensagem" aria-label="Número da mensagem" value={msgNo}
            onChange={(e) => setMsgNo(e.target.value.replace(/\D/g, ""))} />
          <button type="submit">Ir</button>
        </form>
        {error && <button className="viewer-error" onClick={() => setError(null)} title="Fechar">{error} ✕</button>}
      </div>
      <div className="viewer-body">
        <div className="chat-box">
          {topDay && <div className="float-day" aria-hidden>{topDay}</div>}
          <Virtuoso
            ref={list}
            totalCount={summary.messageCount}
            initialTopMostItemIndex={jumpTo !== null ? { index: Math.max(0, jumpTo - 1), align: "center" } : 0}
            increaseViewportBy={600}
            components={LIST_COMPONENTS}
            rangeChanged={({ startIndex, endIndex }) => {
              for (let p = Math.floor(startIndex / PAGE); p <= Math.floor(endIndex / PAGE); p++) loadPage(p);
              const m = row(startIndex);
              setTopDay(m ? fmtLongDate(m.ts) : null);
            }}
            itemContent={(index) => {
              const m = row(index);
              if (!m) {
                loadPage(Math.floor(index / PAGE));
                return <div className="placeholder" style={{ marginLeft: index % 2 ? "auto" : undefined }} />;
              }
              const prev = index > 0 ? row(index - 1) : undefined;
              const newDay = !prev || dayKey(prev.ts) !== dayKey(m.ts);
              return <Message convId={id} hasMedia={summary.hasMedia} m={m} newDay={newDay} right={m.author !== null && m.author !== summary.authors[0]} target={m.id === target} />;
            }}
          />
        </div>
        {results && (
          <aside className="results" aria-label="Resultados da busca">
            <div className="results-head">
              <div><b>Resultados</b><span>{count}</span></div>
              <button className="icon-btn" onClick={() => { setQuery(""); setResults(null); }} aria-label="Fechar resultados">
                <Icon name="close" size={14} stroke={2.4} />
              </button>
            </div>
            <div className="results-list">
              {results.map((r) => {
                const a = r.author ? summary.authors.indexOf(r.author) : -1;
                return (
                  <button key={r.id} className={`result${r.id === target ? " on" : ""}`} onClick={() => jump(r.id)}>
                    <span className="meta">
                      <b>#{r.id}</b>·{a >= 0 && <Dot i={a} />}{r.author ?? "sistema"} · {fmtDate(r.ts)} {fmtTime(r.ts)}
                    </span>
                    <span className="snip"><Snippet text={r.text ?? ""} query={searched} /></span>
                  </button>
                );
              })}
              {results.length === 0 && <div className="empty-note" style={{ padding: "18px 12px" }}>Nada encontrado para essa busca.</div>}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}

interface MessageProps {
  convId: string;
  hasMedia: boolean;
  m: MessageRow;
  newDay: boolean;
  right: boolean;
  target: boolean;
}

function Message({ convId, hasMedia, m, newDay, right, target }: MessageProps) {
  const showSticker = hasMedia && m.kind === "sticker" && !!m.mediaFile;
  const file = FILE_KIND[m.kind];
  return (
    <div className={`msg-row${right ? " right" : ""}`}>
      {newDay && <div className="day-sep">{fmtLongDate(m.ts)}</div>}
      {m.kind === "system" ? (
        <div className="sys-msg">{m.text}</div>
      ) : (
        <div className={`bubble${showSticker ? " bare" : ""}${target ? " target" : ""}`} id={`msg-${m.id}`}>
          <span className="author">{m.author}</span>
          {showSticker && <Sticker id={convId} file={m.mediaFile!} size={96} label="Figurinha" />}
          {!showSticker && file && (
            <div className="file-card">
              <span><Icon name={file[1]} size={16} stroke={2} /></span>
              <span className="fname">{file[0]}{m.mediaFile && <small>{m.mediaFile}</small>}</span>
            </div>
          )}
          {m.kind === "audio" && (
            <div className="file-card audio-chip">
              <span><Icon name="play" size={12} stroke={2} /></span>
              <span className="fname">Áudio{m.mediaFile && <small>{m.mediaFile}</small>}</span>
            </div>
          )}
          {m.kind === "deleted" && <span className="muted-msg"><Icon name="blocked" size={14} stroke={2} />Mensagem apagada</span>}
          {m.kind === "media_hidden" && <span className="muted-msg"><Icon name="blocked" size={14} stroke={2} />Mídia oculta</span>}
          {m.text && m.kind !== "deleted" && m.kind !== "media_hidden" && <span className="text">{m.text}</span>}
          <span className="time">{fmtTime(m.ts)} · #{m.id}</span>
        </div>
      )}
    </div>
  );
}
