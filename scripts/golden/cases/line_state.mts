// golden/line_state.json: the line-state rules of the widget (src/lineState.ts, moved verbatim out of src/App.tsx) over every fixture lyric set.
import { register } from "node:module";
import type { Fixtures } from "../run.mts";

// lineState.ts, lyrics.ts and spicy.ts import siblings without an extension, which Node's loader does not resolve
register(
  "data:text/javascript," +
    encodeURIComponent(`export async function resolve(s, c, next) {
  try { return await next(s, c); } catch (e) {
    if (s.startsWith(".") && !/\\.[mc]?[jt]s$/.test(s)) return next(s + ".ts", c);
    throw e;
  }
}`),
  import.meta.url,
);

const LS = await import("../../../src/lineState.ts");
const { parseLrc, parseYrc, estimateWordTimings } = await import("../../../src/lyrics/lyrics.ts");
const { loadSpicyLyrics } = await import("../../../src/lyrics/spicy.ts");
type LyricLine = import("../../../src/lyrics/lyrics.ts").LyricLine;

// JSON cannot carry NaN / Infinity / -0: those travel as strings
const enc = (n: number): number | string =>
  Number.isNaN(n) ? "NaN" : n === Infinity ? "Infinity" : n === -Infinity ? "-Infinity" : Object.is(n, -0) ? "-0" : n;

// Output lines are stored as an index into a base list when they equal one of its lines (keeps the golden small);
// Rust decodes them the same way.
const refs = (out: LyricLine[], base: LyricLine[]) => {
  const at = new Map<string, number>();
  base.forEach((l, i) => at.has(JSON.stringify(l)) || at.set(JSON.stringify(l), i));
  return out.map((l) => at.get(JSON.stringify(l)) ?? l);
};

const L = (t: number, text: string, extra: Partial<LyricLine> = {}): LyricLine => ({ t, text, ...extra });

async function spicyLines(data: unknown): Promise<LyricLine[]> {
  (globalThis as any).window = { player: { fetchSpicyLyrics: async () => ({ status: "ok", lyrics: data }) } };
  const quiet = console.error;
  console.error = () => {};
  try {
    return (await loadSpicyLyrics({ artist: "a", title: "t" }))?.lines ?? [];
  } finally {
    console.error = quiet;
  }
}

