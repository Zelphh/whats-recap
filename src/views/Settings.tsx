import { useEffect, useState } from "react";
import { api } from "../api";
import type { Stats, StatsSettings } from "../types";

interface Props {
  id: string;
  hasMedia: boolean;
  onStats: (s: Stats) => void;
}

const parseList = (s: string) => [...new Set(s.split(/[\n,]/).map((w) => w.trim().toLowerCase()).filter(Boolean))];

export function Settings({ id, hasMedia, onStats }: Props) {
  const [settings, setSettings] = useState<StatsSettings | null>(null);
  const [stopwords, setStopwords] = useState("");
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  function load(s: StatsSettings) {
    setSettings(s);
    setStopwords(s.stopwords.join("\n"));
  }

  useEffect(() => {
    api.getSettings(id).then(load).catch((e) => setStatus(String(e)));
  }, [id]);

  if (!settings) return <div className="empty">Carregando…</div>;

  async function save() {
    if (!settings) return;
    setSaving(true);
    setStatus(null);
    try {
      const next = { ...settings, stopwords: parseList(stopwords) };
      onStats(await api.updateSettings(id, next));
      load(next);
      setStatus("Estatísticas recalculadas.");
    } catch (e) {
      setStatus(String(e));
    } finally {
      setSaving(false);
    }
  }

  const set = <K extends keyof StatsSettings>(k: K, v: StatsSettings[K]) => setSettings({ ...settings, [k]: v });

  return (
    <div className="page">
      <div>
        <h1>Configurações da análise</h1>
        <p className="subtitle">Ao salvar, as estatísticas desta conversa são recalculadas.</p>
      </div>
      <div className="card form">
        <div className="field">
          <label htmlFor="gap">Intervalo que inicia uma nova conversa (horas)</label>
          <input id="gap" className="input" type="number" min={0.25} step={0.25} style={{ width: 120 }}
            value={settings.sessionGapSecs / 3600}
            onChange={(e) => set("sessionGapSecs", Math.round(Number(e.target.value) * 3600))} />
          <span className="hint">Usado no tempo de resposta: respostas depois desse intervalo são descartadas.</span>
        </div>
        <div className="field">
          <label htmlFor="freq">Frequência mínima para "palavras exclusivas"</label>
          <input id="freq" className="input" type="number" min={1} style={{ width: 120 }}
            value={settings.minExclusiveFreq} onChange={(e) => set("minExclusiveFreq", Math.max(1, Number(e.target.value)))} />
        </div>
        {hasMedia && (
          <div className="field">
            <label htmlFor="dist">Tolerância para agrupar figurinhas (0–16)</label>
            <input id="dist" className="input" type="number" min={0} max={16} style={{ width: 120 }}
              value={settings.stickerMaxDistance}
              onChange={(e) => set("stickerMaxDistance", Math.min(16, Math.max(0, Number(e.target.value))))} />
            <span className="hint">
              Distância máxima entre os hashes perceptuais (de 64 bits) para duas figurinhas contarem como a mesma.
              0 = só arquivos visualmente idênticos; valores altos podem juntar figurinhas diferentes.
            </span>
          </div>
        )}
        <label className="check">
          <input type="checkbox" checked={settings.stripAccents} onChange={(e) => set("stripAccents", e.target.checked)} />
          Ignorar acentos nas palavras (faz "é" e "e" contarem juntos)
        </label>
        <label className="check">
          <input type="checkbox" checked={settings.groupSkinTones} onChange={(e) => set("groupSkinTones", e.target.checked)} />
          Agrupar tons de pele dos emojis (👍🏽 conta como 👍)
        </label>
        <div className="field">
          <label htmlFor="stop">Palavras ignoradas (stopwords)</label>
          <span className="hint">Uma por linha. Adicione, por exemplo, "kk" para tirar risadas do ranking.</span>
          <textarea id="stop" value={stopwords} onChange={(e) => setStopwords(e.target.value)} />
        </div>
        <div className="row">
          <button className="btn primary" disabled={saving} onClick={save}>{saving ? "Recalculando…" : "Salvar e recalcular"}</button>
          <button className="btn" disabled={saving} onClick={() => api.defaultSettings().then(load)}>Restaurar padrão</button>
          {status && <span className="hint" role="status">{status}</span>}
        </div>
      </div>
    </div>
  );
}
