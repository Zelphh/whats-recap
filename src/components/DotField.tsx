import { useEffect, useRef } from "react";

// Campo de pontos interativo da tela inicial: os pontos fogem do cursor, esquentam de cor
// perto dele e o clique solta uma onda. Pensado para não pesar no WebKitGTK:
// - só redesenha enquanto há interação (cursor se mexendo, suavização assentando ou onda
//   ativa); parado, o custo é zero;
// - cobre só a área visível (fica "sticky" no topo do scroll), em resolução 1×;
// - a base de cada ponto é pré-calculada; por quadro, os pontos são agrupados por cor e
//   opacidade em poucos Path2D, com um fill por grupo;
// - com "reduzir movimento", desenha um quadro estático e ignora o cursor.

const GAP = 28;
const RADIUS = 230; // alcance do cursor, em px
const PUSH = 34;
const LEVELS = 10; // degraus de opacidade usados para agrupar os pontos
const RIPPLE_MS = 1400;

interface Ripple { x: number; y: number; t0: number }

function readColors() {
  const s = getComputedStyle(document.documentElement);
  return { dot: s.getPropertyValue("--field-dot").trim(), hot: s.getPropertyValue("--field-hot").trim() };
}

export function DotField() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current!;
    const ctx = canvas.getContext("2d")!;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let colors = readColors();
    let w = 0, h = 0;
    // Base de cada ponto: posição já deslocada pelo "ruído", tamanho e opacidade.
    let bx = new Float32Array(0), by = new Float32Array(0), bs = new Float32Array(0), ba = new Float32Array(0);
    let mx = -1e4, my = -1e4, sx = -1e4, sy = -1e4; // cursor e posição suavizada
    let presence = 0, target = 0; // o quanto o cursor influencia (some aos poucos ao sair)
    const ripples: Ripple[] = [];
    let raf = 0;

    const build = () => {
      const cols = Math.ceil(w / GAP), rows = Math.ceil(h / GAP), n = cols * rows;
      bx = new Float32Array(n); by = new Float32Array(n); bs = new Float32Array(n); ba = new Float32Array(n);
      let k = 0;
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++, k++) {
          const x = GAP / 2 + c * GAP, y = GAP / 2 + r * GAP;
          const q = (Math.sin(x * 0.011) + Math.sin(y * 0.016) + Math.sin((x + y) * 0.007) + 3) / 6; // 0–1
          const ang = (q * 6 - 3) * 1.4;
          bx[k] = x + Math.cos(ang) * 5;
          by[k] = y + Math.sin(ang) * 5;
          bs[k] = 0.7 + q * 1.5;
          ba[k] = 0.16 + q * 0.32;
        }
      }
    };

    /** Desenha um quadro; devolve true se ainda há movimento a animar. */
    const draw = (now: number) => {
      sx += (mx - sx) * 0.12;
      sy += (my - sy) * 0.12;
      presence += (target - presence) * 0.1;
      for (let i = ripples.length - 1; i >= 0; i--) if (now - ripples[i].t0 > RIPPLE_MS) ripples.splice(i, 1);

      const paths: (Path2D | undefined)[] = new Array(LEVELS * 2);
      const reach = Math.max(w, h) * 0.7;
      for (let i = 0; i < bx.length; i++) {
        let x = bx[i], y = by[i], hot = 0, grow = 0;
        const dx = x - sx, dy = y - sy;
        if (presence > 0.01 && dx < RADIUS && dx > -RADIUS && dy < RADIUS && dy > -RADIUS) {
          const d = Math.hypot(dx, dy) || 1;
          const inf = Math.max(0, 1 - d / RADIUS) * presence;
          if (inf > 0) {
            x += (dx / d) * inf * inf * PUSH;
            y += (dy / d) * inf * inf * PUSH;
            hot = inf * 1.6;
            grow = inf * 2.6;
          }
        }
        for (const r of ripples) {
          const t = (now - r.t0) / RIPPLE_MS;
          const rx = bx[i] - r.x, ry = by[i] - r.y, rd = Math.hypot(rx, ry) || 1, R = t * reach;
          const k = Math.exp(-((rd - R) ** 2) / 1800) * (1 - t);
          if (k < 0.01) continue;
          x += (rx / rd) * k * 16;
          y += (ry / rd) * k * 16;
          hot += k;
          grow += k * 2.5;
        }
        hot = Math.min(1, hot);
        const a = Math.min(0.95, ba[i] + hot * 0.6);
        const key = (hot > 0.08 ? LEVELS : 0) + Math.min(LEVELS - 1, Math.floor(a * LEVELS));
        const p = (paths[key] ??= new Path2D());
        const s = bs[i] + grow;
        p.moveTo(x + s, y);
        p.arc(x, y, s, 0, 6.2832);
      }

      ctx.clearRect(0, 0, w, h);
      for (let key = 0; key < paths.length; key++) {
        const p = paths[key];
        if (!p) continue;
        ctx.globalAlpha = ((key % LEVELS) + 0.5) / LEVELS;
        ctx.fillStyle = key >= LEVELS ? colors.hot : colors.dot;
        ctx.fill(p);
      }
      ctx.globalAlpha = 1;

      return ripples.length > 0 || Math.abs(presence - target) > 0.005 ||
        (presence > 0.01 && (Math.abs(mx - sx) > 0.5 || Math.abs(my - sy) > 0.5));
    };

    const loop = (now: number) => {
      raf = draw(now) ? requestAnimationFrame(loop) : 0;
    };
    const wake = () => {
      if (!raf) raf = requestAnimationFrame(loop);
    };

    const resize = () => {
      const r = canvas.getBoundingClientRect();
      w = Math.round(r.width);
      h = Math.round(r.height);
      canvas.width = w;
      canvas.height = h;
      build();
      draw(performance.now());
    };
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const onMove = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      const x = e.clientX - r.left, y = e.clientY - r.top;
      const inside = x >= 0 && y >= 0 && x <= w && y <= h;
      if (inside) {
        if (presence < 0.01) { sx = x; sy = y; } // entrou agora: sem "arrastar" desde longe
        mx = x;
        my = y;
      }
      target = inside ? 1 : 0;
      wake();
    };
    const onDown = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      const x = e.clientX - r.left, y = e.clientY - r.top;
      if (x < 0 || y < 0 || x > w || y > h) return;
      ripples.push({ x, y, t0: performance.now() });
      wake();
    };
    const onLeave = () => {
      target = 0;
      wake();
    };
    if (!reduce) {
      window.addEventListener("pointermove", onMove, { passive: true });
      window.addEventListener("pointerdown", onDown, { passive: true });
      document.documentElement.addEventListener("pointerleave", onLeave);
    }

    // Troca de tema (escolha manual ou do sistema): relê as cores e redesenha.
    const recolor = () => {
      colors = readColors();
      draw(performance.now());
    };
    const mo = new MutationObserver(recolor);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", recolor);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      mo.disconnect();
      mq.removeEventListener("change", recolor);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerdown", onDown);
      document.documentElement.removeEventListener("pointerleave", onLeave);
    };
  }, []);

  return (
    <div className="field" aria-hidden>
      <canvas ref={ref} />
    </div>
  );
}
