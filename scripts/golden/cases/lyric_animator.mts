// golden/lyric_animator.json: src/lyrics/lyricAnimator.ts run in Node over a
// fake DOM. Real Spicy syllable timings (fixtures/spicy) become lines built the
// way App.tsx Units builds them (letterWindows decides word vs wordGroup),
// then stepLine runs for 2 s at a fixed dt (1/60, 1/144), for both animation
// styles and several lyricMotion values, plus a run with an irregular dt
// sequence (a stall > 0.1 s, zero, negative). Every frame is stepped; every
// SAMPLE-th frame is recorded: all springs (x, v, g), the Written numbers, and
// the six style custom properties the TS set on each element.
//
// Numbers are strings (String(n), "-0" for negative zero) so they round-trip
// exactly. A frame is one string of "|"-separated tokens, units in reading
// order (a wordGroup is followed by its letters):
//   per unit: scale x v g, y x v g, glow x v g, [opacity x v g, dots only],
//             w.s w.y w.gb w.go w.gp w.o, then style --s --y --gb --go --gp --o
//             ("-" = property not set)
//   init/reset strings add three tokens per unit after those: willChange,
//             backfaceVisibility, dataset.gpu ("-" when unset).
import {
  collectLine,
  letterWindows,
  resetLine,
  setAnimationStyle,
  setLyricMotion,
  stateOf,
  stepLine,
  type AnimationStyle,
  type LetterUnit,
  type WordUnit,
} from "../../../src/lyrics/lyricAnimator.ts";
import type { Fixtures } from "../run.mts";

const enc = (n: number) => (Object.is(n, -0) ? "-0" : String(n));
const SAMPLE_60 = 6;
const SAMPLE_144 = 14;

// ── fake DOM: only what collectLine and the stepper touch ──
interface FakeEl {
  classList: { contains(c: string): boolean };
  dataset: Record<string, string>;
  style: {
    props: Map<string, string>;
    willChange?: string;
    backfaceVisibility?: string;
    setProperty(p: string, v: string): void;
    removeProperty(p: string): void;
  };
  children: FakeEl[];
  querySelectorAll(sel: string): FakeEl[];
}

function fakeEl(classes: string[], dataset: Record<string, string>, children: FakeEl[] = []): FakeEl {
  const props = new Map<string, string>();
  const el: FakeEl = {
    classList: { contains: (c) => classes.includes(c) },
    dataset: { ...dataset },
    style: {
      props,
      setProperty: (p, v) => void props.set(p, v),
      removeProperty: (p) => void props.delete(p),
    },
    children,
    querySelectorAll(sel) {
      const want = sel.split(",").map((s) => s.trim().replace(/^\./, ""));
      return this.children.filter((c) => want.some((w) => c.classList.contains(w)));
    },
  };
  return el;
}

// ── lines, built like App.tsx Units ──
interface Syl {
  t: number;
  end: number;
  text: string;
}
interface Input {
  dot: boolean;
  wordGroup: boolean;
  s: number;
  e: number;
  sweep: boolean;
  letters: { s: number; e: number }[];
}
interface Line {
  name: string;
  t0: number; // first step time
  el: FakeEl; // the line element
  inputs: Input[]; // what collectLine reads from it
}

const f3 = (n: number) => n.toFixed(3);

function wordLine(name: string, syls: Syl[], lineEnd: number, animation: AnimationStyle, t0: number): Line {
  const els: FakeEl[] = [];
  const inputs: Input[] = [];
  syls.forEach((w, i) => {
    const next = syls[i + 1];
    const start = w.t;
    const end = w.end ?? next?.t ?? lineEnd ?? start + 0.5;
    const letters = Array.from(w.text.trim());
    const win = letters.length ? letterWindows(start, end, letters.length, animation) : null;
    if (!win) {
      els.push(fakeEl(["word"], { s: f3(start), e: f3(end) }));
      inputs.push({ dot: false, wordGroup: false, s: Number(f3(start)), e: Number(f3(end)), sweep: false, letters: [] });
      return;
    }
    const lets = letters.map((_, li) =>
      fakeEl(["letter"], { s: f3(start + li * win.per), e: f3(start + (li + 1) * win.per) }),
    );
    els.push(fakeEl(["wordGroup"], { s: f3(start), e: f3(win.groupEnd) }, lets));
    inputs.push({
      dot: false,
      wordGroup: true,
      s: Number(f3(start)),
      e: Number(f3(win.groupEnd)),
      sweep: false,
      letters: letters.map((_, li) => ({
        s: Number(f3(start + li * win.per)),
        e: Number(f3(start + (li + 1) * win.per)),
      })),
    });
  });
  // a stray non-matching child, as the real line has spaces and wrappers
  return { name, t0, el: fakeEl(["line"], {}, [fakeEl(["word-glue"], {}), ...els]), inputs };
}

