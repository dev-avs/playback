/**
 * Per-syllable and per-letter lyric animation, ported from spicy-lyrics'
 * LyricsAnimator (src/utils/Lyrics/Animator/Lyrics/LyricsAnimator.ts).
 *
 * The model has three tiers, and getting them separate is the whole point:
 *
 *   word   — every syllable is a box that scales and lifts. Peak scale 1.0505.
 *   letter — only syllables held long enough are split into letters. Those
 *            letters scale/lift *on top of* their word's own motion, with a
 *            much punchier peak (1.175) and double the lift.
 *   dot    — interlude markers; their own curves, plus opacity.
 *
 * The wave inside a word is NOT one clock per letter. Exactly one letter is
 * Active at a time; every other letter in the word reads *that* letter's
 * progress attenuated by how far away it sits. So what travels across the word
 * is a single bump, not N independent bumps that happen to be offset.
 *
 * Everything is written to CSS custom properties. React never re-renders per
 * frame; the DOM is the only mutable state.
 */

// ── cubic hermite spline (monotone) ──
// spicy uses `cubic-spline` (natural cubic), which overshoots between control
// points. Monotone Hermite matches the same keyframes without ever punching
// past them, which matters for scale (never above 1.175) and glow (never
// negative). On the flat GLOW plateau (0.15..0.6) both agree exactly.
type Point = readonly [number, number];

class Spline {
  private xs: number[];
  private ys: number[];
  private ms: number[];

  constructor(points: readonly Point[]) {
    const n = points.length;
    this.xs = points.map((p) => p[0]);
    this.ys = points.map((p) => p[1]);

    // finite differences
    const d: number[] = new Array(n - 1);
    for (let i = 0; i < n - 1; i++) {
      d[i] = (this.ys[i + 1] - this.ys[i]) / (this.xs[i + 1] - this.xs[i]);
    }

    // tangents
    this.ms = new Array(n);
    this.ms[0] = d[0];
    this.ms[n - 1] = d[n - 2];
    for (let i = 1; i < n - 1; i++) {
      this.ms[i] = d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2;
    }

    // monotone correction (Fritsch–Carlson)
    for (let i = 0; i < n - 1; i++) {
      if (d[i] === 0) {
        this.ms[i] = 0;
        this.ms[i + 1] = 0;
      } else {
        const a = this.ms[i] / d[i];
        const b = this.ms[i + 1] / d[i];
        const h = a * a + b * b;
        if (h > 9) {
          const t = 3 / Math.sqrt(h);
          this.ms[i] = t * a * d[i];
          this.ms[i + 1] = t * b * d[i];
        }
      }
    }
  }

  at(x: number): number {
    const xs = this.xs;
    if (x <= xs[0]) return this.ys[0];
    const last = xs.length - 1;
    if (x >= xs[last]) return this.ys[last];

    let i = 0;
    for (; i < last; i++) if (x < xs[i + 1]) break;

    const h = xs[i + 1] - xs[i];
    const t = (x - xs[i]) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    const h00 = 2 * t3 - 3 * t2 + 1;
    const h10 = t3 - 2 * t2 + t;
    const h01 = -2 * t3 + 3 * t2;
    const h11 = t3 - t2;
    return (
      h00 * this.ys[i] +
      h10 * h * this.ms[i] +
      h01 * this.ys[i + 1] +
      h11 * h * this.ms[i + 1]
    );
  }
}

// ── curves (LyricsAnimator.ts:39-101, 115-150) ──
const WORD_SCALE = new Spline([
  [0, 0.95],
  [0.7, 1.0505],
  [1, 1],
]);
const LETTER_SCALE = new Spline([
  [0, 0.95],
  [0.7, 1.175],
  [1, 1],
]);
const WORD_Y = new Spline([
  [0, 1 / 100],
  [0.9, -(1 / 60)],
  [1, 0],
]);
const LETTER_Y = new Spline([
  [0, 1 / 100],
  [0.9, -(1 / 56)],
  [1, 0],
]);
const GLOW = new Spline([
  [0, 0],
  [0.15, 1],
  [0.6, 1],
  [1, 0],
]);

