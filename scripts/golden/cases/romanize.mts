// golden/romanize.json: src/lyrics/romanize.ts, every exported function (and the helpers exported for this), over
// lyric lines from every fixture (LRC, YRC, Spicy), hand-made Japanese/Korean/Chinese/mixed lines, reading tokens
// from the real kuromoji (what electron/readings.cjs sends) and fabricated ones (missing fields, wrong counts).
// The inputs are stored next to the outputs: tests/romanize.rs feeds the Rust port the golden's own inputs.
import { register } from "node:module";
import { createRequire } from "node:module";
import type { Fixtures } from "../run.mts";
import {
  alignRomanizedToWords,
  applyPinyin,
  applyReadings,
  attaches,
  detectLyricLang,
  hasHanScript,
  hasNonLatinScript,
  isSpace,
  joinChars,
  katakanaToHiragana,
  lineChars,
  linesNeedReadings,
  linesNeedRomanization,
  morae,
  readingTexts,
  romanizeJapanese,
  romanizeKanaHangul,
  romanizeKorean,
  romanizeLines,
  romanizeText,
  spread,
  tokenChars,
  withChars,
  withRomaLines,
  type CharText,
  type ReadingToken,
} from "../../../src/lyrics/romanize.ts";
import { parseLrc, parseYrc, type LyricLine, type LyricWord } from "../../../src/lyrics/lyrics.ts";

// spicy.ts imports "./lyrics" and "../vendor/objpack" without an extension, which Node's loader does not resolve
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
const { loadSpicyLyrics } = await import("../../../src/lyrics/spicy.ts");

const require = createRequire(import.meta.url);
const kuromoji = require("kuromoji");
type Tok = { surface_form: string; reading?: string; pos: string; pos_detail_1: string; basic_form: string };
const tokenizer: { tokenize(t: string): Tok[] } = await new Promise((ok, no) =>
  kuromoji.builder({ dicPath: "node_modules/kuromoji/dict" }).build((e: unknown, t: never) => (e ? no(e) : ok(t))),
);
// what electron/readings.cjs answers
const kuromojiTokens = (texts: string[]): ReadingToken[][] =>
  texts.map((text) =>
    tokenizer.tokenize(text).map((t) => ({
      s: t.surface_form,
      ...(t.reading ? { r: t.reading } : {}),
      p: t.pos,
      d: t.pos_detail_1,
      b: t.basic_form,
    })),
  );

const L = (t: number, text: string, extra: Partial<LyricLine> = {}): LyricLine => ({ t, text, ...extra });
const W = (t: number, text: string, extra: Partial<LyricWord> = {}): LyricWord => ({ t, text, ...extra });
const wordLine = (t: number, parts: (string | [string, Partial<LyricWord>])[], extra: Partial<LyricLine> = {}): LyricLine => {
  const words = parts.map((p, i) => (typeof p === "string" ? W(t + i * 0.25, p, { end: t + i * 0.25 + 0.25 }) : W(t + i * 0.25, p[0], { end: t + i * 0.25 + 0.25, ...p[1] })));
  return { t, text: words.map((w) => w.text).join(""), words, ...extra };
};