function dotLine(name: string, t0: number): Line {
  const spans = [
    [t0 + 0.2, t0 + 0.8],
    [t0 + 0.8, t0 + 1.4],
    [t0 + 1.4, t0 + 2.0],
  ];
  return {
    name,
    t0,
    el: fakeEl(["line"], {}, spans.map(([s, e]) => fakeEl(["dot"], { s: f3(s), e: f3(e) }))),
    inputs: spans.map(([s, e]) => ({
      dot: true,
      wordGroup: false,
      s: Number(f3(s)),
      e: Number(f3(e)),
      sweep: false,
      letters: [],
    })),
  };
}

function sweepLine(name: string, t0: number): Line {
  const s = t0 + 0.3,
    e = t0 + 1.7;
  return {
    name,
    t0,
    el: fakeEl(["line"], {}, [fakeEl(["word"], { s: f3(s), e: f3(e), sweep: "1" })]),
    inputs: [{ dot: false, wordGroup: false, s: Number(f3(s)), e: Number(f3(e)), sweep: true, letters: [] }],
  };
}

type Maker = (animation: AnimationStyle) => Line;

function realLines(fx: Fixtures): Maker[] {
  const makers: Maker[] = [];
  for (const file of [
    "spicy/24Dl2hfEkqdEq6QCooE2Z8.json",
    "spicy/3SSQha0LUUl3ZUd2AxbThr.json",
    "spicy/3ITCIl15EUUuGJmFuOuTdW.json",
  ]) {
    const data = fx.json(file);
    const lines: { syls: Syl[]; end: number }[] = data.Content.filter((c: any) => c.Lead?.Syllables?.length).map(
      (c: any) => ({
        syls: c.Lead.Syllables.map((s: any) => ({ t: s.StartTime, end: s.EndTime, text: s.Text })),
        end: c.Lead.EndTime,
      }),
    );
    // the line holding the longest syllable, windowed around it
    let bi = 0,
      bj = 0,
      bd = -1;
    lines.forEach((l, i) =>
      l.syls.forEach((s, j) => {
        if (s.end - s.t > bd) {
          bd = s.end - s.t;
          bi = i;
          bj = j;
        }
      }),
    );
    const held = lines[bi];
    const a = Math.max(0, bj - 3);
    const heldSyls = held.syls.slice(a, bj + 5);
    const heldStart = held.syls[bj].t;
    makers.push((an) => wordLine(`${file}#held`, heldSyls, held.end, an, heldStart - 0.3));
    // the line with the most syllables, first eight
    let mi = 0;
    lines.forEach((l, i) => {
      if (l.syls.length > lines[mi].syls.length) mi = i;
    });
    const many = lines[mi].syls.slice(0, 8);
    makers.push((an) => wordLine(`${file}#many`, many, lines[mi].end, an, many[0].t - 0.2));
  }
  return makers;
}

// ── snapshots ──
const sty = (el: FakeEl, p: string) => el.style.props.get(p) ?? "-";

function unitTokens(u: WordUnit | LetterUnit, extra: boolean, out: string[]) {
  const el = u.el as unknown as FakeEl;
  for (const sp of [u.scale, u.y, u.glow]) out.push(enc(sp.x), enc(sp.v), enc(sp.g));
  const op = (u as WordUnit).opacity;
  if (op) out.push(enc(op.x), enc(op.v), enc(op.g));
  const w = u.w;
  out.push(enc(w.s), enc(w.y), enc(w.gb), enc(w.go), enc(w.gp), enc(w.o));
  for (const p of ["--s", "--y", "--gb", "--go", "--gp", "--o"]) out.push(sty(el, p));
  if (extra) out.push(el.style.willChange ?? "-", el.style.backfaceVisibility ?? "-", el.dataset.gpu ?? "-");
}

