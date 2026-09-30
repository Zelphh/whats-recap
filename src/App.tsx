import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { ConversationInfo, Stats } from "./types";
import { Icon, Logo, type ICONS } from "./components/ui";
import { useTheme } from "./theme";
import { Home } from "./views/Home";
import { Dashboard } from "./views/Dashboard";
import { Viewer } from "./views/Viewer";
import { Settings } from "./views/Settings";
import { Chat, type ChatTurn } from "./views/Chat";

type View = "home" | "dashboard" | "viewer" | "settings" | "chat";

export function App() {
  const [conversations, setConversations] = useState<ConversationInfo[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [view, setView] = useState<View>("home");
  const [stats, setStats] = useState<Stats | null>(null);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [jumpTo, setJumpTo] = useState<number | null>(null);
  // Histórico do chat por conversa, mantido enquanto o app está aberto.
  const [chats, setChats] = useState<Record<string, ChatTurn[]>>({});
  const { theme, toggle } = useTheme();

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
  const names = info ? info.summary.authors.join(" & ") : "";

  function openConversation(id: string) {
    setCurrent(id);
    setJumpTo(null);
    setView("dashboard");
  }

  function openMessage(id: number) {
    setJumpTo(id);
    setView("viewer");
  }

  const nav = (v: View, label: string, icon: keyof typeof ICONS) => (
    <button className={`nav-item${view === v ? " active" : ""}`} aria-current={view === v ? "page" : undefined} onClick={() => setView(v)}>
      <Icon name={icon} /><span>{label}</span>
    </button>
  );

  const full = view === "viewer" || view === "chat";

  return (
    <div className="app">
      <aside className="sidebar">
        <nav aria-label="Navegação">
          <div className="brand">
            <Logo />
            <div>
              <div className="brand-name">WhatsRecap</div>
              <div className="brand-sub">local · offline</div>
            </div>
          </div>
          {nav("home", "Importar", "home")}
          {info && (
            <>
              <div className="nav-convo" title={names}>
                <span className="pair" aria-hidden>
                  {info.summary.authors.map((a, i) => <i key={a} style={{ background: `var(--p${i + 1})` }} />)}
                </span>
                <span>{names}</span>
              </div>
              {nav("dashboard", "Dashboard", "dash")}
              {nav("viewer", "Conversa", "viewer")}
              {nav("settings", "Configurações", "settings")}
              <button className="nav-item" disabled>
                <Icon name="llm" /><span>Análises LLM</span><span className="soon">em breve</span>
              </button>
              {nav("chat", "Chat", "chat")}
            </>
          )}
          <div style={{ flex: 1 }} />
          <div className="privacy">
            <Icon name="lock" size={16} stroke={2} />
            <span>Nenhuma mensagem sai deste computador.</span>
          </div>
          <button className={`theme-toggle${theme === "dark" ? " dark" : ""}`} onClick={toggle}
            aria-label={`Tema ${theme === "dark" ? "escuro" : "claro"}; alternar`}>
            <span>{theme === "dark" ? "Escuro" : "Claro"}</span>
            <span className="track"><span className="knob"><Icon name={theme === "dark" ? "moon" : "sun"} size={14} stroke={2.2} /></span></span>
          </button>
        </nav>
      </aside>
      <main className={`main${full ? " full" : ""}${view === "home" ? " bare" : ""}`}>
        {view === "home" && <Home conversations={conversations} onOpen={openConversation} onChanged={refresh} />}
        {view !== "home" && info && statsError && (
          <div className="page"><div className="error-box" role="alert">{statsError}</div></div>
        )}
        {view === "dashboard" && info && !statsError &&
          (stats
            ? <Dashboard key={info.id} id={info.id} stats={stats} summary={info.summary} onOpenMessage={openMessage} />
            : <div className="loading">Carregando…</div>)}
        {view === "viewer" && info && (
          <Viewer id={info.id} summary={info.summary} jumpTo={jumpTo} onJumpHandled={() => setJumpTo(null)} />
        )}
        {view === "chat" && info && (
          <Chat
            id={info.id}
            summary={info.summary}
            turns={chats[info.id] ?? []}
            setTurns={(update) => setChats((c) => ({ ...c, [info.id]: update(c[info.id] ?? []) }))}
            onOpenMessage={openMessage}
          />
        )}
        {view === "settings" && info && <Settings id={info.id} names={names} hasMedia={info.summary.hasMedia} onStats={setStats} />}
      </main>
    </div>
  );
}