const DOT_SCALE = new Spline([
  [0, 0.75],
  [0.7, 1.05],
  [1, 1],
]);
const DOT_Y = new Spline([
  [0, 0],
  [0.9, -0.12],
  [1, 0],
]);
const DOT_GLOW = new Spline([
  [0, 0],
  [0.6, 1],
  [1, 1],
]);
const DOT_OPACITY = new Spline([
  [0, 0.35],
  [0.6, 1],
  [1, 1],
]);

// Spring tuning — frequency in Hz, damping ratio in [0,1).
const SCALE_FREQ = 0.88, SCALE_DAMP = 0.64;
const Y_FREQ = 1.45, Y_DAMP = 0.4;
const GLOW_FREQ = 1.18, GLOW_DAMP = 0.56;

const DOT_SCALE_FREQ = 0.7, DOT_SCALE_DAMP = 0.6;
const DOT_Y_FREQ = 1.25, DOT_Y_DAMP = 0.4;
const DOT_GLOW_FREQ = 1, DOT_GLOW_DAMP = 0.5;
const DOT_OPACITY_FREQ = 1, DOT_OPACITY_DAMP = 0.5;

/**
 * Letters already sung inside a still-Active word keep a residual glow rather
 * than going dark, so a held note stays lit behind the travelling bump.
 * Only reached when no letter is Active (the boundary between two letters).
 */
const SUNG_LETTER_GLOW = 0.2;

/**
 * Glow → CSS. spicy writes pixel blur radii against a ~44px lyric font; our
 * panel runs at ~17px, so the same *proportions* are expressed in em and CSS
 * multiplies them back up by the element's own font size. Ratios preserved
 * exactly: letter 4+12px, word 4+2px, dot 4+6px at 44px.
 */
const GLOW_BLUR_BASE = 0.09; // em
const LETTER_BLUR_RANGE = 0.27; // em
const WORD_BLUR_RANGE = 0.045; // em
const DOT_BLUR_RANGE = 0.136; // em

// Alpha multipliers, straight from the reference (185% / 35% / 90%).
// The letter figure is deliberately over 1 — it saturates around g=0.54 and
// holds, which is what makes a held syllable read as *blown out* rather than
// as a brief flicker.
const LETTER_GLOW_ALPHA = 1.85;
const WORD_GLOW_ALPHA = 0.35;
const DOT_GLOW_ALPHA = 0.9;

/** Gradient sweep endpoints, in % (Mixed.css `--gradient-position`). */
const GRAD_UNSUNG = -20;
const GRAD_SUNG = 100;
const GRAD_SPAN = 120;

/**
 * Letter windows stop short of the syllable's real end (spicy's Emphasize
 * `Substractions.EndTime = 250`), so the wave locks to idle before the next
 * syllable rises. The group shares the shortened window with its letters.
 *
 * Taken as a *fraction* rather than the reference's flat 250 ms. That constant
 * only ever met syllables held 1 s or longer, where it is a quarter of the
 * window; applied to a 400 ms word it would swallow two thirds of it and the
 * wave would be over before it read. At 1000 ms the two rules agree exactly.
 */
const LETTER_TAIL_S = 0.25;
const LETTER_TAIL_FRACTION = 0.25;

/** Below this a letter would hold the bump for barely a frame — not worth it. */
const MIN_LETTER_MS = 30;

/**
 * Per-letter window the spring tuning assumes, derived from the reference's own
 * numbers: a 1 s syllable, less the 250 ms tail, over ~6 letters ≈ 125 ms.
 */
const WAVE_REF_LETTER_MS = 125;
const MAX_FREQ_SCALE = 3;

/**
 * How much faster this letter's springs need to run to keep the wave's shape.
 *
 * The reference only ever waved syllables held 1 s or longer, so its
 * frequencies are tuned for a leisurely bump. Run the same 0.88 Hz spring
 * against a 40 ms letter window and the goal has come and gone long before the
 * spring has meaningfully moved — the wave survives as a faint shimmer, which
 * is precisely the mush the letter wave used to produce.
 *
 * Scaling frequency with the window preserves the *shape* of the motion at any
 * speed. Clamped at 1 so nothing held long ever deviates from spicy's tuning,
 * and at 3 so a very fast syllable stays a wave rather than becoming a twitch.
 */
