import { useEffect, useState } from "react";

type Theme = "light" | "dark";

const KEY = "whatsrecap-theme";
const mq = window.matchMedia("(prefers-color-scheme: dark)");

function stored(): Theme | null {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : null;
  } catch {
    return null;
  }
}

/** Tema atual: a escolha salva ou, sem escolha, o do sistema. A troca vira `data-theme` no <html>. */
export function useTheme() {
  const [choice, setChoice] = useState<Theme | null>(stored);
  const [system, setSystem] = useState<Theme>(mq.matches ? "dark" : "light");

  useEffect(() => {
    const onChange = () => setSystem(mq.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    if (choice) document.documentElement.dataset.theme = choice;
    else delete document.documentElement.dataset.theme;
  }, [choice]);

  const theme = choice ?? system;
  const toggle = () => {
    const next: Theme = theme === "dark" ? "light" : "dark";
    try {
      localStorage.setItem(KEY, next);
    } catch {
      // Sem armazenamento, a escolha vale só para esta sessão.
    }
    setChoice(next);
  };
  return { theme, toggle };
}

/** Cor do autor pelo índice (a cor segue a pessoa, nunca a posição no ranking). */
export const authorVar = (i: number) => `var(--p${i + 1})`;
export const authorSoft = (i: number) => `var(--p${i + 1}s)`;