export default async (fx: Fixtures) => {
  // ── lyric sets: what the widget's pipeline starts from ──────
  const sets: { name: string; lines: LyricLine[] }[] = [];
  const add = (name: string, lines: LyricLine[]) => sets.push({ name, lines });

  for (const f of fx.list("lrclib")) {
    const j = fx.json(f);
    const lines = parseLrc(j.syncedLyrics ?? "");
    add(f, lines);
    add(`${f}#estimated`, estimateWordTimings(lines));
  }
  for (const f of fx.list("netease")) {
    const j = fx.json(f);
    add(`${f}#lrc`, parseLrc(j.lrc?.lyric ?? ""));
    add(`${f}#yrc`, parseYrc(j.yrc?.lyric ?? ""));
  }
  for (const f of ["backing.lrc", "chinese.lrc", "empty.lrc", "gaps.lrc"]) {
    const lines = parseLrc(fx.text(`edge/${f}`));
    add(`edge/${f}`, lines);
    add(`edge/${f}#estimated`, estimateWordTimings(lines));
  }
  add("edge/plain.txt", parseLrc(fx.text("edge/plain.txt")));
  for (const f of [...fx.list("spicy"), ...fx.list("spicy-edge")].filter((f) => f.endsWith(".json"))) {
    add(f, await spicyLines(fx.json(f)));
  }
  // hand-made: backing vocals around gaps, exact thresholds, ends missing / earlier than starts
  const w = (t: number, end: number | undefined, text = "x") => ({ t, end, text });
  add("hand:empty", []);
  add("hand:one", [L(0, "a")]);
  add("hand:intro exactly 3s", [L(3, "a"), L(4, "b")]);
  add("hand:intro just under 3s", [L(2.999, "a"), L(5, "b")]);
  add("hand:gap exactly 3s", [L(0, "a", { end: 1 }), L(4, "b", { end: 6 })]);
  add("hand:gap just under 3s", [L(0, "a", { end: 1 }), L(3.999, "b", { end: 6 })]);
  add("hand:gap after bg", [L(0, "lead", { end: 2 }), L(1, "back", { bg: true, end: 2.5 }), L(10, "next", { end: 12 })]);
  add("hand:bg first", [L(1, "back", { bg: true, end: 2 }), L(2, "lead", { end: 3 }), L(9, "x", { end: 10 })]);
  add("hand:only bg", [L(5, "a", { bg: true, end: 6 }), L(9, "b", { bg: true })]);
  add("hand:trailing bg", [L(0, "a", { end: 1 }), L(1, "b", { bg: true })]);
  add("hand:no ends", [L(0, "a"), L(2, "b"), L(10, "c"), L(11, "d")]);
  add("hand:words no ends", [
    L(0, "a b", { words: [w(0, undefined), w(1, undefined)] }),
    L(10, "c", { words: [w(10, undefined)] }),
    L(20, "d", { words: [w(20, 21.5)] }),
    L(30, "e", { words: [] }),
    L(40, "f"),
  ]);
  add("hand:words past next", [L(0, "a", { words: [w(0, 50)] }), L(5, "b", { words: [w(5, 6)] })]);
  add("hand:end before start", [L(10, "a", { end: 5 }), L(20, "b", { end: 12 }), L(30, "c")]);
  add("hand:same time", [L(5, "a"), L(5, "b"), L(5, "c")]);
  add("hand:negative t", [L(-5, "a"), L(-1, "b"), L(7, "c")]);
  add("hand:zero-length", [L(0, "a", { end: 0 }), L(3, "b", { end: 3 }), L(6, "c", { end: 6 })]);
  add("hand:preexisting gap lines", [L(0, "a", { end: 1 }), L(1, "• • •", { gap: true, end: 9, hideAt: 8.5 }), L(9, "b", { end: 10 })]);
  add("hand:fractions", [L(0.1, "a", { end: 0.30000000000000004 }), L(3.4000000000000004, "b", { end: 7.1 }), L(1e6, "c")]);

  // ── resolveActive / isGapHiding sample times ───────────────
  const tsFor = (lines: LyricLine[]): number[] => {
    const set = new Set<number>([-1e9, -5, -0.001, 0, 0.001, 0.5]);
    const last = lines.reduce((m, l) => Math.max(m, l.end ?? l.t, l.hideAt ?? 0), 0);
    for (let t = 0; t <= Math.min(last + 6, 400); t += 1.13) set.add(Math.round(t * 1e6) / 1e6);
    for (const l of lines.slice(0, 60)) {
      for (const v of [l.t, l.end, l.hideAt]) {
        if (v == null) continue;
        for (const d of [-0.001, 0, 1e-9]) set.add(v + d);
      }
    }
    set.add(last + 100);
    set.add(1e9);
    set.add(NaN);
    set.add(Infinity);
    set.add(-Infinity);
    return [...set];
  };

  const pipeline = sets.map(({ name, lines }) => {
    const variants: Record<string, any> = {};
    for (const estimated of [false, true]) {
      const sealed = LS.sealEnds(lines, estimated);
      const gapped = LS.withGaps(lines, estimated);
      const finished = LS.finishLines(lines, true, estimated);
      const ts = tsFor(finished).filter((_, i) => !estimated || i % 3 === 0);
      const resolved = ts.map((t) => {
        const r = LS.resolveActive(finished, t);
        return `${enc(t)} ${r.active.join(",")} ${r.anchor} ${LS.isGapHiding(finished[r.anchor], t) ? 1 : 0}`;
      });
      // a second, coarser pass over the sealed-only lines (what a source without gaps sees)
      const sealedTs = ts.filter((_, i) => i % 7 === 0);
      const resolvedSealed = sealedTs.map((t) => {
        const r = LS.resolveActive(sealed, t);
        return `${enc(t)} ${r.active.join(",")} ${r.anchor}`;
      });
      // per-line render flags and depth cue at a few times
      const pickTs = [0, 33.3, 120, NaN];
      const firstIdx = LS.firstLyricIndex(finished);
      const leadOf = LS.leadIndexes(finished);
      const flags = pickTs.map((t) => {
        const { active, anchor } = LS.resolveActive(finished, t);
        const set = new Set(active);
        const rows = finished.slice(0, 70).map((_, i) => {
          const f = LS.lineFlags(finished, i, true, set, t, firstIdx);
          return `${+f.isCurrent}${+f.isPast}${+f.isIntroPreview} ${LS.lineBlur(true, true, anchor, f.isIntroPreview, leadOf, i)}`;
        });
        const off = finished.slice(0, 3).map((_, i) => {
          const f = LS.lineFlags(finished, i, false, set, t, firstIdx);
          return `${+f.isCurrent}${+f.isPast}${+f.isIntroPreview} ${LS.lineBlur(false, true, anchor, f.isIntroPreview, leadOf, i)} ${LS.lineBlur(true, false, anchor, false, leadOf, i)}`;
        });
        return { t: enc(t), anchor, rows, off };
      });
      const ends = lines.slice(0, -1).map((l, i) => `${LS.lineEndOf(l, lines[i + 1], estimated)} ${LS.lineEndOf(l, lines[i + 1], !estimated)}`);
      const sealedFin = LS.sealEnds(lines, estimated);
      variants[String(estimated)] = {
        sealedEnds: sealed.map((l) => String(l.end)),
        gapped: refs(gapped, lines),
        finished: refs(finished, sealedFin), resolved,
        resolvedSealed,
        firstIdx,
        leadOf: leadOf.join(","),
        flags,
        ends,
      };
    }
    return { name, in: lines, unsyncedSame: LS.finishLines(lines, false, false) === lines, variants };
  });

  // ── makeGap ────────────────────────────────────────────────
  const gapTimes = [0, 0.5, 1, 3, 3.3, 7.77, 10, 1e6, -2, 1 / 3, 1e-9];
  const makeGap: any[] = [];
  for (const a of gapTimes) for (const b of gapTimes) makeGap.push({ in: [a, b], out: LS.makeGap(a, b) });
  for (const [a, b] of [[NaN, 3], [0, NaN], [Infinity, 3], [0, Infinity], [-Infinity, 0]]) makeGap.push({ in: [enc(a), enc(b)], out: LS.makeGap(a, b) });

  // ── isGapHiding ────────────────────────────────────────────
  const hideLines: (LyricLine | undefined)[] = [
    undefined,
    L(0, "x"),
    L(0, "x", { gap: false, hideAt: 1 }),
    L(0, "x", { gap: true }),
    L(0, "x", { gap: true, hideAt: 4.5 }),
    L(0, "x", { gap: true, hideAt: 0 }),
    L(0, "x", { gap: true, hideAt: -1 }),
    LS.makeGap(0, 9),
  ];
  const isGapHiding: any[] = [];
  for (const line of hideLines) {
    const hideAt = line?.hideAt;
    const ts = [-1, 0, 4.4999999, 4.5, 4.5000001, 8.45, 8.5, 9, 100, NaN, Infinity, -Infinity];
    if (hideAt != null) ts.push(hideAt, hideAt - 1e-9, hideAt + 1e-9);
    for (const t of ts) isGapHiding.push({ line: line ?? null, t: enc(t), out: LS.isGapHiding(line, t) });
  }

  // ── fmt ────────────────────────────────────────────────────
  const fmtVals = [
    0, -0, 0.5, 0.999, 1, 59, 59.5, 59.999, 59.9999999999, 60, 60.001, 61, 599.999, 600, 3599, 3599.999, 3600, 3661, 36000, 86399, 86400, 1e6,
    1e9, 1e21, 1.7976931348623157e308, 5e-324, -0.5, -1, -59.999, -60, -61, -3600, -3661, -1e9, 1 / 3, 125.7, 215.4999, 184.2, 0.1 + 0.2,
    NaN, Infinity, -Infinity,
  ];
  for (let s = 0; s < 400; s += 7.31) fmtVals.push(Math.round(s * 1000) / 1000);
  const fmt: any[] = [];
  for (const v of fmtVals) for (const r of [false, true]) fmt.push({ in: [enc(v), r], out: LS.fmt(v, r) });
  for (const v of [0, 12.5, -3, NaN]) fmt.push({ in: [enc(v)], out: LS.fmt(v) }); // remaining defaults to false

  // ── splitForWave ───────────────────────────────────────────
  const waveTexts = [
    "", "a", "ab", "hello", "Hello World", " ", "  ", "a b", "yeah~", "ooh-ooh",
    "é", "é", "éé", "à́b", "́", "́́", "ñandú",
    "😀", "😀😀", "ab😀", "👍🏽", "👍🏽👍🏽", "👨‍👩‍👧‍👦", "👨‍👩‍👧‍👦👨‍👩‍👧‍👦", "🏳️‍🌈", "🏳️‍🌈a", "🇯🇵", "🇯🇵🇰🇷", "🇯🇵🇰🇷🇺🇸", "🇺🇸🇬🇧🇫", "❤️", "❤️❤️", "1️⃣2️⃣", "©️", "⌚︎", "✌🏻✌🏿", "🧑‍🚀🧑🏽‍🚀",
    "가", "한글", "안녕하세요", "ᄒ", "ᄒᆞᆫ", "각", "각가", "각", "각각", "각", "ᄀ가",
    "あい", "ゔ゛", "ア゙", "窗外", "窗外的雨", "你好", "𠀀𠀁", "𠀀",
    "नमस्ते", "हिन्दी", "ক্ষ", "தமிழ்", "ไทย", "สวัสดี", "ខ្មែរ", "abก", "aก", "กa",
    "مرحبا", "سلام", "שלום", "ab؀", "֐x", "x׿", "ݐa", "ࢠa", "ॿ", "ঀ", "৿", "஀", "௿", "๿", "ក", "៿",
    "֏x", "ऀ", "਀ab", "ഀab", "ab຀", "abc๿", "ͣ", "ab‍", "a‍b", "‍‍", "a\r\nb", "\r\n", "\n\n", "a\nb", "  ",
    "ÀÉ", "Å", "ǆ", "ﬁ", "ß", "İ", "ä̈b", "กำ", "ก็", "ษ",
    // (lone surrogates are left out: a Rust &str cannot hold them)
  ];
  const splitForWave = [...new Set(waveTexts)].map((s) => ({ in: s, out: LS.splitForWave(s) }));

  // ── clock helpers ──────────────────────────────────────────
  const positions = [0, 0.3, 12.34, 100, 213.6, -1, NaN, Infinity, -Infinity, 1e9];
  const offsets = [0, 0.1, -0.1, 0.5, -2, 0.30000000000000004, NaN, Infinity];
  const lyricTimeOf: any[] = [];
  for (const p of positions) for (const o of offsets) lyricTimeOf.push({ in: [enc(p), enc(o)], out: enc(LS.lyricTimeOf(p, o)) });

  const durs = [0, 5, 19.99, 20, 20.001, 30, 180, 215.5, -1, NaN, Infinity];
  const nears = [-1, 0, 0.001, 5, 9.99, 10, 10.001, 25, 169.99, 170, 170.01, 205.5, 205.499999, 215.5, 300, NaN, Infinity];
  const isNearEnd: any[] = [];
  for (const d of durs) for (const n of nears) isNearEnd.push({ in: [enc(d), enc(n)], out: LS.isNearEnd(d, n) });

  return {
    consts: {
      NEXT_POPUP_SECS: LS.NEXT_POPUP_SECS,
      MIN_GAP: LS.MIN_GAP,
      PRE_HIDDEN_DOT_MS: LS.PRE_HIDDEN_DOT_MS,
      INTERLUDE_PADDING: LS.INTERLUDE_PADDING,
      LYRIC_LEAD: LS.LYRIC_LEAD,
      BLUR_STEP: LS.BLUR_STEP,
      BLUR_MAX: LS.BLUR_MAX,
      UNSPLITTABLE: LS.UNSPLITTABLE.source,
    },
    pipeline,
    makeGap,
    isGapHiding,
    fmt,
    splitForWave,
    lyricTimeOf,
    isNearEnd,
  };
};