function letterFreqScale(windowMs: number): number {
  if (!(windowMs > 0)) return 1;
  return Math.min(MAX_FREQ_SCALE, Math.max(1, WAVE_REF_LETTER_MS / windowMs));
}

/**
 * Letter-wave windows for a syllable, or null if it should stay a single unit.
 *
 * All the timing policy for the wave lives here so the renderer only has to ask
 * "does this syllable wave, and where do its letters sit?".
 */
export function letterWindows(
  start: number,
  end: number,
  count: number,
): { groupEnd: number; per: number } | null {
  const dur = end - start;
  if (count < 2 || dur <= 0) return null;

  const groupEnd = end - Math.min(LETTER_TAIL_S, dur * LETTER_TAIL_FRACTION);
  const per = (groupEnd - start) / count;
  if (per * 1000 < MIN_LETTER_MS) return null;

  return { groupEnd, per };
}

// Falloff from the Active letter. Motion is tight (only the neighbour really
// moves); light bleeds much wider. Two different jobs, two different curves.
const posFalloff = (distance: number) => 1 / (1 + Math.pow(distance, 2.8));
const glowFalloff = (distance: number) => 1 / (1 + distance * 0.9);

// d3-ease easeSinOut
const easeSinOut = (t: number) => Math.sin((t * Math.PI) / 2);

// ── spring (Fraktality/spr.lua, underdamped branch) ──
class Spring {
  x: number;
  v: number;
  g: number;
  readonly start: number;
  private readonly f: number;
  private readonly d: number;

  constructor(start: number, freqHz: number, damping: number) {
    this.x = start;
    this.v = 0;
    this.g = start;
    this.start = start;
    this.f = freqHz * 2 * Math.PI;
    this.d = damping;
  }

  setGoal(g: number) {
    this.g = g;
  }

  reset() {
    this.x = this.start;
    this.v = 0;
    this.g = this.start;
  }

  /**
   * Advance one step; returns the new position.
   *
   * Deliberately has no sleep-and-snap shortcut: the reference integrates every
   * frame, and snapping to goal drops the tail of the settle, which is exactly
   * the part that reads as weight.
   */
  step(dt: number): number {
    const dx = this.x - this.g;
    const f = this.f;
    const d = this.d;
    const q = Math.exp(-d * f * dt);
    const c = Math.sqrt(1 - d * d);
    const i = Math.cos(dt * f * c);
    const j = Math.sin(dt * f * c);
    const z = j / c;
    const y = j / (f * c);
    this.x = (dx * (i + z * d) + this.v * y) * q + this.g;
    this.v = (this.v * (i - z * d) - dx * (z * f)) * q;
    return this.x;
  }
}

// ── model ──
export type UnitState = "NotSung" | "Active" | "Sung";

/** Last values written to the DOM, so unchanged frames skip the write. */
interface Written {
  s: number;
  y: number;
  gb: number;
  go: number;
  gp: number;
  o: number;
}

const blankWritten = (): Written => ({
  s: NaN,
  y: NaN,
  gb: NaN,
  go: NaN,
  gp: NaN,
  o: NaN,
});

export interface LetterUnit {
  el: HTMLElement;
  s: number;
  e: number;
  scale: Spring;
  y: Spring;
  glow: Spring;
  w: Written;
}

export interface WordUnit {
  el: HTMLElement;
  s: number;
  e: number;
  kind: "word" | "group" | "dot";
  /**
   * Fill-only unit: the source has no real word sync, so the line is swept as a
   * progress indicator and nothing scales, lifts or glows. Guessed timings do
   * not deserve emphasis — emphasis reads as "this word, now", which is a claim
   * the source cannot back.
   */
  sweep: boolean;
  scale: Spring;
  y: Spring;
  glow: Spring;
  /** dots only */
  opacity: Spring | null;
  /** populated only when kind === "group" */
  letters: LetterUnit[];
  w: Written;
}