export default async (fx: Fixtures) => {
  // ── line sets ────────────────────────────────────────────
  const sets: { name: string; lines: LyricLine[] }[] = [];
  const addSet = (name: string, lines: LyricLine[]) => sets.push({ name, lines });

  for (const f of fx.list("lrclib")) {
    const j = fx.json(f);
    addSet(`${f}#synced`, parseLrc(j.syncedLyrics ?? ""));
    addSet(`${f}#plain`, parseLrc(j.plainLyrics ?? ""));
  }
  const romaSources: { name: string; lines: LyricLine[]; roma: LyricLine[] }[] = [];
  for (const f of fx.list("netease")) {
    const j = fx.json(f);
    const lrc = parseLrc(j.lrc?.lyric ?? "");
    const roma = parseLrc(j.romalrc?.lyric ?? "");
    const yrc = parseYrc(j.yrc?.lyric ?? "");
    addSet(`${f}#lrc`, lrc);
    addSet(`${f}#tlyric`, parseLrc(j.tlyric?.lyric ?? ""));
    addSet(`${f}#romalrc`, roma);
    addSet(`${f}#yrc`, yrc);
    romaSources.push({ name: `${f}#lrc+romalrc`, lines: lrc, roma }, { name: `${f}#yrc+romalrc`, lines: yrc, roma });
  }
  for (const f of ["backing.lrc", "chinese.lrc", "empty.lrc", "gaps.lrc"]) addSet(`edge/${f}`, parseLrc(fx.text(`edge/${f}`)));
  addSet("edge/plain.txt", parseLrc(fx.text("edge/plain.txt")));
  addSet(
    "edge/scripts.txt",
    fx
      .text("edge/scripts.txt")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((text, i) => L(i + 1, text)),
  );
  const spicyFiles = [...fx.list("spicy"), ...fx.list("spicy-edge")].filter((f) => f.endsWith(".json"));
  const quiet = console.error;
  console.error = () => {};
  for (const f of spicyFiles) {
    (globalThis as any).window = { player: { fetchSpicyLyrics: async () => ({ status: "ok", lyrics: fx.json(f) }) } };
    const r = await loadSpicyLyrics({ artist: "a", title: "t" });
    if (!r) continue;
    addSet(`${f}#lines`, r.lines);
    if (r.romanized) {
      addSet(`${f}#romanized`, r.romanized);
      romaSources.push({ name: `${f}#lines+romanized`, lines: r.lines, roma: r.romanized });
    }
  }
  console.error = quiet;

  const yrcHand: Record<string, string> = {
    kana: "[0,2000](0,500,0)あ(500,500,0)い(1000,500,0)ア(1500,500,0)ー",
    "kanji per char": "[0,3000](0,500,0)女(500,500,0)の(1000,500,0)子(1500,500,0)に(2000,500,0)な(2500,500,0)り(3000,500,0)た(3500,500,0)い",
    "kanji words": "[0,3000](0,1000,0)憧れ(1000,500,0)て(1500,500,0)い(2000,500,0)る\n[4000,2000](4000,1000,0)馬車(5000,500,0)が(5500,500,0)来た",
    "ja spaces": "[0,3000](0,500,0)ねえ(500,500,0)　(1000,500,0)女(1500,500,0)の(2000,500,0)子",
    "ja counting": "[0,3000](0,500,0)一(500,500,0)二(1000,500,0)三(1500,500,0)四(2000,500,0)五",
    "ja particles": "[0,3000](0,500,0)私(500,500,0)は(1000,500,0)学校(1500,500,0)へ(2000,500,0)行く",
    "ja mixed latin": "[0,3000](0,500,0)Hello (500,500,0)世界(1000,500,0)です (1500,500,0)OK",
    "ja join": "[0,3000](0,300,0)にほ(300,300,0)ん(600,300,0)ご",
    "ja long vowel": "[0,2000](0,500,0)ラ(500,500,0)ー(1000,500,0)メ(1500,500,0)ン",
    "ja sokuon": "[0,2000](0,500,0)ちょ(500,500,0)っ(1000,500,0)と(1500,500,0)待って",
    "ja n apostrophe": "[0,2000](0,500,0)こ(500,500,0)ん(1000,500,0)い(1500,500,0)ろ",
    "ko words": "[0,2000](0,500,0)안(500,500,0)녕 (1000,500,0)하(1500,500,0)세",
    "ko whole": "[0,2000](0,1000,0)안녕 (1000,1000,0)하세요",
    "zh per char": "[500,2500](500,300,0)窗(800,300,0)外(1100,300,0)的(1400,300,0)雨",
    "zh words": "[0,2000](0,500,0)你好(500,500,0)世界(1000,500,0)我们",
    "zh latin": "[0,2000](0,500,0)Hi (500,500,0)你(1000,500,0)好(1500,500,0)OK",
    "zh digits": "[0,2000](0,500,0)1(500,500,0)你(1000,500,0)2好",
    "zh poly": "[0,3000](0,500,0)银(500,500,0)行(1000,500,0)行(1500,500,0)走(2000,500,0)重(2500,500,0)要",
    "zh punct": "[0,3000](0,500,0)你(500,500,0)，(1000,500,0)好(1500,500,0)！",
    "zh astral": "[0,2000](0,500,0)𠀀(500,500,0)你(1000,500,0)😀(1500,500,0)好",
    "mixed scripts": "[0,3000](0,500,0)Привет (500,500,0)你好(1000,500,0)안녕(1500,500,0)こんにちは(2000,500,0)مرحبا",
    "latin only": "[0,2000](0,500,0)Hel(500,500,0)lo (1000,500,0)world",
    "cyrillic": "[0,2000](0,500,0)При(500,500,0)вет (1000,500,0)мир",
  };
  for (const [name, text] of Object.entries(yrcHand)) addSet(`hand:yrc:${name}`, parseYrc(text));

  const lrcHand: Record<string, string> = {
    ja: "[00:01.00]ねえ　女の子になりたい\n[00:02.00]お願いいいですか？\n[00:03.00]乗せられて　連れられて\n[00:04.00]みんなきっと憧れている\n[00:05.00]ちょっと話しちゃおう\n[00:06.00]童話に続く入口はもうそこだよ\n[00:07.00]一二三四五\n[00:08.00]私は学校へ行く",
    "ja katakana": "[00:01.00]コンピューター\n[00:02.00]ヴァイオリン\n[00:03.00]ファイト\n[00:04.00]ティーチャー\n[00:05.00]ッ\n[00:06.00]ー\n[00:07.00]んあ\n[00:08.00]ほんいん",
    ko: "[00:01.00]안녕하세요 세계\n[00:02.00]값 닭 읊다\n[00:03.00]ㅋㅋㅋ 한글",
    zh: "[00:01.00]窗外的雨下了一整夜\n[00:02.00]我还在等你说再见\n[00:03.00]风吹过长长的街道\n[00:04.00]行人都已经回家了\n[00:05.00](你听见了吗)\n[00:06.00]我们的故事还没有结束\n[00:07.00]银行行走\n[00:08.00]重要重复",
    mixed: "[00:01.00]Hello 世界\n[00:02.00]안녕 こんにちは 你好\n[00:03.00]Привет мир\n[00:04.00]abc\n[00:05.00]\n[00:06.00]1234",
    latin: "[00:01.00]Hello world\n[00:02.00]Bohemian Rhapsody",
    "gap han": "[00:01.00]你好\n[00:05.00]\n[00:06.00]世界",
  };
  for (const [name, text] of Object.entries(lrcHand)) addSet(`hand:lrc:${name}`, parseLrc(text));

  addSet("hand:empty", []);
  addSet("hand:no-text", [L(1, ""), L(2, " "), L(3, "　")]);
  addSet("hand:gap", [L(1, "你好", { gap: true }), L(2, "こんにちは", { gap: true }), L(3, "世界")]);
  addSet("hand:words-empty-array", [L(1, "你好", { words: [] }), L(2, "かな", { words: [] })]);
  addSet("hand:join-flags", [
    wordLine(1, ["こ", ["ん", { joinNext: true }], ["に", { joinNext: true }], ["ち", { joinNext: true, readJoin: true }], "は"]),
    wordLine(2, [["你", { joinNext: true }], ["好", { joinNext: false }], "世", "界"]),
    wordLine(3, [["안", { joinNext: true }], ["녕", { joinNext: true }], "하", "세", "요"]),
    wordLine(4, [["a", { joinNext: true }], ["b", { joinNext: true, readJoin: false }], ["c", { readJoin: true }], "d"]),
    wordLine(5, [["こ", { joinNext: false, readJoin: true }], ["ん", { joinNext: true, readJoin: true }], "に"]),
    wordLine(6, [["Привет", { joinNext: true }], ["мир", { joinNext: true }], "!"]),
    wordLine(7, [["漢", { joinNext: true }], ["字", { joinNext: true }], "も", ["Ab", { joinNext: true }], "c"]),
  ]);
  addSet("hand:words-whitespace", [
    wordLine(1, ["你", " ", "好", "  ", "a"]),
    wordLine(2, [" ", "こ", " ", "ん"]),
    wordLine(3, ["　", "你", " ", "好"]),
    wordLine(4, ["", "你", "", "好"]),
  ]);
  addSet("hand:text-vs-words", [{ t: 1, text: "different text", words: [W(1, "你"), W(1.5, "好")] }, { t: 2, text: "你好", words: [W(2, "ab"), W(2.5, "cd")] }]);
  const long: LyricLine[] = [];
  for (let i = 0; i < 35; i++) long.push(L(i, i < 33 ? `line ${i}` : "你好"));
  addSet("hand:han-after-30", long);
  addSet("hand:han-at-30", [...long.slice(0, 29), L(30, "你好"), L(31, "x")]);
  addSet("hand:kana-after-30", [...long.slice(0, 30), L(31, "かな")]);
  addSet("hand:hangul-30", [...long.slice(0, 29), L(30, "안녕")]);
  addSet("hand:uroman-only", [L(1, "Привет"), L(2, "Γειά σου"), L(3, "שלום")]);
  addSet("hand:uroman-after-30", [...long.slice(0, 30), L(31, "Привет")]);
  addSet("hand:ja-priority", [L(1, "你好"), L(2, "안녕"), L(3, "か")]);
  addSet("hand:ko-priority", [L(1, "你好"), L(2, "안녕")]);

  // ── texts ────────────────────────────────────────────────
  const texts = new Set<string>(["", " ", "a", "Hello", "你好", "こんにちは", "안녕하세요", "カタカナ", "ｶﾀｶﾅ", "漢字かな交じり", "Привет"]);
  for (const s of sets) for (const l of s.lines) { texts.add(l.text); for (const w of l.words ?? []) texts.add(w.text); }
  for (const t of Object.values(lrcHand)) for (const ln of t.split("\n")) texts.add(ln.replace(/^\[[^\]]*\]/, ""));
  // every hiragana pair and long vowel / sokuon / n combination
  const hira: string[] = [];
  for (let c = 0x3040; c <= 0x30ff; c++) hira.push(String.fromCodePoint(c));
  for (const a of hira) {
    texts.add(a);
    texts.add(a + "ー");
    texts.add("っ" + a);
    texts.add("ん" + a);
    texts.add(a + "ん");
    for (const b of hira.filter((_, i) => i % 3 === 0 || a === "ん" || a === "っ")) texts.add(a + b);
  }
  texts.add("ちょっと待って、きっちり、ほっちゃん、あっっか、っっ、んんあ、んや、んゆ、んよ、んい、おんな、ほんや");
  texts.add("ーあー、あーー、んー、ーー、あっー、ゃ、ゃゃ、かゃ、っゃ");
  texts.add("\u{1F600}か\u{1F600}んあ\u{20000}ー");
  // the whole hangul block, in runs
  const hangul: string[] = [];
  for (let c = 0xac00; c <= 0xd7a3; c++) hangul.push(String.fromCodePoint(c));
  for (let i = 0; i < hangul.length; i += 97) texts.add(hangul.slice(i, i + 97).join(""));
  texts.add("한글 \u{1F600} 가\u{20000}나 ㅋㅋ 힣힢");
  texts.add("안녕 こんにちは 你好 Привет");
  const textRows = [...texts].map((s) => ({
    in: s,
    hasHan: hasHanScript(s),
    nonLatin: hasNonLatinScript(s),
    k2h: katakanaToHiragana(s),
    ja: romanizeJapanese(s),
    ko: romanizeKorean(s),
    kanaHangul: romanizeKanaHangul(s),
    text: romanizeText(s),
  }));

  // ── per line set ─────────────────────────────────────────
  const mutate = (tokens: ReadingToken[][], kind: string): ReadingToken[][] => {
    const copy: ReadingToken[][] = JSON.parse(JSON.stringify(tokens));
    switch (kind) {
      case "noR":
        copy.forEach((l) => l.forEach((t, i) => i % 2 === 0 && delete t.r));
        break;
      case "noPOS":
        copy.forEach((l) => l.forEach((t, i) => i % 3 === 0 && (delete t.p, delete t.d, delete t.b)));
        break;
      case "noFields":
        copy.forEach((l) => l.forEach((t) => (delete t.r, delete t.p, delete t.d, delete t.b)));
        break;
      case "shortTokens":
        copy.length = Math.max(0, copy.length - 2);
        break;
      case "emptyLine":
        copy.forEach((l, i) => i % 4 === 1 && (l.length = 0));
        break;
      case "dropChar":
        copy.forEach((l, i) => {
          const last = l[l.length - 1];
          if (last && i % 3 === 0 && last.s.length > 1) last.s = last.s.slice(0, -1);
        });
        break;
      case "swapOrder":
        copy.forEach((l, i) => i % 5 === 0 && l.reverse());
        break;
      case "mergeAll":
        copy.forEach((l) => l.length && l.splice(0, l.length, { s: l.map((t) => t.s).join(""), r: l.map((t) => t.r ?? "").join("") }));
        break;
      case "splitChars":
        copy.forEach((l, i) =>
          l.splice(0, l.length, ...l.flatMap((t) => (i % 2 ? Array.from(t.s).map((s) => ({ s, ...(t.p ? { p: t.p } : {}) })) : [t]))),
        );
        break;
      case "badReading":
        copy.forEach((l) => l.forEach((t, i) => t.r && i % 2 === 0 && (t.r = "ア")));
        break;
      case "longReading":
        copy.forEach((l) => l.forEach((t) => t.r && (t.r = t.r + t.r)));
        break;
    }
    return copy;
  };
  const variants = ["orig", "noR", "noPOS", "noFields", "shortTokens", "emptyLine", "dropChar", "swapOrder", "mergeAll", "splitChars", "badReading", "longReading"];
  const allTokens = new Map<string, ReadingToken>();
  const setRows = sets.map(({ name, lines }) => {
    const base = kuromojiTokens(readingTexts(lines));
    const readings: Record<string, { tokens: ReadingToken[][]; out: LyricLine[]; roman: LyricLine[] }> = {};
    // the many token variants only on Japanese and hand-made sets; the others get the real tokens when they have non-Latin text
    const hand = name.startsWith("hand:");
    const real = ["orig", "noR", "noPOS", "dropChar", "splitChars"];
    for (const v of hand ? variants : detectLyricLang(lines) === "ja" ? real : lines.some((l) => hasNonLatinScript(l.text)) ? ["orig"] : []) {
      const tokens = v === "orig" ? base : mutate(base, v);
      for (const l of tokens) for (const t of l) allTokens.set(JSON.stringify(t), t);
      const out = applyReadings(lines, tokens);
      readings[v] = { tokens, out, roman: romanizeLines(out) };
    }
    const roman = romanizeLines(lines);
    return {
      name,
      lines,
      need: linesNeedRomanization(lines),
      lang: detectLyricLang(lines) ?? null,
      needReadings: linesNeedReadings(lines),
      readingTexts: readingTexts(lines),
      lineChars: lines.map(lineChars),
      romanizeLines: roman,
      applyPinyin: applyPinyin(lines),
      pinyinRoman: romanizeLines(applyPinyin(lines)),
      readings,
      withRoma: withRomaLines(lines, roman),
    };
  });

  // ── roma alignment ───────────────────────────────────────
  const shift = (ls: LyricLine[], dt: number): LyricLine[] => ls.map((l) => ({ ...l, t: l.t + dt }));
  const alignCases: { name: string; lines: LyricLine[]; roma: LyricLine[]; tol?: number }[] = [];
  for (const r of romaSources) {
    alignCases.push({ name: r.name, lines: r.lines, roma: r.roma });
    alignCases.push({ name: `${r.name}#tol0`, lines: r.lines, roma: r.roma, tol: 0 });
    alignCases.push({ name: `${r.name}#tol3`, lines: r.lines, roma: r.roma, tol: 3 });
    alignCases.push({ name: `${r.name}#shift.5`, lines: r.lines, roma: shift(r.roma, 0.5) });
    alignCases.push({ name: `${r.name}#shift-.7`, lines: r.lines, roma: shift(r.roma, -0.7) });
    alignCases.push({ name: `${r.name}#reversed`, lines: r.lines, roma: [...r.roma].reverse() });
  }
  const bulky = (name: string) => name.startsWith("spicy/");
  for (const s of sets) {
    const roman = romanizeLines(s.lines);
    alignCases.push({ name: `${s.name}#self`, lines: s.lines, roma: roman });
    if (bulky(s.name)) continue;
    alignCases.push({ name: `${s.name}#drop-every-3rd`, lines: s.lines, roma: roman.filter((_, i) => i % 3) });
    alignCases.push({ name: `${s.name}#tol.05+shift.04`, lines: s.lines, roma: shift(roman, 0.04), tol: 0.05 });
  }
  // tolerance edges: roma exactly at, just inside and just outside the tolerance
  const edgeLine = wordLine(10, ["你", "好", "世", "界"]);
  for (const dt of [0, 0.6, -0.6, 0.59999999, 0.60000001, -0.60000001, 0.3, 1, -1]) {
    alignCases.push({ name: `edge dt=${dt}`, lines: [edgeLine], roma: [L(10 + dt, "ni hao shi jie")] });
  }
  const tokenCounts = ["ni", "ni hao", "ni hao shi", "ni hao shi jie", "ni hao shi jie wo", "a b c d e f g h", "  spaced   out  tokens ", "　ni hao\t", "", "   ", "ni hao shi jie wo men de"];
  for (const rt of tokenCounts) alignCases.push({ name: `tokens "${rt}"`, lines: [edgeLine], roma: [L(10, rt)] });
  for (const n of [1, 2, 3, 5, 7]) {
    const words = Array.from({ length: n }, (_, i) => W(1 + i * 0.1, String.fromCharCode(0x4e00 + i)));
    for (const k of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13]) {
      alignCases.push({ name: `spread ${n} words ${k} tokens`, lines: [{ t: 1, text: words.map((w) => w.text).join(""), words }], roma: [L(1, Array.from({ length: k }, (_, i) => `t${i}`).join(" "))] });
    }
  }
  alignCases.push({ name: "no lines", lines: [], roma: [L(1, "x")] });
  alignCases.push({ name: "no roma", lines: [L(1, "x")], roma: [] });
  alignCases.push({ name: "gap line", lines: [L(1, "x", { gap: true }), L(1.1, "y")], roma: [L(1, "a"), L(1.1, "b")] });
  alignCases.push({ name: "line only", lines: [L(1, "你好"), L(2, "世界")], roma: [L(1, " ni hao "), L(2, "shi jie")] });
  alignCases.push({ name: "same time lines", lines: [L(1, "a"), L(1, "b"), L(1, "c")], roma: [L(1, "x"), L(1, "y")] });
  alignCases.push({ name: "roma skipped ahead", lines: [L(5, "a"), L(6, "b")], roma: [L(1, "x"), L(2, "y"), L(5.2, "z")] });
  alignCases.push({
    name: "word flags kept",
    lines: [{ t: 1, end: 3, text: "ab", hideAt: 4, bg: true, opposite: true, words: [W(1, "a", { end: 2, joinNext: true, readJoin: true }), W(2, "b", { end: 3 })] }],
    roma: [L(1, "x y")],
  });
  alignCases.push({ name: "fewer tokens than words empties some", lines: [wordLine(1, ["a", "b", "c", "d", "e"])], roma: [L(1, "x y")] });
  alignCases.push({ name: "all words emptied", lines: [{ t: 1, text: "ab", words: [] }], roma: [L(1, "x")] });
  alignCases.push({ name: "roma line with words", lines: [L(1, "你好")], roma: [wordLine(1, ["ni", "hao"])] });
  const align = alignCases.map((c) => ({
    name: c.name,
    lines: c.lines,
    roma: c.roma,
    ...(c.tol === undefined ? {} : { tol: c.tol }),
    out: c.tol === undefined ? alignRomanizedToWords(c.lines, c.roma) : alignRomanizedToWords(c.lines, c.roma, c.tol),
  }));

  // ── withRomaLines ────────────────────────────────────────
  const withRoma: { name: string; lines: LyricLine[]; roman: LyricLine[]; tol?: number; out: LyricLine[] }[] = [];
  const addWR = (name: string, lines: LyricLine[], roman: LyricLine[], tol?: number) =>
    withRoma.push({ name, lines, roman, ...(tol === undefined ? {} : { tol }), out: tol === undefined ? withRomaLines(lines, roman) : withRomaLines(lines, roman, tol) });
  for (const r of romaSources) {
    addWR(r.name, r.lines, r.roma);
    addWR(`${r.name}#tol0`, r.lines, r.roma, 0);
    addWR(`${r.name}#tol2`, r.lines, r.roma, 2);
  }
  for (const s of sets) {
    const roman = romanizeLines(s.lines);
    addWR(`${s.name}#fewer`, s.lines, roman.slice(1));
    if (bulky(s.name)) continue;
    addWR(`${s.name}#shift`, s.lines, shift(roman, 0.5));
    addWR(`${s.name}#shift-far`, s.lines, shift(roman, 2));
    addWR(`${s.name}#blank`, s.lines, roman.map((l) => ({ ...l, text: "  " })));
    addWR(`${s.name}#same-text`, s.lines, s.lines);
    addWR(`${s.name}#trim-equal`, s.lines, s.lines.map((l) => ({ ...l, text: ` ${l.text} ` })));
  }
  addWR("empty roman", [L(1, "你好"), L(2, "こんにちは")], []);
  addWR("empty lines", [], [L(1, "x")]);
  addWR("gap skipped", [L(1, "你好", { gap: true }), L(2, "你好")], [L(1, "ni hao"), L(2, "ni hao")]);
  addWR("same length different times", [L(1, "你好"), L(2, "世界")], [L(1, "ni hao"), L(2.5, "shi jie")]);
  addWR("roma carries words", [L(1, "你好")], [wordLine(1, ["ni", "hao"])]);

  // ── helpers over tokens, strings and char texts ──────────────
  const handTokens: ReadingToken[] = [
    { s: "憧れ", r: "アコガレ" }, { s: "女", r: "オンナ" }, { s: "馬車", r: "バシャ" }, { s: "馬車", r: "ババ" }, { s: "馬車", r: "バ" },
    { s: "馬車", r: "ばしゃ" }, { s: "来た", r: "キタ" }, { s: "食べる", r: "タベル" }, { s: "食べる", r: "ル" }, { s: "食べる", r: "タ" },
    { s: "一二三", r: "イチニサン" }, { s: "一二三", r: "イチニ" }, { s: "日々", r: "ヒビ" }, { s: "々", r: "ビ" }, { s: "人々", r: "ヒトビト" },
    { s: "漢字", r: "カンジ" }, { s: "漢字", r: "カ" }, { s: "漢字", r: "" }, { s: "漢字" }, { s: "かな", r: "カナ" }, { s: "カナ", r: "カナ" },
    { s: "ドア", r: "ドア" }, { s: "(漢)", r: "(カン)" }, { s: "(漢)", r: "カン" }, { s: "a.b漢", r: "a.bカン" }, { s: "漢+字", r: "カン+ジ" },
    { s: "漢[字]", r: "カン[ジ]" }, { s: "漢^字$", r: "カン^ジ$" }, { s: "漢\\字", r: "カン\\ジ" }, { s: "漢|字", r: "カン|ジ" }, { s: "漢?字*", r: "カン?ジ*" },
    { s: "漢{字}", r: "カン{ジ}" }, { s: "漢-字", r: "カン-ジ" }, { s: "漢 字", r: "カン ジ" }, { s: "漢字", r: "カン\nジ" }, { s: "漢字", r: "カ ン" },
    { s: "漢字", r: "カン\r" }, { s: "漢字", r: "　カ" }, { s: "漢字を", r: "カンジヲ" }, { s: "を漢", r: "ヲカン" }, { s: "あ漢い字う", r: "アカンイジウ" },
    { s: "あ漢い字う", r: "アカンイジ" }, { s: "漢い字", r: "カンイジ" }, { s: "漢い字", r: "カイジ" }, { s: "漢い字", r: "カンジ" },
    { s: "辞書", r: "ジショ" }, { s: "今日", r: "キョウ" }, { s: "今日", r: "キョ" }, { s: "今日は", r: "キョウハ" }, { s: "友達", r: "トモダチ" },
    { s: "𠀀字", r: "ジ" }, { s: "𠀀字", r: "カンジ" }, { s: "😀漢", r: "カン" }, { s: "漢😀", r: "カン😀" }, { s: "", r: "x" }, { s: "", },
    { s: "ー", r: "ー" }, { s: "漢ー", r: "カンー" }, { s: "っ漢", r: "っカン" }, { s: "漢っ", r: "カンっ" }, { s: "漢字", r: "チョッ" }, { s: "漢字", r: "きょう" },
    { s: "三四五", r: "サンヨンゴ" }, { s: "Ａ漢", r: "ＡカンＡ" }, { s: "漢", r: "カンジ" }, { s: "漢", r: "カ" },
    { s: "は", p: "助詞", d: "係助詞", b: "は" }, { s: "へ", p: "助詞", d: "格助詞", b: "へ" }, { s: "を", p: "助詞", d: "格助詞", b: "を" },
    { s: "が", p: "助詞", d: "接続助詞", b: "が" }, { s: "です", p: "助動詞", b: "です" }, { s: "だ", p: "助動詞", b: "だ" }, { s: "た", p: "助動詞", b: "た" },
    { s: "いる", p: "動詞", d: "非自立", b: "いる" }, { s: "ある", p: "動詞", d: "非自立", b: "ある" }, { s: "いく", p: "動詞", d: "非自立", b: "いく" },
    { s: "たち", p: "動詞", d: "接尾", b: "たち" }, { s: "走る", p: "動詞", d: "自立", b: "走る" }, { s: "達", p: "名詞", d: "接尾", b: "達" },
    { s: "女", p: "名詞", d: "一般", b: "女" }, { s: "。", p: "記号", d: "句点", b: "。" }, { s: "、", p: "記号" }, { s: "や", p: "副詞" }, { s: "x", p: "未知" },
    { s: "は", p: "名詞", d: "一般", b: "は" }, { s: "は", p: "助詞" }, { s: "四", p: "名詞", d: "数", b: "四", r: "ヨン" }, { s: "三", p: "名詞", d: "数", b: "三", r: "サン" },
  ];
  const tokenHelpers = [...handTokens, ...allTokens.values()].map((t) => ({ tok: t, attaches: attaches(t), tokenChars: tokenChars(t) }));
  const spaceStrings = ["", " ", "  ", "\t", "\n", " ", "　", "﻿", "\u0085", " ", "​", "a", " a", "a ", "　　", "᠎", "  "];
  const isSpaceRows = spaceStrings.map((s) => ({ in: s, out: isSpace(s) }));
  const kanaStrings = ["", "あ", "ばしゃ", "しゃ", "きょう", "っと", "ちょっと", "ー", "あー", "ラーメン", "っ", "っっ", "あっ", "あっっ", "ゃ", "あゃ", "ゃあ", "っゃ", "かっきょう", "ぁぃぅぇぉゎ", "あぁ", "ふぁいと", "くゎ", "あ😀い", "アイウ", "漢字", "abc"];
  const moraeRows = kanaStrings.map((s) => ({ in: s, out: morae(s) }));
  const spreadRows: { reading: string; n: number; out: string[] }[] = [];
  for (const s of kanaStrings) for (const n of [0, 1, 2, 3, 4, 5, 7]) spreadRows.push({ reading: s, n, out: spread(s, n) });
  const ct = (t: string, roma = false, sp?: boolean): CharText => ({ t, roma, ...(sp === undefined ? {} : { sp }) });
  const charLists: Record<string, CharText[]> = {
    empty: [],
    "roma latin": [ct("ni", true), ct("hao", true)],
    "roma then cjk": [ct("ni", true), ct("，"), ct("hao", true)],
    "latin then roma": [ct("OK"), ct("ni", true)],
    "latin latin": [ct("a"), ct("b")],
    "digits": [ct("1"), ct("2", true), ct("3")],
    "empty t": [ct("a", true), ct("", true), ct("b", true)],
    "roma space": [ct("a", true), ct(" "), ct("b", true)],
    "roma then space char": [ct("ni", true), ct(" ", false), ct("hao", true)],
    "sp": [ct("a", false, true), ct("b", false, true), ct("c", false, false), ct("d", false, true)],
    "sp after space": [ct(" ", false, false), ct("a", false, true)],
    "sp first": [ct("a", false, true)],
    "sp ws end": [ct("a", false, false), ct("　", false, false), ct("b", false, true)],
    "sp nbsp": [ct("a "), ct("b", false, true)],
    "astral": [ct("𠀀"), ct("a", true), ct("b", true), ct("😀")],
    "ü": [ct("lü", true), ct("nǚ", true)],
    "accented": [ct("é"), ct("a", true)],
    "punct": [ct("ni", true), ct("!"), ct("hao", true), ct("?")],
  };
  const joinRows = Object.entries(charLists).map(([name, chars]) => ({ name, chars, out: joinChars(chars) }));
  const withCharsLines: { name: string; line: LyricLine; chars: CharText[]; out: LyricLine }[] = [];
  const addWC = (name: string, line: LyricLine, chars: CharText[]) => withCharsLines.push({ name, line, chars, out: withChars(line, chars) });
  addWC("no words", L(1, "你好"), [ct("ni", true), ct("hao", true)]);
  addWC("empty words", L(1, "你好", { words: [] }), [ct("ni", true), ct("hao", true)]);
  addWC("words unknown", wordLine(1, ["你", "好"]), [ct("ni", true), ct("hao", true)]);
  addWC("words unknown multi", wordLine(1, ["你好", "世界"]), [ct("ni", true), ct("hao", true), ct("shi", true), ct("jie", true)]);
  addWC("words known", wordLine(1, ["あ", "い", "う"]), [ct("あ", false, false), ct("い", false, true), ct("う", false, false)]);
  addWC("words known all sp", wordLine(1, ["あ", "い", "う"]), [ct("あ", false, true), ct("い", false, true), ct("う", false, true)]);
  addWC("words known spaces", wordLine(1, ["あ", " ", "う"]), [ct("あ", false, true), ct(" ", false, false), ct("う", false, true)]);
  addWC("words known leading ws next", wordLine(1, ["あ", "い"]), [ct("あ", false, false), ct(" い", false, false)]);
  addWC("known empty text word", wordLine(1, ["あ", "い", "う"]), [ct("あ", false, false), ct("", false, true), ct("う", false, false)]);
  addWC("known ws-only text", wordLine(1, ["あ", "い"]), [ct(" ", false, false), ct("い", false, true)]);
  addWC("fewer chars than words", wordLine(1, ["あ", "い", "う"]), [ct("あ", false, true)]);
  addWC("more chars than words", wordLine(1, ["あ"]), [ct("あ", false, true), ct("い", false, true)]);
  addWC("word flags", { t: 1, text: "ab", end: 2, hideAt: 3, bg: true, words: [W(1, "a", { end: 1.5, joinNext: true }), W(1.5, "b", { readJoin: false })] }, [ct("a", false, true), ct("b", false, true)]);
  addWC("astral word", wordLine(1, ["𠀀", "a"]), [ct("x", true), ct("y", true)]);
  addWC("multi-char words known", wordLine(1, ["ab", "cd"]), [ct("a", false, true), ct("b", false, false), ct("c", false, true), ct("d", false, false)]);
  addWC("roma words", wordLine(1, ["你", "好"]), [ct("ni", true, true), ct("hao", true, true)]);
  const helpers = { tokens: tokenHelpers, isSpace: isSpaceRows, morae: moraeRows, spread: spreadRows, joinChars: joinRows, withChars: withCharsLines };

  return { texts: textRows, sets: setRows, align, withRoma, helpers };
};
