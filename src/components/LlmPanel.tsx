import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { fmtBytes, fmtDec } from "../format";
import type { LlmActive, LlmDownloadEvent, LlmInfo, LlmModel } from "../types";

const mib = (n: number) => n * 1024 ** 2;

function where(a: LlmActive) {
  return a.backend === "gpu" ? `na GPU${a.device ? ` (${a.device})` : ""}` : "na CPU (mais lento)";
}

/** Progresso de um download, com a velocidade medida entre eventos. */
interface Progress extends LlmDownloadEvent {
  speed: number | null;
}

function ModelRow({ m, info, progress, busy, onDownload, onCancel, onDelete }: {
  m: LlmModel; info: LlmInfo; progress: Progress | null; busy: boolean;
  onDownload: () => void; onCancel: () => void; onDelete: () => void;
}) {
  const recommended = m.id === info.models[0]?.id;
  const fits = !info.gpu || info.gpu.freeMib >= m.minVramMib || info.active?.modelId === m.id;
  const p = progress?.modelId === m.id ? progress : null;
  return (
    <div className="llm-model">
      <div className="spread" style={{ alignItems: "flex-start" }}>
        <div>
          <div className="llm-model-name">
            {m.name} {recommended && <span className="chip lime" style={{ fontSize: 12 }}>recomendado</span>}
          </div>
          <div className="setting-hint">
            {fmtBytes(m.size)} · precisa de {fmtDec(m.minVramMib / 1024)} GB de memória de vídeo livre para rodar na GPU
            {info.gpu && !fits && " (não cabe agora)"}
          </div>
        </div>
        <div className="llm-actions">
          {m.downloading ? (
            <button className="btn sm" onClick={onCancel}>Pausar</button>
          ) : m.downloaded ? (
            m.removable && <button className="btn sm" disabled={busy} onClick={onDelete}>Apagar</button>
          ) : (
            <button className="btn sm primary" disabled={busy} onClick={onDownload}>
              {m.partialBytes ? "Continuar download" : "Baixar"}
            </button>
          )}
        </div>
      </div>
      {m.downloading && (
        <div className="llm-progress" role="status">
          <div className="meter"><div style={{ width: `${p ? (100 * p.done) / p.total : 0}%` }} /></div>
          <span className="note">
            {!p ? "Começando…"
              : p.phase === "resume" ? `Conferindo o que já foi baixado… ${fmtBytes(p.done)} de ${fmtBytes(p.total)}`
                : `${fmtBytes(p.done)} de ${fmtBytes(p.total)}${p.speed ? ` · ${fmtBytes(p.speed)}/s` : ""}`}
          </span>
        </div>
      )}
      {!m.downloading && !m.downloaded && m.partialBytes != null && (
        <div className="note">Download pausado em {fmtBytes(m.partialBytes)} de {fmtBytes(m.size)}.</div>
      )}
      {m.downloaded && m.path && <div className="note llm-path" title={m.path}>{m.path}</div>}
    </div>
  );
}