export function stateOf(t: number, s: number, e: number): UnitState {
  if (t < s) return "NotSung";
  if (t >= e) return "Sung";
  return "Active";
}

function progress(t: number, s: number, e: number): number {
  if (t <= s) return 0;
  if (t >= e) return 1;
  return (t - s) / (e - s);
}

function promoteToGPU(el: HTMLElement) {
  if (el.dataset.gpu === "1") return;
  el.style.willChange = "transform, opacity, text-shadow, scale";
  el.style.backfaceVisibility = "hidden";
  el.dataset.gpu = "1";
}

function makeLetter(el: HTMLElement): LetterUnit {
  promoteToGPU(el);
  const s = Number(el.dataset.s ?? 0);
  const e = Number(el.dataset.e ?? 0);
  // Only the letter springs are rescaled. Word-level motion still runs at the
  // reference's tuning against the syllable's own window, exactly as in spicy —
  // it is the letters that got handed short windows by lowering the threshold.
  const k = letterFreqScale((e - s) * 1000);
  return {
    el,
    s,
    e,
    scale: new Spring(LETTER_SCALE.at(0), SCALE_FREQ * k, SCALE_DAMP),
    y: new Spring(LETTER_Y.at(0), Y_FREQ * k, Y_DAMP),
    glow: new Spring(GLOW.at(0), GLOW_FREQ * k, GLOW_DAMP),
    w: blankWritten(),
  };
}

function makeWord(el: HTMLElement): WordUnit {
  promoteToGPU(el);
  const kind: WordUnit["kind"] = el.classList.contains("dot")
    ? "dot"
    : el.classList.contains("wordGroup")
      ? "group"
      : "word";

  const isDot = kind === "dot";
  return {
    el,
    s: Number(el.dataset.s ?? 0),
    e: Number(el.dataset.e ?? 0),
    kind,
    sweep: el.dataset.sweep === "1",
    scale: isDot
      ? new Spring(DOT_SCALE.at(0), DOT_SCALE_FREQ, DOT_SCALE_DAMP)
      : new Spring(WORD_SCALE.at(0), SCALE_FREQ, SCALE_DAMP),
    y: isDot
      ? new Spring(DOT_Y.at(0), DOT_Y_FREQ, DOT_Y_DAMP)
      : new Spring(WORD_Y.at(0), Y_FREQ, Y_DAMP),
    glow: isDot
      ? new Spring(DOT_GLOW.at(0), DOT_GLOW_FREQ, DOT_GLOW_DAMP)
      : new Spring(GLOW.at(0), GLOW_FREQ, GLOW_DAMP),
    opacity: isDot
      ? new Spring(DOT_OPACITY.at(0), DOT_OPACITY_FREQ, DOT_OPACITY_DAMP)
      : null,
    letters:
      kind === "group"
        ? Array.from(el.querySelectorAll<HTMLElement>(".letter"), makeLetter)
        : [],
    w: blankWritten(),
  };
}

/** Build the animatable model for one rendered line. */
export function collectLine(lineEl: HTMLElement): WordUnit[] {
  return Array.from(
    lineEl.querySelectorAll<HTMLElement>(".word, .wordGroup, .dot"),
    makeWord,
  );
}

// ── DOM writes ──
function write(
  el: HTMLElement,
  prop: string,
  value: number,
  last: number,
  format: (v: number) => string,
  epsilon: number,
): number {
  if (last === last && Math.abs(value - last) < epsilon) return last;
  el.style.setProperty(prop, format(value));
  return value;
}

const fmt4 = (v: number) => v.toFixed(4);
const fmt3 = (v: number) => v.toFixed(3);
const fmtPct = (v: number) => `${v.toFixed(2)}%`;

const EPS_TRANSFORM = 1e-3;
const EPS_GLOW = 2e-3;
const EPS_GRAD = 0.05;

/** Common transform write for any unit. `yMul` is 2 for letters (see below). */
/**
 * The Lyric motion setting: scales how far each unit grows and lifts away from
 * rest (1 = the tuned motion above, 0 = still, more = bouncier). The fill and
 * glow are not touched.
 */
