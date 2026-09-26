import { useCallback, useEffect, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { api } from "../api";
import { dayKey, fmtDate, fmtLongDate, fmtTime } from "../format";
import { Sticker } from "../components/Sticker";
import type { ImportSummary, MessageKind, MessageRow } from "../types";

const PAGE = 200;

const KIND_LABEL: Partial<Record<MessageKind, string>> = {
  sticker: "Figurinha",
  audio: "Áudio",
  image: "Imagem",
  video: "Vídeo",
  doc: "Documento",
  media_hidden: "Mídia oculta",
  deleted: "Mensagem apagada",
};

interface Props {
  id: string;
  summary: ImportSummary;
  jumpTo: number | null;
  onJumpHandled: () => void;
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
  const [date, setDate] = useState("");
  const [error, setError] = useState<string | null>(null);

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

  async function search(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) return setResults(null);
    try {
      setResults(await api.searchMessages(id, query));
    } catch (err) {
      setError(String(err));
    }
  }

  async function goToDate(value: string) {
    setDate(value);
    if (!value) return;
    const msgId = await api.locateDate(id, value);
    if (msgId !== null) jump(msgId);
    else setError("Nenhuma mensagem nessa data ou depois dela.");
  }

  const firstIso = new Date(summary.firstTs * 1000).toISOString().slice(0, 10);
  const lastIso = new Date(summary.lastTs * 1000).toISOString().slice(0, 10);

  return (
    <div className="viewer">
      <div className="viewer-bar">
        <form onSubmit={search} className="row">
          <input className="input" placeholder="Buscar na conversa…" value={query} onChange={(e) => setQuery(e.target.value)} style={{ width: 260 }} />
          <button className="btn" type="submit">Buscar</button>
        </form>
        <label className="row hint">
          Ir para data
          <input className="input" type="date" min={firstIso} max={lastIso} value={date} onChange={(e) => goToDate(e.target.value)} />
        </label>
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            const v = Number(new FormData(e.currentTarget).get("msg"));
            if (v > 0) jump(v);
          }}
        >
          <input className="input" name="msg" type="number" min={1} max={summary.messageCount} placeholder="Nº da mensagem" style={{ width: 150 }} />
          <button className="btn" type="submit">Ir</button>
        </form>
        {error && <span className="error" onClick={() => setError(null)}>{error}</span>}
      </div>
      <div className={`viewer-body${results ? " with-results" : ""}`}>
        <Virtuoso
          ref={list}
          totalCount={summary.messageCount}
          initialTopMostItemIndex={jumpTo !== null ? { index: Math.max(0, jumpTo - 1), align: "center" } : 0}
          increaseViewportBy={600}
          rangeChanged={({ startIndex, endIndex }) => {
            for (let p = Math.floor(startIndex / PAGE); p <= Math.floor(endIndex / PAGE); p++) loadPage(p);
          }}
          itemContent={(index) => {
            const m = row(index);
            if (!m) {
              loadPage(Math.floor(index / PAGE));
              return <div className="placeholder" />;
            }
            const prev = index > 0 ? row(index - 1) : undefined;
            const newDay = !prev || dayKey(prev.ts) !== dayKey(m.ts);
            return <Message convId={id} hasMedia={summary.hasMedia} m={m} newDay={newDay} right={m.author !== null && m.author !== summary.authors[0]} target={m.id === target} />;
          }}
        />
        {results && (
          <div className="results">
            <div className="row" style={{ justifyContent: "space-between" }}>
              <strong>{results.length === 200 ? "200+ resultados" : `${results.length} resultado(s)`}</strong>
              <button className="btn small" onClick={() => setResults(null)}>Fechar</button>
            </div>
            {results.map((r) => (
              <button key={r.id} className="result" onClick={() => jump(r.id)}>
                <div className="meta">#{r.id} · {r.author ?? "sistema"} · {fmtDate(r.ts)} {fmtTime(r.ts)}</div>
                <div>{(r.text ?? "").slice(0, 160)}</div>
              </button>
            ))}
          </div>
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
  const showSticker = hasMedia && m.kind === "sticker" && m.mediaFile;
  const label = showSticker ? undefined : KIND_LABEL[m.kind];
  return (
    <div className={`msg-row${right ? " right" : ""}`}>
      {newDay && <div className="day-sep">{fmtLongDate(m.ts)}</div>}
      {m.kind === "system" ? (
        <div className="system-msg">{m.text}</div>
      ) : (
        <div className={`bubble${target ? " target" : ""}`} id={`msg-${m.id}`}>
          <div className="who" style={{ color: right ? "var(--series-2)" : "var(--series-1)" }}>{m.author}</div>
          {showSticker && (
            <div className="sticker-msg">
              <Sticker id={convId} file={m.mediaFile!} size={96} label="Figurinha" />
            </div>
          )}
          {label && <div className="kind">{label}{m.mediaFile ? ` · ${m.mediaFile}` : ""}</div>}
          {m.text && <div>{m.text}</div>}
          <div className="time">#{m.id} · {fmtTime(m.ts)}</div>
        </div>
      )}
    </div>
  );
}
