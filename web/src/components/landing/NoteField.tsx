"use client";

import { useEffect, useRef, useState } from "react";
import { CELADON, contourAngle, paint, rng, smoothstep, warp } from "@/lib/note-field";
import styles from "./NoteField.module.css";

export interface LitNote {
  amount: string;
  to: string;
  memo: string;
  height: string;
  verifier: string;
  context: string;
}

/** Canvases are this much larger than the stage on every side, so the drift never shows an edge. */
const MARGIN = 28;
/** One pulse per Zcash block. */
const BLOCK_MS = 75_000;

type Box = { x0: number; y0: number; x1: number; y1: number };
type Zone = Box & { feather: number; floor: number };

function grow(b: Box, by: number): Box {
  return { x0: b.x0 - by, y0: b.y0 - by, x1: b.x1 + by, y1: b.y1 + by };
}

/**
 * The hero's field: tens of thousands of dim notes drawn once per size and
 * drifted by the compositor, with one note lit and annotated. It sits inside
 * the certificate and keeps clear of the claim (`[data-ink]`) and the serials
 * (`[data-serial]`) that the page lays over it.
 */
export function NoteField({ note }: { note: LitNote }) {
  const stageRef = useRef<HTMLDivElement>(null);
  const farRef = useRef<HTMLCanvasElement>(null);
  const nearRef = useRef<HTMLCanvasElement>(null);
  const litRef = useRef<HTMLDivElement>(null);
  const tagRef = useRef<HTMLDivElement>(null);
  const pulseRef = useRef<HTMLSpanElement>(null);
  const [others, setOthers] = useState(35_999);

  useEffect(() => {
    const stage = stageRef.current;
    const far = farRef.current;
    const near = nearRef.current;
    const lit = litRef.current;
    const tag = tagRef.current;
    const pulse = pulseRef.current;
    const cert = stage?.parentElement;
    if (!stage || !far || !near || !lit || !tag || !pulse || !cert) return;

    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const wideQuery = window.matchMedia("(min-width: 1181px)");
    let disposed = false;
    let drawnW = 0;
    let drawnH = 0;

    function relative(r: DOMRect, base: DOMRect): Box {
      return { x0: r.left - base.left, y0: r.top - base.top, x1: r.right - base.left, y1: r.bottom - base.top };
    }

    /** Places the lit note beside (or above) the claim and returns the areas the field must keep clear. */
    function layout(W: number, H: number) {
      const base = stage!.getBoundingClientRect();
      const wide = wideQuery.matches;
      // The claim's real ink: line boxes of its text and its buttons, not the block boxes.
      const range = document.createRange();
      let ink: Box | null = null;
      for (const el of cert!.querySelectorAll<HTMLElement>("[data-ink]")) {
        range.selectNodeContents(el);
        const b = relative(el.tagName === "A" ? el.getBoundingClientRect() : range.getBoundingClientRect(), base);
        ink = ink ? { x0: Math.min(ink.x0, b.x0), y0: Math.min(ink.y0, b.y0), x1: Math.max(ink.x1, b.x1), y1: Math.max(ink.y1, b.y1) } : b;
      }
      if (!ink) ink = { x0: 0, y0: 0, x1: 0, y1: 0 };

      const at = wide
        ? { x: Math.round(Math.min(W - 350, Math.max(ink.x1 + 130, W * 0.7))), y: Math.round(Math.max(96, Math.min(H * 0.27, ink.y0 + 20))), side: "right" }
        : { x: Math.round(W - 30), y: W < 600 ? 54 : 70, side: "left" };
      lit!.style.left = `${at.x}px`;
      lit!.style.top = `${at.y}px`;
      lit!.dataset.side = at.side;

      // The tag may still be mid-entrance; measure where it will come to rest.
      const transform = getComputedStyle(tag!).transform;
      const shift = transform && transform !== "none" ? new DOMMatrixReadOnly(transform).m42 : 0;
      const t = relative(tag!.getBoundingClientRect(), base);
      t.y0 -= shift;
      t.y1 -= shift;

      const zones: Zone[] = [
        { ...grow(ink, wide ? 28 : 12), feather: wide ? 96 : 40, floor: 0.1 },
        { ...grow(t, 16), feather: 30, floor: 0.06 },
      ];
      cert!.querySelectorAll<HTMLElement>("[data-serial]").forEach((el) => {
        zones.push({ ...grow(relative(el.getBoundingClientRect(), base), 8), feather: 30, floor: 0 });
      });
      return { x: at.x, y: at.y, zones };
    }

    function render() {
      const W = stage!.clientWidth;
      const H = stage!.clientHeight;
      if (!W || !H) return;
      drawnW = W;
      drawnH = H;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const small = W < 700;
      const { x: nx, y: ny, zones } = layout(W, H);
      const count = Math.round(Math.min(40_000, Math.max(9_000, ((small ? 44_000 : 35_000) * W * H) / 1e6)));

      const FW = W + 2 * MARGIN;
      const FH = H + 2 * MARGIN;
      // The far layer stays at 1x so it reads softer; the near layer is sharp.
      far!.width = FW;
      far!.height = FH;
      near!.width = Math.round(FW * dpr);
      near!.height = Math.round(FH * dpr);
      for (const c of [far!, near!]) {
        c.style.width = `${FW}px`;
        c.style.height = `${FH}px`;
      }
      const fc = far!.getContext("2d");
      const nc = near!.getContext("2d");
      if (!fc || !nc) return;
      fc.setTransform(1, 0, 0, 1, MARGIN, MARGIN);
      nc.setTransform(dpr, 0, 0, dpr, MARGIN * dpr, MARGIN * dpr);
      fc.clearRect(-MARGIN, -MARGIN, FW, FH);
      nc.clearRect(-MARGIN, -MARGIN, FW, FH);

      const BUCKETS = 6;
      const farB: number[][] = Array.from({ length: BUCKETS }, () => []);
      const nearB: number[][] = Array.from({ length: BUCKETS }, () => []);
      const glints: number[] = [];
      const next = rng(118);
      const scale = small ? 1 / 230 : 1 / 310;
      let placed = 0;
      for (let tries = 0; placed < count && tries < count * 30; tries++) {
        const x = next() * FW - MARGIN;
        const y = next() * FH - MARGIN;
        const u = x * scale;
        const v = y * scale;
        const dx = x - nx;
        const dy = y - ny;
        const dd = dx * dx + dy * dy;
        // A current runs through the lit note, and a dark pocket opens around it.
        const here = warp(u, v);
        const current = here + (small ? 0.2 : 0.27) * Math.exp(-dd / (small ? 150 * 150 : 290 * 290));
        let density = 0.05 + 0.95 * smoothstep(0.44, 0.64, current);
        for (const z of zones) {
          const ex = Math.max(z.x0 - x, 0, x - z.x1);
          const ey = Math.max(z.y0 - y, 0, y - z.y1);
          if (ex < z.feather && ey < z.feather) density *= z.floor + (1 - z.floor) * smoothstep(0, z.feather, Math.hypot(ex, ey));
        }
        density *= 1 - Math.exp(-dd / (58 * 58));
        if (next() > density) continue;
        placed++;

        const angle = contourAngle(u, v, here) + (next() - 0.5) * 0.3;
        const isNear = next() < 0.44;
        const tone = next();
        const len = (isNear ? 2.8 + next() * 4.2 : 1.3 + next() * 2.2) * (small ? 0.8 : 1);
        const hx = (Math.cos(angle) * len) / 2;
        const hy = (Math.sin(angle) * len) / 2;
        if (isNear && tone > 0.985) {
          glints.push(x - hx, y - hy, x + hx, y + hy);
          continue;
        }
        const bucket = Math.min(BUCKETS - 1, Math.floor(tone * tone * BUCKETS));
        (isNear ? nearB : farB)[bucket].push(x - hx, y - hy, x + hx, y + hy);
      }
      paint(fc, farB, CELADON, 0.07, 0.27, 1);
      paint(nc, nearB, CELADON, 0.13, 0.5, 0.85);
      paint(nc, [glints], CELADON, 0.72, 0.72, 0.9);
      setOthers(placed - 1);
    }

    function start() {
      if (disposed) return;
      render();
      cert!.dataset.go = "";
    }
    (document.fonts?.ready ?? Promise.resolve()).then(start);
    // If fonts never settle, still start the entrance; the CSS has its own failsafe too.
    const startAnyway = window.setTimeout(() => {
      if (!disposed && !("go" in cert.dataset)) start();
    }, 1100);

    let resizeTimer = 0;
    const onResize = () => {
      window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => {
        // Phones resize on every toolbar show/hide; only redraw on a real change.
        if (Math.abs(stage.clientWidth - drawnW) > 2 || Math.abs(stage.clientHeight - drawnH) > 120) render();
      }, 160);
    };
    window.addEventListener("resize", onResize);

    let hidden = document.hidden;
    let offscreen = false;
    const syncPaused = () => {
      if (hidden || offscreen) stage.dataset.paused = "";
      else delete stage.dataset.paused;
    };
    const onVisibility = () => {
      hidden = document.hidden;
      syncPaused();
    };
    document.addEventListener("visibilitychange", onVisibility);
    const io = new IntersectionObserver(([entry]) => {
      offscreen = !entry.isIntersecting;
      syncPaused();
    });
    io.observe(cert);

    const beat = () => {
      if (reduce || hidden || offscreen) return;
      pulse.classList.remove(styles.pulsing);
      void pulse.offsetWidth;
      pulse.classList.add(styles.pulsing);
    };
    const firstBeat = reduce ? 0 : window.setTimeout(beat, 1500);
    const everyBlock = reduce ? 0 : window.setInterval(beat, BLOCK_MS);

    return () => {
      disposed = true;
      window.clearTimeout(startAnyway);
      window.clearTimeout(resizeTimer);
      window.clearTimeout(firstBeat);
      window.clearInterval(everyBlock);
      window.removeEventListener("resize", onResize);
      document.removeEventListener("visibilitychange", onVisibility);
      io.disconnect();
    };
  }, []);

  const label = `One note out of the field, lit: ${note.amount} to ${note.to}, memo ${note.memo}, mined at height ${note.height}, signed for ${note.verifier} · ${note.context}. Every other note stays dark.`;

  return (
    <div className={styles.stage} ref={stageRef}>
      <div className={styles.field} aria-hidden="true">
        <canvas className={styles.far} ref={farRef} />
        <canvas className={styles.near} ref={nearRef} />
      </div>
      <div className={styles.lit} ref={litRef} role="img" aria-label={label}>
        <span className={styles.halo} />
        <span className={styles.pulse} ref={pulseRef} />
        <span className={styles.ring} />
        <span className={styles.core} />
        <div className={styles.tag} ref={tagRef} aria-hidden="true">
          <dl>
            <dt>Amount</dt>
            <dd className={styles.amount}>{note.amount}</dd>
            <dt>To</dt>
            <dd>to {note.to}</dd>
            <dt>Memo</dt>
            <dd>“{note.memo}”</dd>
            <dt>Height</dt>
            <dd>mined at {note.height}</dd>
            <dt>Challenge</dt>
            <dd>signed for {note.verifier}</dd>
            <dd>{note.context}</dd>
          </dl>
          <p className={styles.dark}>
            <b>{others.toLocaleString("en-US")}</b> other notes stay dark
          </p>
        </div>
      </div>
    </div>
  );
}