let motion = 1;
export function setLyricMotion(k: number) {
  motion = k;
}

function writeMotion(u: WordUnit | LetterUnit, scale: number, y: number, yMul: number) {
  u.w.s = write(u.el, "--s", 1 + (scale - 1) * motion, u.w.s, fmt4, EPS_TRANSFORM);
  u.w.y = write(u.el, "--y", y * yMul * motion, u.w.y, fmt4, EPS_TRANSFORM);
}

function writeGlow(
  u: WordUnit | LetterUnit,
  glow: number,
  blurRange: number,
  alphaMul: number,
) {
  const g = Math.max(0, glow);
  u.w.gb = write(u.el, "--gb", GLOW_BLUR_BASE + g * blurRange, u.w.gb, fmt3, EPS_GLOW);
  u.w.go = write(u.el, "--go", Math.min(g * alphaMul, 1), u.w.go, fmt3, EPS_GLOW);
}

function writeGradient(u: WordUnit | LetterUnit, pct: number) {
  u.w.gp = write(u.el, "--gp", pct, u.w.gp, fmtPct, EPS_GRAD);
}

// ── per-frame stepping ──

function stepLetters(group: WordUnit, t: number, dt: number) {
  const wordState = stateOf(t, group.s, group.e);
  const letters = group.letters;

  // Exactly one letter drives the whole group.
  let activeIdx = -1;
  let activeP = 0;
  if (wordState === "Active") {
    for (let i = 0; i < letters.length; i++) {
      if (stateOf(t, letters[i].s, letters[i].e) === "Active") {
        activeIdx = i;
        activeP = progress(t, letters[i].s, letters[i].e);
        break;
      }
    }
  }

  const restScale = LETTER_SCALE.at(0);
  const restY = LETTER_Y.at(0);
  const restGlow = GLOW.at(0);

  for (let k = 0; k < letters.length; k++) {
    const L = letters[k];
    const letterState = stateOf(t, L.s, L.e);

    let targetScale = restScale;
    let targetY = restY;
    let targetGlow = restGlow;
    let targetGrad: number;

    if (wordState === "Active") {
      if (activeIdx !== -1) {
        const baseScale = LETTER_SCALE.at(activeP);
        const baseY = LETTER_Y.at(activeP);
        const baseGlow = GLOW.at(activeP);

        const distance = Math.abs(k - activeIdx);
        const f = posFalloff(distance);
        const gf = glowFalloff(distance);

        targetScale = restScale + (baseScale - restScale) * f;
        targetY = restY + (baseY - restY) * f;
        targetGlow = restGlow + (baseGlow - restGlow) * gf;
      }

      // Letters the bump has not reached yet stay flat regardless of proximity.
      if (letterState === "NotSung") {
        targetScale = restScale;
        targetY = restY;
        targetGlow = restGlow;
      } else if (letterState === "Sung" && activeIdx === -1) {
        targetGlow = GLOW.at(SUNG_LETTER_GLOW);
      }

      // Only the one Active letter has a moving fill. Everything else is
      // binary: fully painted or not painted at all.
      targetGrad =
        letterState === "NotSung"
          ? GRAD_UNSUNG
          : letterState === "Sung"
            ? GRAD_SUNG
            : k === activeIdx
              ? GRAD_UNSUNG + GRAD_SPAN * easeSinOut(activeP)
              : GRAD_UNSUNG;
    } else if (wordState === "NotSung") {
      targetGrad = GRAD_UNSUNG;
    } else {
      targetScale = LETTER_SCALE.at(1);
      targetY = LETTER_Y.at(1);
      targetGlow = GLOW.at(1);
      targetGrad = GRAD_SUNG;
    }

    L.scale.setGoal(targetScale);
    L.y.setGoal(targetY);
    L.glow.setGoal(targetGlow);

    const cScale = L.scale.step(dt);
    const cY = L.y.step(dt);
    const cGlow = L.glow.step(dt);

    // ×2: the reference doubles the letter lift so it clears the word-level
    // lift it is riding on top of.
    writeMotion(L, cScale, cY, 2);
    writeGlow(L, cGlow, LETTER_BLUR_RANGE, LETTER_GLOW_ALPHA);
    writeGradient(L, targetGrad);
  }
}

