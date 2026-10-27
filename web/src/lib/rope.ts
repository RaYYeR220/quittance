/**
 * The certificate border: a hypotrochoid rope unrolled along a straight band,
 * x = aθ − b·sin(θ + φ), y = mid + c·cos(θ + φ), drawn as a repeating SVG tile.
 */

const PERIOD = 40;
const HEIGHT = 14;

const STRANDS = [
  { b: 7, c: 4.6, n: 5, opacity: 0.5, width: 0.5 },
  { b: 7, c: -4.6, n: 5, opacity: 0.5, width: 0.5 },
  { b: -3.5, c: 2, n: 4, opacity: 0.32, width: 0.45 },
];

function tile(vertical: boolean): string {
  const a = PERIOD / (2 * Math.PI);
  const tau = Math.PI * 2;
  let paths = "";
  for (const s of STRANDS) {
    for (let k = 0; k < s.n; k++) {
      const phase = (k * tau) / s.n;
      let d = "";
      for (let i = 0; i <= 240; i++) {
        const t = -tau + (i / 240) * 3 * tau;
        let x = a * t - s.b * Math.sin(t + phase);
        let y = HEIGHT / 2 + s.c * Math.cos(t + phase);
        if (vertical) [x, y] = [y, x];
        d += `${i ? "L" : "M"}${x.toFixed(2)} ${y.toFixed(2)}`;
      }
      paths += `<path d='${d}' fill='none' stroke='%23cfd8cc' stroke-opacity='${s.opacity}' stroke-width='${s.width}'/>`;
    }
  }
  const w = vertical ? HEIGHT : PERIOD;
  const h = vertical ? PERIOD : HEIGHT;
  const rules = vertical
    ? `<path d='M.5 0V${PERIOD}M${HEIGHT - 0.5} 0V${PERIOD}' stroke='%23cfd8cc' stroke-opacity='.5'/>`
    : `<path d='M0 .5H${PERIOD}M0 ${HEIGHT - 0.5}H${PERIOD}' stroke='%23cfd8cc' stroke-opacity='.5'/>`;
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${w}' height='${h}' viewBox='0 0 ${w} ${h}'>${paths}${rules}</svg>`;
  return `url("data:image/svg+xml,${svg.replace(/</g, "%3C").replace(/>/g, "%3E")}")`;
}

/** CSS `background-image` values for the horizontal and vertical runs of the band. */
export const rope = { horizontal: tile(false), vertical: tile(true) };