function frame(words: WordUnit[], extra: boolean): string {
  const out: string[] = [];
  for (const W of words) {
    unitTokens(W, extra, out);
    for (const L of W.letters) unitTokens(L, extra, out);
  }
  return out.join("|");
}

// ── runs ──
const JITTER = [1 / 60, 1 / 60, 0.3, 1 / 144, 0, -0.5, 1 / 30, 0.05, 0.1, 0.1000001, 1 / 60, 0.25];

interface RunSpec {
  maker: Maker;
  style: AnimationStyle;
  motion: number;
  dt?: number;
  dtSeq?: number[];
  frames: number;
  sample: number;
}

function run(spec: RunSpec) {
  setAnimationStyle(spec.style);
  setLyricMotion(spec.motion);
  const line = spec.maker(spec.style);
  const words = collectLine(line.el as unknown as HTMLElement);
  const init = frame(words, true);
  const snaps: { i: number; t: string; s: string }[] = [];
  let t = line.t0;
  for (let i = 0; i < spec.frames; i++) {
    const dt = spec.dtSeq ? spec.dtSeq[i % spec.dtSeq.length] : spec.dt!;
    stepLine(words, t, dt);
    if (i % spec.sample === 0 || i === spec.frames - 1) snaps.push({ i, t: enc(t), s: frame(words, false) });
    t += dt;
  }
  resetLine(words);
  const reset = frame(words, true);
  return {
    name: line.name,
    style: spec.style,
    motion: enc(spec.motion),
    dt: spec.dt === undefined ? null : enc(spec.dt),
    dt_seq: spec.dtSeq ? spec.dtSeq.map(enc) : null,
    frames: spec.frames,
    sample: spec.sample,
    t0: enc(line.t0),
    line: line.inputs.map((x) => ({
      dot: x.dot,
      word_group: x.wordGroup,
      s: enc(x.s),
      e: enc(x.e),
      sweep: x.sweep,
      letters: x.letters.map((l) => [enc(l.s), enc(l.e)]),
    })),
    init,
    snaps,
    reset,
  };
}

export default (fx: Fixtures) => {
  const makers: Maker[] = [...realLines(fx), () => dotLine("dots", 10), () => sweepLine("sweep", 20)];
  const styles: AnimationStyle[] = ["playback", "spicy"];
  const runs: ReturnType<typeof run>[] = [];

  for (const m of makers)
    for (const style of styles) {
      runs.push(run({ maker: m, style, motion: 1, dt: 1 / 60, frames: 120, sample: SAMPLE_60 }));
      runs.push(run({ maker: m, style, motion: 1, dt: 1 / 144, frames: 288, sample: SAMPLE_144 }));
    }
  // lyricMotion variants: still, half, bouncy (and a negative one)
  for (const m of makers.slice(0, 2).concat(makers.slice(-2)))
    for (const style of styles)
      for (const motion of [0, 0.5, 2.5, -1])
        runs.push(run({ maker: m, style, motion, dt: 1 / 60, frames: 120, sample: SAMPLE_60 }));
  // irregular dt: stall, zero, negative, clamp edge
  for (const m of makers)
    for (const style of styles)
      runs.push(run({ maker: m, style, motion: 1, dtSeq: JITTER, frames: 120, sample: SAMPLE_60 }));

  // stepLine on an empty line is a no-op
  stepLine([], 1, 1 / 60);

  // letterWindows
  const lw: unknown[] = [];
  const starts = [0, 12.345, 100.0005];
  const durs = [-1, 0, 0.001, 0.05, 0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.75, 0.999, 1, 1.0001, 1.5, 2, 3.7, 10];
  for (const an of styles)
    for (const start of starts)
      for (const d of durs)
        for (const count of [0, 1, 2, 3, 5, 8, 20]) {
          const r = letterWindows(start, start + d, count, an);
          lw.push({
            start: enc(start),
            end: enc(start + d),
            count,
            animation: an,
            out: r && { group_end: enc(r.groupEnd), per: enc(r.per) },
          });
        }

  // stateOf
  const so: unknown[] = [];
  const vals = [-1, 0, 0.5, 1, 1.5, 2, 2.0000000001, 3, NaN];
  for (const t of vals)
    for (const s of vals)
      for (const e of vals) so.push({ t: enc(t), s: enc(s), e: enc(e), out: stateOf(t, s, e) });

  return { runs, letter_windows: lw, state_of: so };
};