function stepDot(dot: WordUnit, t: number, dt: number) {
  const state = stateOf(t, dot.s, dot.e);
  const p = state === "Active" ? progress(t, dot.s, dot.e) : state === "Sung" ? 1 : 0;

  dot.scale.setGoal(DOT_SCALE.at(p));
  dot.y.setGoal(DOT_Y.at(p));
  dot.glow.setGoal(DOT_GLOW.at(p));
  dot.opacity!.setGoal(DOT_OPACITY.at(p));

  const cScale = dot.scale.step(dt);
  const cY = dot.y.step(dt);
  const cGlow = dot.glow.step(dt);
  const cOpacity = dot.opacity!.step(dt);

  writeMotion(dot, cScale, cY, 1);
  writeGlow(dot, cGlow, DOT_BLUR_RANGE, DOT_GLOW_ALPHA);
  dot.w.o = write(dot.el, "--o", cOpacity, dot.w.o, fmt3, EPS_TRANSFORM);
}

/**
 * Advance one frame for a whole line.
 *
 * @param words the line's units, in reading order
 * @param t     playback position in seconds (already offset-corrected)
 * @param dt    seconds since the previous frame
 */
export function stepLine(words: WordUnit[], t: number, dt: number) {
  if (!words.length) return;
  // A backgrounded window can hand us a multi-second dt. The springs converge
  // rather than explode at that size, but clamping keeps the first frame after
  // a stall from skipping the settle entirely.
  const d = Math.min(Math.max(dt, 0), 0.1);

  for (let i = 0; i < words.length; i++) {
    const W = words[i];

    if (W.kind === "dot") {
      stepDot(W, t, d);
      continue;
    }

    const state = stateOf(t, W.s, W.e);
    const p = state === "Active" ? progress(t, W.s, W.e) : state === "Sung" ? 1 : 0;

    if (W.sweep) {
      // Fill only. Motion and glow are left untouched, so the CSS resting
      // values hold and nothing pops on a timing we merely guessed.
      writeGradient(
        W,
        state === "Active"
          ? GRAD_UNSUNG + GRAD_SPAN * p
          : state === "Sung"
            ? GRAD_SUNG
            : GRAD_UNSUNG,
      );
      continue;
    }

    W.scale.setGoal(WORD_SCALE.at(p));
    W.y.setGoal(WORD_Y.at(p));
    W.glow.setGoal(GLOW.at(p));

    const cScale = W.scale.step(d);
    const cY = W.y.step(d);
    const cGlow = W.glow.step(d);

    // Word-level motion applies to plain words *and* to letter groups — the
    // group carries its syllable's own rise while its letters ripple inside it.
    writeMotion(W, cScale, cY, 1);

    if (W.kind === "group") {
      stepLetters(W, t, d);
    } else {
      // Whole-syllable fill: linear, no easing (the reference eases only the
      // per-letter sweep).
      writeGradient(
        W,
        state === "Active"
          ? GRAD_UNSUNG + GRAD_SPAN * p
          : state === "Sung"
            ? GRAD_SUNG
            : GRAD_UNSUNG,
      );
      writeGlow(W, cGlow, WORD_BLUR_RANGE, WORD_GLOW_ALPHA);
    }
  }
}

const CUSTOM_PROPS = ["--s", "--y", "--gb", "--go", "--gp", "--o"] as const;

function clearUnit(u: WordUnit | LetterUnit) {
  for (const prop of CUSTOM_PROPS) u.el.style.removeProperty(prop);
  u.w = blankWritten();
  u.scale.reset();
  u.y.reset();
  u.glow.reset();
}

/** Return a line's units to idle, e.g. when it stops being current. */
export function resetLine(words: WordUnit[]) {
  for (const W of words) {
    clearUnit(W);
    W.opacity?.reset();
    for (const L of W.letters) clearUnit(L);
  }
}
