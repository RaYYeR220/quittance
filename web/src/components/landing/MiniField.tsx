"use client";

import { useEffect, useRef } from "react";
import { CELADON, COPPER, contourAngle, paint, rng, smoothstep, warp } from "@/lib/note-field";

/**
 * A small static field of notes.
 * - `all`: a viewing key, every note lit.
 * - `none`: a txid, every note dark, one merely pointed at.
 * - `one`: a receipt, one note lit.
 * - `void`: the dark ground behind what a receipt leaves out.
 */
export type MiniMode = "all" | "none" | "one" | "void";

function draw(cv: HTMLCanvasElement, mode: MiniMode, seed: number) {
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (!w || !h) return;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  cv.width = Math.round(w * dpr);
  cv.height = Math.round(h * dpr);
  const c = cv.getContext("2d");
  if (!c) return;
  c.setTransform(dpr, 0, 0, dpr, 0, 0);
  c.clearRect(0, 0, w, h);

  const next = rng(901 + seed * 37);
  const scale = 1 / 150;
  const ox = seed * 7.3;
  const oy = seed * 3.1;
  const count = Math.round((w * h) / (mode === "void" ? 60 : 30));
  const nx = w * 0.63;
  const ny = h * 0.46;
  const buckets: number[][] = Array.from({ length: 5 }, () => []);
  let placed = 0;
  for (let tries = 0; placed < count && tries < count * 30; tries++) {
    const x = next() * w;
    const y = next() * h;
    const u = x * scale + ox;
    const v = y * scale + oy;
    const here = warp(u, v);
    let density = mode === "all" ? 0.3 + 0.7 * smoothstep(0.44, 0.64, here) : 0.06 + 0.94 * smoothstep(0.44, 0.64, here);
    if (mode === "one" || mode === "none") {
      const dx = x - nx;
      const dy = y - ny;
      density *= 1 - Math.exp(-(dx * dx + dy * dy) / (30 * 30));
    }
    if (next() > density) continue;
    placed++;
    const angle = contourAngle(u, v, here) + (next() - 0.5) * 0.3;
    const len = 2 + next() * 3.6;
    const hx = (Math.cos(angle) * len) / 2;
    const hy = (Math.sin(angle) * len) / 2;
    const tone = next();
    buckets[Math.min(4, Math.floor(tone * tone * 5))].push(x - hx, y - hy, x + hx, y + hy);
  }

  if (mode === "all") paint(c, buckets, COPPER, 0.32, 0.9, 0.9);
  else if (mode === "void") paint(c, buckets, CELADON, 0.03, 0.12, 0.8);
  else paint(c, buckets, CELADON, 0.07, 0.32, 0.85);

  if (mode === "one") {
    const g = c.createRadialGradient(nx, ny, 0, nx, ny, 46);
    g.addColorStop(0, `rgba(${COPPER},.3)`);
    g.addColorStop(1, `rgba(${COPPER},0)`);
    c.fillStyle = g;
    c.beginPath();
    c.arc(nx, ny, 46, 0, Math.PI * 2);
    c.fill();
    c.strokeStyle = `rgba(${COPPER},.9)`;
    c.lineWidth = 1;
    c.beginPath();
    c.arc(nx, ny, 10, 0, Math.PI * 2);
    c.stroke();
    c.fillStyle = "#d99466";
    c.beginPath();
    c.arc(nx, ny, 3.2, 0, Math.PI * 2);
    c.fill();
  }
  if (mode === "none") {
    c.strokeStyle = `rgba(${CELADON},.75)`;
    c.lineWidth = 1;
    c.setLineDash([2, 3]);
    c.beginPath();
    c.arc(nx, ny, 10, 0, Math.PI * 2);
    c.stroke();
    c.setLineDash([]);
    c.strokeStyle = `rgba(${CELADON},.45)`;
    c.beginPath();
    c.moveTo(nx + 7.5, ny - 7.5);
    c.lineTo(nx + 34, ny - 34);
    c.lineTo(Math.min(w - 4, nx + 90), ny - 34);
    c.stroke();
    c.fillStyle = `rgba(${CELADON},.35)`;
    c.beginPath();
    c.arc(nx, ny, 2.6, 0, Math.PI * 2);
    c.fill();
  }
}

export function MiniField({ mode, seed, className }: { mode: MiniMode; seed: number; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    draw(cv, mode, seed);
    let width = window.innerWidth;
    let timer = 0;
    const onResize = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (window.innerWidth === width) return;
        width = window.innerWidth;
        draw(cv, mode, seed);
      }, 160);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("resize", onResize);
    };
  }, [mode, seed]);
  return <canvas ref={ref} className={className} aria-hidden="true" />;
}
