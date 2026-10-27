/**
 * Seeded noise and stroke painting for the fields of notes on the landing page.
 * Every note is a short stroke laid along the contours of a domain-warped
 * value-noise field, so the strokes gather into slow currents like engraving.
 */

export type Rng = () => number;

/** mulberry32: small, fast and deterministic. */
export function rng(seed: number): Rng {
  let a = seed | 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const LATTICE = (() => {
  const next = rng(3531904);
  const g = new Float32Array(256 * 256);
  for (let i = 0; i < g.length; i++) g[i] = next();
  return g;
})();

function fade(t: number): number {
  return t * t * (3 - 2 * t);
}

function valueNoise(x: number, y: number): number {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  const u = fade(x - xi);
  const v = fade(y - yi);
  const x0 = xi & 255;
  const y0 = yi & 255;
  const x1 = (x0 + 1) & 255;
  const y1 = (y0 + 1) & 255;
  const a = LATTICE[y0 * 256 + x0];
  const b = LATTICE[y0 * 256 + x1];
  const c = LATTICE[y1 * 256 + x0];
  const d = LATTICE[y1 * 256 + x1];
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
}

function fbm(x: number, y: number): number {
  return 0.55 * valueNoise(x, y) + 0.3 * valueNoise(x * 2.03 + 17, y * 2.03 + 5) + 0.15 * valueNoise(x * 4.11 + 3, y * 4.11 + 29);
}

/** Domain-warped fbm in [0, 1]: the "current" the notes drift in. */
export function warp(u: number, v: number): number {
  const qx = fbm(u * 1.3 + 11, v * 1.3 + 7);
  const qy = fbm(u * 1.3 + 31, v * 1.3 + 3);
  return fbm(u + 1.9 * qx, v + 1.9 * qy);
}

export function smoothstep(edge0: number, edge1: number, t: number): number {
  const x = Math.min(1, Math.max(0, (t - edge0) / (edge1 - edge0)));
  return x * x * (3 - 2 * x);
}

/** Angle of the contour through (u, v): strokes follow the current, not cross it. */
export function contourAngle(u: number, v: number, here: number): number {
  const e = 0.004;
  return Math.atan2(warp(u, v + e) - here, warp(u + e, v) - here) + Math.PI / 2;
}

/**
 * Strokes are grouped into opacity buckets so each bucket is a single path:
 * one `stroke()` per bucket instead of one per note.
 */
export function paint(ctx: CanvasRenderingContext2D, buckets: number[][], rgb: string, alphaFrom: number, alphaTo: number, width: number): void {
  ctx.lineCap = "round";
  ctx.lineWidth = width;
  const steps = Math.max(1, buckets.length - 1);
  buckets.forEach((segments, k) => {
    if (!segments.length) return;
    ctx.strokeStyle = `rgba(${rgb},${(alphaFrom + ((alphaTo - alphaFrom) * k) / steps).toFixed(3)})`;
    ctx.beginPath();
    for (let j = 0; j < segments.length; j += 4) {
      ctx.moveTo(segments[j], segments[j + 1]);
      ctx.lineTo(segments[j + 2], segments[j + 3]);
    }
    ctx.stroke();
  });
}

export const CELADON = "207,216,204";
export const COPPER = "194,122,75";
