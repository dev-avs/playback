// golden/lyrics.json: src/lyrics/lyrics.ts over the lrclib, netease and edge fixtures plus hand-made edge cases.
// loadLyrics runs against a fake window.player whose fetchLyrics returns the recorded `res`.
import type { Fixtures } from "../run.mts";
import {
  SELF_DELIMITING,
  asideWordIndex,
  charsIn,
  estimateRate,
  estimateWordTimings,
  extractCredits,
  isCreditLine,
  loadLyrics,
  markParentheticalBackingVocals,
  parseLrc,
  parseMmSs,
  parseYrc,
  splitTrailingAside,
  stripBrackets,
  type LyricLine,
  type LyricWord,
} from "../../../src/lyrics/lyrics.ts";

const L = (t: number, text: string, extra: Partial<LyricLine> = {}): LyricLine => ({ t, text, ...extra });
const W = (t: number, text: string, extra: Partial<LyricWord> = {}): LyricWord => ({ t, text, ...extra });

export default async (fx: Fixtures) => {
  // ── inputs ───────────────────────────────────────────────
  const lrclib = fx.list("lrclib").map((f) => ({ f, j: fx.json(f) }));
  const netease = fx.list("netease").map((f) => ({ f, j: fx.json(f) }));
  const edge = ["backing.lrc", "chinese.lrc", "empty.lrc", "gaps.lrc"].map((f) => ({
    f: `edge/${f}`,
    text: fx.text(`edge/${f}`),
  }));

  const lrcs: { name: string; in: string }[] = [];
  for (const { f, j } of lrclib) {
    lrcs.push({ name: f, in: j.syncedLyrics ?? "" });
    lrcs.push({ name: `${f}#plain`, in: j.plainLyrics ?? "" });
  }
  for (const { f, j } of netease) {
    lrcs.push({ name: `${f}#lrc`, in: j.lrc?.lyric ?? "" });
    lrcs.push({ name: `${f}#tlyric`, in: j.tlyric?.lyric ?? "" });
    lrcs.push({ name: `${f}#romalrc`, in: j.romalrc?.lyric ?? "" });
  }
  for (const { f, text } of edge) {
    lrcs.push({ name: f, in: text });
    lrcs.push({ name: `${f}#crlf`, in: text.replace(/\n/g, "\r\n") });
  }
  lrcs.push({ name: "edge/plain.txt", in: fx.text("edge/plain.txt") });
  const hand: Record<string, string> = {
    empty: "",
    "no brackets": "hello\nworld",
    "enhanced": "[00:01.00] <00:01.00>Hello <00:01.50>big <00:02.25>world\n[00:04.00]<00:04.00>Next<00:04.50>line",
    "enhanced no space": "[00:01.00]<00:01.00>Hel<00:01.20>lo",
    "enhanced empty words": "[00:01.00]<00:01.00>   <00:02.00>",
    "enhanced tag only": "[00:01.00]<00:01.00>",
    "stray tags": "[00:01.00] <b>bold</b> text <>x< y\n[00:02.00]<<>>a\n[00:03.00]a<b",
    "repeated": "[00:12.00][00:45.00]chorus\n[00:20.00]verse\n[00:30.00]",
    "repeated with space": "[00:12.00] [00:45.00]  chorus  ",
    "same time": "[00:10.00]a\n[00:10.00]b\n[00:10.00]c\n[00:10.00]",
    "end marks": "[00:00.50]\n[00:01.00]one\n[00:03.00]\n[00:03.50]\n[00:04.00]two\n[00:09.00]three\n[00:20.00]",
    "end mark before next": "[00:01.00]one\n[00:02.00]\n[00:02.50]two",
    "end mark equals next": "[00:01.00]one\n[00:02.00]\n[00:02.00]two",
    "offset": "[offset:+500]\n[ti:x]\n[00:01.00]a\n[00:02.00]b",
    "odd stamps": "[1:2]a\n[0:0]b\n[00:00]c\n[99:59.999]d\n[007:08.5]e",
    "huge minutes": "[99999999999999999999:01]x\n[00:01.123456789012345]y",
    "bad stamps": "[00:01.]a\n[00:.5]b\n[:01]c\n[00:01]\n[ 00:01]d\n[00;01]e\n[00:01.5.5]f\n[ar:x]g",
    "prefix text": "abc [00:01.00]x\nfoo[00:02.00][00:03.00] y [00:04.00]",
    "stamp in body": "[00:01.00]a [00:02.00] b",
    "unicode ws": "[00:01.00]\u00a0\u3000hello\u00a0\ufeff\n[00:02.00]\u0085x\u0085",
    "line separators": "[00:01.00]one\u2028two\n[00:02.00]\u2028three\n[00:03.00]\u2029\n[00:04.00]\rfour\rfive",
    "cr only": "[00:01.00]a\r[00:02.00]b",
    "stamp then cr": "[00:01.00]\r\n[00:02.00]b\r\n",
    "cjk": "[00:01.00]窗外的雨下了一整夜\n[00:02.00]안녕하세요 세계\n[00:03.00]مرحبا بالعالم\n[00:04.00]😀 emoji 👍🏽",
    "tabs and spaces": "[00:01.00]\t\ttabbed\t\n[00:02.00]  \n[00:03.00]x",
    "unsorted": "[00:30.00]c\n[00:10.00]a\n[00:20.00]b",
    "no trailing newline": "[00:01.00]a",
    "only newlines": "\n\n\n",
    "enhanced then plain": "[00:01.00]<00:01.00>a <00:02.00>b\n[00:03.00]plain",
    "enhanced two stamps": "[00:01.00][00:05.00]<00:01.00>a <00:02.00>b",
    "word tag no digits": "[00:01.00]<a:b>x <00:02.00>y",
  };
  for (const [name, text] of Object.entries(hand)) lrcs.push({ name: `hand:${name}`, in: text });

  const yrcs: { name: string; in: string }[] = [];
  for (const { f, j } of netease) yrcs.push({ name: `${f}#yrc`, in: j.yrc?.lyric ?? "" });
  const yhand: Record<string, string> = {
    empty: "",
    "latin fused": "[1000,2000](1000,300,0)Hel(1300,300,0)lo (1600,400,0)wor(2000,500,0)ld",
    "latin spaced": "[0,3000](0,500,0)One (500,500,0)two (1000,500,0)three",
    "trailing buffer": "[0,3000](0,500,0)One (500,500,0)tw(1000,500,0)o",
    cjk: "[500,2500](500,300,0)窗(800,300,0)外(1100,300,0)的(1400,300,0)雨",
    hangul: "[0,2000](0,500,0)안(500,500,0)녕 (1000,500,0)하(1500,500,0)세",
    kana: "[0,2000](0,500,0)あ(500,500,0)い(1000,500,0)ア(1500,500,0)ー",
    mixed: "[0,2000](0,500,0)Hi(500,500,0)あ(1000,500,0)b(1500,500,0)c ",
    "empty tokens": "[0,1000](0,100,0)(100,100,0) (200,100,0)x",
    "only spaces": "[0,1000](0,100,0) (100,100,0)  ",
    "no tokens": "[0,1000]just text",
    "json meta": '{"t":0,"c":[{"tx":"作词: x"}]}\n[0,1000](0,100,0)hi',
    crlf: "[0,1000](0,100,0)hi\r\n[1000,1000](1000,100,0)yo\r\n",
    "cr middle": "[0,1000](0,100,0)hi\rthere",
    unsorted: "[5000,1000](5000,100,0)b\n[1000,1000](1000,100,0)a",
    "same start": "[1000,1000](1000,100,0)a\n[1000,500](1000,100,0)b",
    astral: "[0,1000](0,100,0)😀(100,100,0)x (200,100,0)y😀",
    "nbsp": "[0,1000](0,100,0)a\u00a0(100,100,0)b\u3000(200,100,0)c",
    "big numbers": "[99999999999999999999,5](12345678901234567,3,0)x",
    "nested parens": "[0,1000](0,100,0)a (b)(100,100,0)c",
    "no close": "[0,1000](0,100)x(100,100,0",
    "decimal-ish": "[0.5,1000](0,100,0)x",
  };
  for (const [name, text] of Object.entries(yhand)) yrcs.push({ name: `hand:${name}`, in: text });

  const creditTexts = [
    "", "   ", "作词 : x", "作词: x", "作词：", "作词 :x", "作词", "OP:x", "op : x", "Sp\u3000: y", "Written by Foo", "written  by",
    "Written byte", "Written by", "WRITTEN BY\tx", "written\u00a0by x", "Lyrics by me", "lyrics by", "Music by:", "mixed by Z",
    "混音助理 : abc", "混音助理 :", "混音助理 ： ", "一二三四五六七八 : x", "一二三四五六七八九 : x", "一: x", "x: y", "Hello: world",
    "Estelle, songwriters", "x, Producer", "x,producer ", "x, co-producers", "x, producerz", "x , mixer", "a, b, writer", "writer",
    "will.i.am, Estelle, John Legend, songwriters", "Vocal: x", "Vocals: x", "Vocalz: x", "Keyboard : k", "Keyboards：k", "Remix:",
    "Remix: a", "Mixing :a", "mastering: a", "MASTERING: a", "制作人 : a", "制作 : a", "制 : a", "制作人员 : a", "词：a", "曲:a",
    "\u00a0作词：a", "\ufeff作词：a", "\u0085作词：a", "\u2028作词：a", "Strings\n: a", "Bass\t:\tb", "Ｂass: x", "ſtrings: x", "ıP: x", "K: x",
    "Piano: ", "Piano:x", "Lyricist :x", "LYRICIST：x", "composed  by x", "Composed by", "producedby x", "music  by\nx", "mixed by_x",
    "recorded by-x", "performed by.", "arranged by 你", "一二 ：  \u3000x", "一二 ： \u3000", "𠀀: x", "Ditto - NewJeans", "뉴진스",
    "Is this the real life", "(Ooh, ooh)", "Bohemian Rhapsody - Queen",
  ];
  for (const l of [...lrcs, ...yrcs]) {
    for (const ln of l.in.split("\n")) creditTexts.push(ln);
  }
  const creditUniq = [...new Set(creditTexts)];

  const lineSets: { name: string; in: LyricLine[] }[] = [];
  for (const l of lrcs) lineSets.push({ name: l.name, in: parseLrc(l.in) });
  for (const l of yrcs) lineSets.push({ name: l.name, in: parseYrc(l.in) });

  const rep = (n: number, f: (i: number) => LyricLine) => Array.from({ length: n }, (_, i) => f(i));
  const hset: Record<string, LyricLine[]> = {
    empty: [],
    "no credits": [L(1, "a b"), L(2, "c d"), L(3, "e f")],
    "header with title gap": [L(0, "Ditto - NewJeans"), L(1, "뉴진스"), L(2, "词：A"), L(3, "曲：B"), L(4, "编曲：C"), L(5, "Real lyric"), L(6, "more")],
    "header gap too big": [L(0, "t1"), L(1, "t2"), L(2, "t3"), L(3, "作词：A"), L(4, "Real lyric")],
    "header gap exactly 2": [L(0, "t1"), L(1, "t2"), L(2, "作词：A"), L(3, "Real lyric")],
    "header run resets": [L(0, "作词：A"), L(1, "t"), L(2, "t"), L(3, "作曲：B"), L(4, "t"), L(5, "t"), L(6, "t"), L(7, "作曲：Z"), L(8, "real")],
    "trailing block": [L(0, "real one"), L(1, "real two"), L(2, "器乐 : a"), L(3, "音频工程师 : b"), L(4, "混音助理 : c")],
    "trailing and header": [L(0, "作词：A"), L(1, "x y"), L(2, "z"), L(3, "混音工程师 : c")],
    "only credits": [L(0, "作词：A"), L(1, "作曲：B")],
    "short song closing credits": [L(0, "only line"), L(1, "混音 : x")],
    "trailing 15 limit": rep(30, (i) => L(i, i < 5 ? "real" : `混音${i} : x`)),
    "trailing exactly 15": [L(0, "real"), ...rep(15, (i) => L(i + 1, `混音${i} : x`))],
    "trailing 16 contiguous": rep(16, (i) => L(i, `混音${i} : x`)),
    "credit in middle": [L(0, "a"), L(1, "b"), L(2, "c"), L(3, "词：x"), L(4, "d"), L(5, "e")],
    "header at index 9": [...rep(9, (i) => L(i, i % 3 === 2 ? "词：x" : "hello")), L(9, "作词：y"), L(10, "real"), L(11, "real2"), L(12, "real3")],
    "header at index 10": [...rep(10, (i) => L(i, i < 8 ? (i % 2 ? "hello world" : "作词：q") : "t")), L(10, "作词：y"), L(11, "real")],
    "with words": [L(0, "作词 : A", { words: [W(0, "作词", { end: 0.5 })] }), L(1, "real", { end: 3 })],
    "credit text spaces": [L(0, "  作词 : A  "), L(1, "x")],
  };
  for (const [name, l] of Object.entries(hset)) lineSets.push({ name: `hand:${name}`, in: l });

  // backing vocals
  const aside = (text: string, words?: LyricWord[], extra: Partial<LyricLine> = {}) =>
    L(5, text, { ...(words ? { words } : {}), ...extra });
  const bset: Record<string, LyricLine[]> = {
    wholly: [L(1, "lead"), L(2, "(back)"), L(3, "[back2]"), L(4, "(a) b (c)"), L(5, "(ab)"), L(6, "()"), L(7, "(a)"), L(8, "[x y]")],
    "opens on bracket": [L(1, "(first)"), L(2, "(second)"), L(3, "lead"), L(4, "(third)")],
    "closes early": [L(1, "lead"), L(2, "(a) b (c)"), L(3, "[a] b]"), L(4, "[[a]]"), L(5, "((a))")],
    "gap and bg flags": [L(0, "(x)", { gap: true }), L(1, "(y)", { bg: true }), L(2, "(z)", { gap: false }), L(3, "lead", { bg: false }), L(4, "(w)", { bg: false })],
    "trailing aside": [aside("But think of you? (But think of you?)"), aside("say it (say it)"), aside("a (b)"), aside("a (bc)"), aside("(a) (bc)")],
    "aside edge": [
      aside("..., (yeah yeah)"), aside("lead ( yeah )"), aside("lead (  )"), aside("lead (x)"), aside("lead(yeah)"), aside("lead (a(b))"),
      aside("lead (a) (bc)"), aside("lead\n(bc)"), aside("lead\u2028x (bc)"), aside("a\n (bc)"), aside("lead (bc))"), aside("lead ((bc)"),
      aside("1 (ok)"), aside("٣ (ok)"), aside("Ⅳ (ok)"), aside("ⓐ (ok)"), aside("— (ok)"), aside("😀 (😀)"), aside("a (😀)"), aside("a (\u{1d7d8}\u{1d7d8})"),
      aside("\u00a0lead  (bc)\u3000"), aside("你好 (哈哈)"), aside("x (ab)(cd)"), aside(" (ab)"), aside("a (b\nc)"), aside("a (bc) "),
    ],
    "aside with words": [
      aside("But think (of you)", [W(0, "But", { end: 1 }), W(1, "think", { end: 2 }), W(2, "(of", { end: 3 }), W(3, "you)", { end: 4 })], { end: 5 }),
      aside("lead (aside)", [W(0, "lead", { end: 1 }), W(1, "(aside)", { end: 2 })]),
      aside("lead (aside)", [W(0, "lead"), W(1, "(aside)")], { end: 9 }),
      aside("lead (aside)", [W(0, "lead", { end: 1, joinNext: true }), W(1, "(", { end: 2 }), W(2, "aside", { end: 3, readJoin: true }), W(3, ")", { end: 4 })]),
      aside("lead (aside)", [W(0, "lead (a"), W(1, "side)")]),
      aside("lead (aside)", [W(0, "(lead"), W(1, "(aside)")]),
      aside("lead (aside)", [W(0, "lead"), W(1, "(((aside)))")]),
      aside("lead (aside)", [W(0, "lead"), W(1, "((")]),
      aside("lead (aside)", [W(0, "lead"), W(1, "(a"), W(2, "b)"), W(3, "c)")]),
      aside("lead (aside)", [W(0, "lead"), W(1, "(ab)"), W(2, "x")]),
      aside("lead (aside)", [W(0, "lead"), W(1, "(aside)")], { bg: false, gap: false, opposite: true, hideAt: 3, roma: L(5, "roma", { words: [W(5, "r")] }) }),
      aside("lead (aside)", []),
      aside("lead (aside)", [W(0, "x"), W(1, "(")]),
    ],
  };
  for (const [name, l] of Object.entries(bset)) lineSets.push({ name: `backing:${name}`, in: l });

  const splitLines: LyricLine[] = Object.values(bset).flat();

  const wordSets: { name: string; in: LyricWord[] }[] = [
    { name: "empty", in: [] },
    { name: "one", in: [W(0, "(a)")] },
    { name: "parens only", in: [W(0, "(("), W(1, "x"), W(2, "))")] },
    { name: "strip all", in: [W(0, "((("), W(1, ")))")] },
    { name: "mid parens", in: [W(0, "a"), W(1, "(b)"), W(2, "c)")] },
    { name: "keeps fields", in: [W(0, "(a", { end: 1, joinNext: true, readJoin: true }), W(1, "b)", { end: 2 })] },
    { name: "aside shape 1", in: [W(0, "a"), W(1, "(b"), W(2, "c)")] },
    { name: "aside shape 2", in: [W(0, "(a"), W(1, "b)")] },
    { name: "no aside", in: [W(0, "a"), W(1, "b"), W(2, "c")] },
    { name: "open no close", in: [W(0, "a"), W(1, "(b"), W(2, "c")] },
    { name: "second opens", in: [W(0, "x"), W(1, "(a"), W(2, "(b)")] },
  ];

  // ── estimator inputs ─────────────────────────────────────
  const eset: Record<string, LyricLine[]> = {
    empty: [],
    single: [L(1, "one two three")],
    "single word": [L(1, "word"), L(5, "word2")],
    "no end": [L(0, "hello there my friend"), L(3, "second line is here"), L(7, "third line goes on and on")],
    "with end": [L(0, "hello there my friend", { end: 2.5 }), L(3, "second line is here", { end: 6 }), L(10, "x y", { end: 10.1 })],
    "end before start": [L(5, "a b c", { end: 4 }), L(6, "d e f")],
    "long span": [L(0, "a b c d e f g h", { end: 100 }), L(200, "x y")],
    "last line": [L(0, "the last line has no next")],
    "words present": [L(0, "a b", { words: [W(0, "a"), W(1, "b")] }), L(2, "c d", { words: [] }), L(4, "e f")],
    "tabs": [L(0, "a\t\tb\n c\u3000d\u00a0e"), L(4, " lead trailing ")],
    "unicode": [L(0, "你好 世界 😀 a😀b"), L(5, "مرحبا بالعالم"), L(9, "안녕 하세요")],
    "tight gaps": [L(0, "abcd efgh"), L(0.3, "abcd efgh"), L(0.5, "abcd efgh"), L(9, "abcd efgh"), L(20, "abcd efgh")],
    "many samples": rep(37, (i) => L(i * 3.1 + (i % 5) * 0.37, `word${i} another${i % 7} x`)),
    "zero gap": [L(0, "a b c d"), L(0, "e f g h")],
    "tiny span": [L(0, "a b", { end: 0.2 }), L(1, "c d", { end: 1.2000001 })],
  };
  for (const [name, l] of Object.entries(eset)) lineSets.push({ name: `est:${name}`, in: l });

  // ── outputs ──────────────────────────────────────────────
  const selfDelimiting: { in: string; out: boolean }[] = [];
  const probes = [
    0x20, 0x41, 0x30ff, 0x3040, 0x309f, 0x30a0, 0x3100, 0x33ff, 0x3400, 0x4dbf, 0x4dc0, 0x4dff, 0x4e00, 0x9fff, 0xa000, 0xf8ff, 0xf900, 0xfaff, 0xfb00,
    0xabff, 0xac00, 0xd7af, 0xd7b0, 0x1100, 0x3130, 0x20000, 0x1f600, 0x10000, 0xe000, 0xff66,
  ];
  for (const cp of probes) {
    const s = String.fromCodePoint(cp);
    const unit = s[s.length - 1];
    selfDelimiting.push({ in: s, out: SELF_DELIMITING.test(unit) });
  }

  const mmss: [string, string][] = [
    ["0", "0"], ["1", "2.5"], ["12", "34.567"], ["99", "99.99"], ["00", "00.000"], ["007", "1"], ["1", "60"], ["3", "7.125"],
    ["10", "0.1"], ["59", "59.999"], ["1", "0.30000000000000004"], ["999999", "0.5"], ["99999999999999999999", "1"], ["1", "123456789012345678"],
    ["0", "0.1234567890123456789"], ["2", "0.7"], ["0", "01.10"],
  ];

  const loadCases: { name: string; bridge: boolean; res: unknown; out: unknown }[] = [];
  const addLoad = async (name: string, res: unknown, bridge = true) => {
    (globalThis as any).window = bridge ? { player: { fetchLyrics: async () => res } } : {};
    const out = await loadLyrics({ artist: "A", title: "T" });
    loadCases.push({ name, bridge, res, out });
  };
  await addLoad("no bridge", null, false);
  await addLoad("null response", null);
  await addLoad("undefined-ish empty object", { source: "lrclib" });
  for (const { f, j } of lrclib) {
    await addLoad(`${f} synced`, { syncedLyrics: j.syncedLyrics, plainLyrics: j.plainLyrics, source: "lrclib" });
    await addLoad(`${f} plain only`, { plainLyrics: j.plainLyrics, source: "lrclib" });
    await addLoad(`${f} synced as null`, { syncedLyrics: null, plainLyrics: j.plainLyrics, source: "lrclib" });
  }
  for (const { f, j } of netease) {
    await addLoad(`${f} yrc+lrc`, { wordLyrics: j.yrc?.lyric, syncedLyrics: j.lrc?.lyric, source: "netease" });
    await addLoad(`${f} lrc only`, { syncedLyrics: j.lrc?.lyric, source: "netease" });
    await addLoad(`${f} lrcmux`, { wordLyrics: j.yrc?.lyric, source: "lrcmux" });
  }
  for (const { f, text } of edge) {
    await addLoad(`${f} synced`, { syncedLyrics: text, source: "lrclib" });
    await addLoad(`${f} as word`, { wordLyrics: text, source: "lrcmux" });
  }
  await addLoad("empty strings", { wordLyrics: "", syncedLyrics: "", plainLyrics: "x", source: "lrclib" });
  await addLoad("word lines all credits falls back to synced", {
    wordLyrics: "[0,1000](0,500,0)作词 : x\n[1000,1000](1000,500,0)作曲 : y",
    syncedLyrics: "[00:01.00]hello there\n[00:05.00]second line",
    source: "netease",
  });
  await addLoad("word garbage falls back to synced", {
    wordLyrics: "not yrc at all",
    syncedLyrics: "[00:01.00]hello there\n[00:05.00]second line",
    source: "netease",
  });
  await addLoad("synced only credits", { syncedLyrics: "[00:01.00]作词 : x\n[00:02.00]作曲 : y", source: "lrclib" });
  await addLoad("synced unparseable", { syncedLyrics: "just text", source: "lrclib" });
  await addLoad("synced enhanced", { syncedLyrics: "[00:01.00]<00:01.00>a <00:02.00>b\n[00:04.00]plain line here", source: "lrclib" });
  await addLoad("synced enhanced all", { syncedLyrics: "[00:01.00]<00:01.00>a <00:02.00>b\n[00:04.00]<00:04.00>c <00:05.00>d", source: "lrclib" });
  await addLoad("synced single words", { syncedLyrics: "[00:01.00]one\n[00:05.00]two\n[00:09.00]", source: "lrclib" });
  await addLoad("synced with header and trailer", {
    syncedLyrics: "[00:00.00]Title - Artist\n[00:00.10]作词 : a\n[00:00.20]作曲 : b\n[00:10.00]first real line here\n[00:15.00]second line (echo here)\n[00:20.00]混音 : z",
    source: "netease",
  });
  await addLoad("word with aside", {
    wordLyrics: "[0,3000](0,500,0)Hello (500,500,0)there (1000,500,0)(echo (1500,500,0)echo)",
    source: "netease",
  });

  return {
    parseMmSs: mmss.map(([a, b]) => ({ in: [a, b], out: parseMmSs(a, b) })),
    isCreditLine: creditUniq.map((s) => ({ in: s, out: isCreditLine(s) })),
    selfDelimiting,
    parseLrc: lrcs.map((c) => ({ name: c.name, in: c.in, out: parseLrc(c.in) })),
    parseYrc: yrcs.map((c) => ({ name: c.name, in: c.in, out: parseYrc(c.in) })),
    extractCredits: lineSets.map((c) => ({ name: c.name, in: c.in, out: extractCredits(c.in) })),
    markParentheticalBackingVocals: lineSets.map((c) => ({ name: c.name, in: c.in, out: markParentheticalBackingVocals(c.in) })),
    splitTrailingAside: splitLines.map((l) => ({ in: l, out: splitTrailingAside(l) })),
    asideWordIndex: wordSets.map((w) => ({ name: w.name, in: w.in, out: asideWordIndex(w.in) })),
    stripBrackets: wordSets.map((w) => ({ name: w.name, in: w.in, out: stripBrackets(w.in) })),
    charsIn: creditUniq.map((s) => ({ in: s, out: charsIn(s) })),
    estimateRate: lineSets.map((c) => ({ name: c.name, in: c.in, out: estimateRate(c.in) })),
    estimateWordTimings: lineSets.map((c) => ({ name: c.name, in: c.in, out: estimateWordTimings(c.in) })),
    loadLyrics: loadCases,
  };
};
