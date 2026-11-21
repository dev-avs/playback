// golden/cubic_spline.json: node_modules/cubic-spline (NaturalSpline) over the
// keyframes lyricAnimator.ts uses plus a few irregular ones, sampled at many x
// including outside the knot range. Numbers are strings (String(n), "-0" for
// negative zero) so they round-trip exactly.
import NaturalSpline from "cubic-spline";
import type { Fixtures } from "../run.mts";

const enc = (n: number) => (Object.is(n, -0) ? "-0" : String(n));

const curves: Record<string, [number, number][]> = {
  WORD_SCALE: [[0, 0.95], [0.7, 1.0505], [1, 1]],
  LETTER_SCALE: [[0, 0.95], [0.7, 1.175], [1, 1]],
  WORD_Y: [[0, 1 / 100], [0.9, -(1 / 60)], [1, 0]],
  LETTER_Y: [[0, 1 / 100], [0.9, -(1 / 56)], [1, 0]],
  GLOW: [[0, 0], [0.15, 1], [0.6, 1], [1, 0]],
  DOT_SCALE: [[0, 0.75], [0.7, 1.05], [1, 1]],
  DOT_Y: [[0, 0], [0.9, -0.12], [1, 0]],
  DOT_GLOW: [[0, 0], [0.6, 1], [1, 1]],
  DOT_OPACITY: [[0, 0.35], [0.6, 1], [1, 1]],
  irregular5: [[-2, 3.5], [-0.25, -1], [0.1, 0.1], [1.3, 7], [4, 2]],
  two_points: [[0, 1], [2, 5]],
  flat: [[0, 2], [1, 2], [3, 2], [4, 2]],
};

export default (_fx: Fixtures) => {
  const out: unknown[] = [];
  for (const [name, pts] of Object.entries(curves)) {
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const sp = new NaturalSpline(xs, ys);
    const lo = xs[0], hi = xs[xs.length - 1];
    const sample: number[] = [];
    // dense grid across and beyond the knots
    for (let i = -60; i <= 160; i++) sample.push(lo + ((hi - lo) * i) / 100);
    // the knots, just below / above each, and a few far outside
    for (const x of xs) sample.push(x, x - 1e-9, x + 1e-9, x - 1e-15, x + 1e-15);
    sample.push(-1e3, -7.25, 1e3, 12.5, 0.123456789, 0.987654321, -0);
    out.push({
      name,
      xs: xs.map(enc),
      ys: ys.map(enc),
      ks: Array.from(sp.ks as Float64Array).map(enc),
      at: sample.map((x) => [enc(x), enc(sp.at(x))]),
    });
  }
  return out;
};
