import { useEffect, useState } from "react";
import { api } from "../api";
import type { Stats, StatsSettings } from "../types";

interface Props {
  id: string;
  names: string;
  hasMedia: boolean;
  onStats: (s: Stats) => void;
}

const parseList = (s: string) => [...new Set(s.split(/[\n,]/).map((w) => w.trim().toLowerCase()).filter(Boolean))];
const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/** Número grande editável, com botões − e +. */
function Stepper({ value, onChange, step, min, max, inputStep, unit, label }: {
  value: number; onChange: (v: number) => void; step: number; min: number; max: number; inputStep: number; unit: string; label: string;
}) {
  return (
    <div className="stepper">
      <button onClick={() => onChange(clamp(value - step, min, max))} aria-label={`Diminuir ${label}`} disabled={value <= min}>−</button>
      <input type="number" min={min} max={max} step={inputStep} value={value} aria-label={label}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isFinite(v) && e.target.value !== "") onChange(clamp(v, min, max));
        }} />
      <button onClick={() => onChange(clamp(value + step, min, max))} aria-label={`Aumentar ${label}`} disabled={value >= max}>+</button>
      <span>{unit}</span>
    </div>
  );
}

function Switch({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint: string }) {
  return (
    <button className="switch" role="switch" aria-checked={checked} onClick={() => onChange(!checked)}>
      <span><b>{label}</b><small>{hint}</small></span>
      <span className="switch-track" aria-hidden />
    </button>
  );
}

export function Settings({ id, names, hasMedia, onStats }: Props) {
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

  if (!settings) return <div className="loading">{status ?? "Carregando…"}</div>;

  async function save() {
    if (!settings) return;
    setSaving(true);
    setStatus(null);
    const t0 = performance.now();
    try {
      const next = { ...settings, stopwords: parseList(stopwords) };
      onStats(await api.updateSettings(id, next));
      load(next);
      const secs = (performance.now() - t0) / 1000;
      setStatus(`Estatísticas recalculadas em ${secs.toLocaleString("pt-BR", { maximumFractionDigits: 1 })} s.`);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function reset() {
    load(await api.defaultSettings());
    setStatus("Valores padrão restaurados. Salve para recalcular.");
  }

  const set = <K extends keyof StatsSettings>(k: K, v: StatsSettings[K]) => setSettings({ ...settings, [k]: v });

  return (
    <div className="page narrow">
      <div className="page-head reveal">
        <div className="kicker">{names}</div>
        <h1 className="title">Configurações da análise</h1>
        <p className="lead">Ao salvar, as estatísticas desta conversa são recalculadas aqui mesmo.</p>
      </div>

      <div className="settings-grid">
        <div className="settings-col">
          <div className="setting r-a reveal" style={{ "--i": 1 } as React.CSSProperties}>
            <div>
              <div className="setting-title">Nova conversa depois de</div>
              <div className="setting-hint">
                Um silêncio maior que isso começa outra conversa (sessão). No tempo de resposta, respostas depois desse
                intervalo são descartadas.
              </div>
            </div>
            <Stepper label="intervalo em horas" unit="horas" value={settings.sessionGapSecs / 3600} step={1} min={0.25} max={168} inputStep={0.25}
              onChange={(h) => set("sessionGapSecs", Math.round(h * 3600))} />
          </div>

          <div className="setting r-b reveal" style={{ "--i": 2 } as React.CSSProperties}>
            <div>
              <div className="setting-title">Frequência mínima</div>
              <div className="setting-hint">Quantas vezes uma palavra precisa aparecer para entrar em “palavras exclusivas”.</div>
            </div>
            <Stepper label="frequência mínima" unit="vezes" value={settings.minExclusiveFreq} step={1} min={1} max={10000} inputStep={1}
              onChange={(v) => set("minExclusiveFreq", Math.round(v))} />
          </div>

          <div className="switches reveal" style={{ "--i": 3 } as React.CSSProperties}>
            <Switch label="Ignorar acentos" hint="“você” e “voce” contam como a mesma palavra"
              checked={settings.stripAccents} onChange={(v) => set("stripAccents", v)} />
            <Switch label="Agrupar tons de pele dos emojis" hint="👍🏽 e 👍🏿 contam como 👍"
              checked={settings.groupSkinTones} onChange={(v) => set("groupSkinTones", v)} />
          </div>

          {hasMedia && (
            <div className="setting r-c reveal" style={{ "--i": 4 } as React.CSSProperties}>
              <div className="spread" style={{ alignItems: "baseline" }}>
                <label className="setting-title" htmlFor="dist">Tolerância para agrupar figurinhas</label>
                <span className="big-val">{settings.stickerMaxDistance}</span>
              </div>
              <input id="dist" className="range" type="range" min={0} max={16} step={1} value={settings.stickerMaxDistance}
                onChange={(e) => set("stickerMaxDistance", Number(e.target.value))} />
              <div className="spread note"><span>0 · só idênticas</span><span>16 · agrupa bastante</span></div>
              <div className="setting-hint" style={{ marginTop: 0 }}>
                Distância máxima entre os hashes perceptuais (de 64 bits) para duas figurinhas contarem como a mesma.
                Valores altos podem juntar figurinhas diferentes.
              </div>
            </div>
          )}
        </div>

        <div className="setting r-d reveal" style={{ "--i": 2, gap: 12 } as React.CSSProperties}>
          <div>
            <label className="setting-title" htmlFor="stop">Palavras ignoradas</label>
            <div className="setting-hint">
              Uma por linha. Dica: adicione <code className="kbd">kk</code> para tirar as risadas do ranking.
            </div>
          </div>
          <textarea id="stop" className="stopwords" rows={14} spellCheck={false} value={stopwords} onChange={(e) => setStopwords(e.target.value)} />
        </div>
      </div>

      <div className="save-bar">
        <button className="btn primary" disabled={saving} onClick={save}>
          {saving && <span className="spinner" aria-hidden />}
          {saving ? "Recalculando…" : "Salvar e recalcular"}
        </button>
        <button className="btn" disabled={saving} onClick={reset}>Restaurar padrão</button>
        <span role="status">{status}</span>
      </div>
    </div>
  );
}