/** Modelo de linguagem do chat: vale para todas as conversas. */
export function LlmPanel() {
  const [info, setInfo] = useState<LlmInfo | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const last = useRef<{ t: number; done: number } | null>(null);

  const refresh = useCallback(() => {
    api.llmInfo().then(setInfo).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const un = listen<LlmDownloadEvent>("llm-download", (e) => {
      const now = performance.now();
      const prev = last.current;
      let speed: number | null = null;
      if (e.payload.phase === "download" && prev && now > prev.t && e.payload.done >= prev.done) {
        speed = ((e.payload.done - prev.done) * 1000) / (now - prev.t);
      }
      last.current = e.payload.phase === "download" ? { t: now, done: e.payload.done } : null;
      // Média móvel: a velocidade entre dois eventos de 200 ms oscila muito.
      setProgress((p) => ({
        ...e.payload,
        speed: speed == null ? p?.speed ?? null : p?.speed ? p.speed * 0.8 + speed * 0.2 : speed,
      }));
    });
    return () => { un.then((f) => f()); };
  }, [refresh]);

  async function download(m: LlmModel) {
    setError(null);
    setProgress(null);
    last.current = null;
    // O download corre no backend; a lista mostra o progresso enquanto isso.
    setInfo((i) => i && { ...i, models: i.models.map((x) => (x.id === m.id ? { ...x, downloading: true } : x)) });
    try {
      await api.llmDownload(m.id);
    } catch (e) {
      if (!String(e).includes("cancelado")) setError(String(e));
    } finally {
      setProgress(null);
      refresh();
    }
  }

  async function remove(m: LlmModel) {
    const ok = await ask(`Apagar o ${m.name} (${fmtBytes(m.size)})? Para usar de novo, será preciso baixar outra vez.`, {
      title: "Apagar modelo", kind: "warning", okLabel: "Apagar", cancelLabel: "Cancelar",
    });
    if (!ok) return;
    setBusy(true);
    try {
      await api.llmDeleteModel(m.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      refresh();
    }
  }

  async function stop() {
    setBusy(true);
    try {
      await api.llmStop();
    } finally {
      setBusy(false);
      refresh();
    }
  }

  if (!info) {
    return (
      <div className="setting llm-panel">
        <div className="setting-title">Modelo de linguagem</div>
        <div className="note">{error ?? "Verificando o modelo e a placa de vídeo…"}</div>
      </div>
    );
  }

  const anyDownloaded = info.models.some((m) => m.downloaded);
  const status = !info.runtime
    ? "O llama-server não veio nesta instalação: o chat usa só as regras fixas."
    : info.status === "loading"
      ? "Carregando…"
      : info.active
        ? `Carregado: ${info.active.modelName} ${where(info.active)}.`
        : !anyDownloaded
          ? "Nenhum modelo baixado: o chat usa só as regras fixas."
          : info.next
            ? `Não carregado. Ao abrir o chat, carrega o ${info.next.modelName} ${where(info.next)}.`
            : "Não carregado.";

  return (
    <div className="setting llm-panel reveal" style={{ "--i": 3 } as React.CSSProperties}>
      <div className="spread" style={{ alignItems: "flex-start" }}>
        <div>
          <div className="setting-title">Modelo de linguagem</div>
          <div className="setting-hint">
            Interpreta as perguntas livres do chat que as regras fixas não entendem; as contagens continuam exatas. Roda
            neste computador e vale para todas as conversas.
          </div>
        </div>
        {info.active && <button className="btn sm" disabled={busy} onClick={stop}>Descarregar</button>}
      </div>

      <div className="llm-status">
        <span className={`chip${info.active ? " lime" : ""}`} style={{ fontSize: 12 }}>
          {info.active ? "pronto" : info.status === "loading" ? "carregando" : anyDownloaded ? "em espera" : "desligado"}
        </span>
        <span>{status}</span>
      </div>
      <div className="note">
        {info.gpu
          ? `Placa de vídeo: ${info.gpu.name} · ${fmtBytes(mib(info.gpu.freeMib))} livres de ${fmtBytes(mib(info.gpu.totalMib))}${info.active?.backend === "gpu" ? " (já descontando o modelo)" : ""}.`
          : info.runtime
            ? "Nenhuma placa de vídeo compatível (Vulkan) encontrada: o modelo roda na CPU, bem mais devagar."
            : null}
      </div>

      {info.runtime && (
        <div className="llm-models">
          {info.models.map((m) => (
            <ModelRow key={m.id} m={m} info={info} progress={progress} busy={busy || info.models.some((x) => x.downloading)}
              onDownload={() => download(m)} onCancel={() => api.llmCancelDownload()} onDelete={() => remove(m)} />
          ))}
        </div>
      )}
      {info.runtime && (
        <div className="note">
          Os modelos são baixados do Hugging Face uma única vez e conferidos pelo SHA-256; as conversas nunca saem do
          computador. Um download pausado continua de onde parou.
        </div>
      )}
      {(error ?? info.lastError) && <div className="error-box" role="alert">{error ?? info.lastError}</div>}
    </div>
  );
}
