import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { ConversationInfo, Stats } from "./types";
import { Home } from "./views/Home";
import { Dashboard } from "./views/Dashboard";
import { Viewer } from "./views/Viewer";
import { Settings } from "./views/Settings";

type View = "home" | "dashboard" | "viewer" | "settings";

export function App() {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [view, setView] = useState<View>("home");
  const [stats, setStats] = useState<Stats | null>(null);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [jumpTo, setJumpTo] = useState<number | null>(null);

  const refresh = useCallback(() => {
    api.listConversations().then(setConversations).catch(console.error);
  }, []);
  useEffect(refresh, [refresh]);

  useEffect(() => {
    setStats(null);
    setStatsError(null);
    if (!current) return;
    api.getStats(current).then(setStats).catch((e) => setStatsError(String(e)));
  }, [current]);

  const info = conversations.find((c) => c.id === current) ?? null;

  function openConversation(id: string) {
    setCurrent(id);
    setJumpTo(null);
    setView("dashboard");
  }

  function openMessage(id: number) {
    setJumpTo(id);
    setView("viewer");
  }

  const nav = (v: View, label: string, disabled = false) => (
    <button className={`nav-item${view === v ? " active" : ""}`} disabled={disabled} onClick={() => setView(v)}>
      {label}
    </button>
  );

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <img src="/icon.svg" alt="" /> WhatsRecap
        </div>
        {nav("home", "Importar / histórico")}
        {info && (
          <>
            <div className="nav-sep" title={info.summary.authors.join(" & ")}>
              {info.summary.authors.join(" & ")}
            </div>
            {nav("dashboard", "Dashboard")}
            {nav("viewer", "Conversa")}
            {nav("settings", "Configurações")}
            <button className="nav-item" disabled>
              Análises LLM <span className="nav-soon">em breve</span>
            </button>
            <button className="nav-item" disabled>
              Chat <span className="nav-soon">em breve</span>
            </button>
          </>
        )}
      </aside>
      <main className={view === "viewer" ? "" : "main"}>
        {view === "home" && <Home conversations={conversations} onOpen={openConversation} onChanged={refresh} />}
        {view !== "home" && info && statsError && (
          <div className="page"><div className="error">{statsError}</div></div>
        )}
        {view === "dashboard" && info && !statsError &&
          (stats ? <Dashboard id={info.id} stats={stats} summary={info.summary} onOpenMessage={openMessage} /> : <div className="empty">Carregando…</div>)}
        {view === "viewer" && info && (
          <Viewer id={info.id} summary={info.summary} jumpTo={jumpTo} onJumpHandled={() => setJumpTo(null)} />
        )}
        {view === "settings" && info && <Settings id={info.id} hasMedia={info.summary.hasMedia} onStats={setStats} />}
      </main>
    </div>
  );
}
