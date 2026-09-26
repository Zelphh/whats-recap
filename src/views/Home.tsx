import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { fmtDate, fmtInt, fmtPct } from "../format";
import type { ConversationInfo, ImportProgressEvent } from "../types";

const PHASES: Record<ImportProgressEvent["phase"], string> = {
  detect: "Detectando formato…",
  parse: "Lendo mensagens…",
  index: "Criando índices…",
  media: "Processando figurinhas e áudios…",
  stats: "Calculando estatísticas…",
};

interface Props {
  conversations: ConversationInfo[];
  onOpen: (id: string) => void;
  onChanged: () => void;
}

export function Home({ conversations, onOpen, onChanged }: Props) {
  const [progress, setProgress] = useState<ImportProgressEvent | null>(null);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [over, setOver] = useState(false);

  async function importPath(path: string) {
    if (importing) return;
    const lower = path.toLowerCase();
    if (!lower.endsWith(".txt") && !lower.endsWith(".zip")) {
      setError("Selecione o arquivo .txt (sem mídia) ou .zip (com mídia) exportado pelo WhatsApp.");
      return;
    }
    setError(null);
    setImporting(true);
    setProgress(null);
    try {
      const info = await api.importConversation(path);
      onChanged();
      onOpen(info.id);
    } catch (e) {
      const msg = String(e);
      setError(msg.includes("cancelada") ? "Importação cancelada." : msg);
    } finally {
      setImporting(false);
    }
  }

  useEffect(() => {
    const unProgress = listen<ImportProgressEvent>("import-progress", (e) => setProgress(e.payload));
    const unDrop = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over" || e.payload.type === "enter") setOver(true);
      else if (e.payload.type === "leave") setOver(false);
      else if (e.payload.type === "drop") {
        setOver(false);
        if (e.payload.paths[0]) importPath(e.payload.paths[0]);
      }
    });
    return () => {
      unProgress.then((f) => f());
      unDrop.then((f) => f());
    };
  }, [importing]);

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

  // Cada fase ocupa uma fatia da barra, para o progresso nunca voltar.
  const overall = progress
    ? { detect: 0, parse: 0.05, index: 0.55, media: 0.62, stats: 0.94 }[progress.phase] +
      progress.fraction * { detect: 0.05, parse: 0.5, index: 0.07, media: 0.32, stats: 0.06 }[progress.phase]
    : 0;

  return (
    <div className="page">
      <div>
        <h1>Importar conversa</h1>
        <p className="subtitle">Tudo é processado nesta máquina: nenhuma mensagem sai do seu computador.</p>
      </div>

      <div className={`dropzone${over ? " over" : ""}`}>
        {importing ? (
          <>
            <strong>{progress ? PHASES[progress.phase] : "Preparando…"}</strong>
            <div className="progress" role="progressbar" aria-valuenow={Math.round(overall * 100)}>
              <div style={{ width: `${overall * 100}%` }} />
            </div>
            <span className="hint">
              {progress
                ? progress.phase === "media"
                  ? `${fmtInt(progress.messages)} arquivos de mídia`
                  : `${fmtInt(progress.messages)} mensagens`
                : ""}
            </span>
            <button className="btn small" onClick={() => api.cancelImport()}>Cancelar</button>
          </>
        ) : (
          <>
            <strong>Arraste aqui o arquivo exportado do WhatsApp</strong>
            <span className="hint">
              <b>.txt</b> para análise sem mídia · <b>.zip</b> para incluir figurinhas e áudios
            </span>
            <button className="btn primary" onClick={pick}>Escolher arquivo</button>
          </>
        )}
      </div>
      {error && <div className="error" role="alert">{error}</div>}

      <div className="card">
        <div className="card-head">
          <h2>Conversas importadas</h2>
        </div>
        {conversations.length === 0 ? (
          <p className="hint">Nenhuma conversa ainda. No WhatsApp: abra a conversa → ⋮ → Mais → Exportar conversa.</p>
        ) : (
          <div className="history">
            {conversations.map((c) => (
              <div key={c.id} className="history-item">
                <button className="open" onClick={() => onOpen(c.id)}>
                  <strong>{c.summary.authors.join(" & ")}</strong>
                  <div className="hint">
                    {fmtInt(c.summary.messageCount)} mensagens · {fmtDate(c.summary.firstTs)} a {fmtDate(c.summary.lastTs)} ·{" "}
                    {c.summary.platform === "ios" ? "iOS" : "Android"} · {c.summary.hasMedia ? "com mídia" : "sem mídia"}
                    {c.summary.orphanLines > 0 &&
                      ` · ${fmtInt(c.summary.orphanLines)} linhas não reconhecidas (${fmtPct(
                        (c.summary.orphanLines * 100) / Math.max(1, c.summary.messageCount),
                      )})`}
                  </div>
                </button>
                <button className="btn small danger" onClick={() => remove(c)}>Remover</button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
