// Han readings: kanji through kuromoji (the tokens electron/readings.cjs sends),
// hanzi through pinyin-pro, spread back onto the words so timing survives.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { applyPinyin, applyReadings, readingTexts, romanizeLines, withRomaLines } from "../src/lyrics/romanize.ts";
import type { LyricLine } from "../src/lyrics/lyrics.ts";

const require = createRequire(import.meta.url);
const kuromoji = require("kuromoji");
type Tok = { surface_form: string; reading?: string; pos: string; pos_detail_1: string; basic_form: string };
const tokenizer: { tokenize(t: string): Tok[] } = await new Promise((ok, no) =>
  kuromoji.builder({ dicPath: "node_modules/kuromoji/dict" }).build((e: unknown, t: never) => (e ? no(e) : ok(t))),
);
// what electron/readings.cjs answers
const tokensFor = (lines: LyricLine[]) =>
  readingTexts(lines).map((text) =>
    tokenizer.tokenize(text).map((t) => ({
      s: t.surface_form,
      ...(t.reading ? { r: t.reading } : {}),
      p: t.pos,
      d: t.pos_detail_1,
      b: t.basic_form,
    })),
  );
const romaji = (lines: LyricLine[]) => romanizeLines(applyReadings(lines, tokensFor(lines)));

test("kanji romanize through their readings, spaced by word", () => {
  const lines: LyricLine[] = [
    { t: 1, text: "ねえ　女の子になりたい" },
    { t: 2, text: "お願いいいですか？" },
    { t: 3, text: "乗せられて　連れられて" },
    { t: 4, text: "みんなきっと憧れている" },
    { t: 5, text: "ちょっと話しちゃおう" },
    { t: 6, text: "童話に続く入口はもうそこだよ" },
  ];
  assert.deepEqual(romaji(lines).map((l) => l.text), [
    "nee onnanoko ni naritai",
    "onegai ii desu ka？",
    "noserarete tsurerarete",
    "minna kitto akogarete iru",
    "chotto hanashichaou",
    "douwa ni tsuzuku iriguchi wa mou soko da yo",
  ]);
});

test("kanji split across timed syllables keep one reading each", () => {
  const w = (text: string, t: number) => ({ text, t, end: t + 0.2 });
  const line: LyricLine = { t: 0, text: "女の子 馬車", words: [w("女", 0), w("の", 1), w("子", 2), w("馬", 3), w("車", 4)] };
  const out = romaji([line])[0];
  assert.deepEqual(
    out.words!.map((x) => [x.text, x.t]),
    [["onna", 0], ["no", 1], ["ko", 2], ["ba", 3], ["sha", 4]],
  );
  // one word each, so the syllables glue: "onnanoko basha"
  assert.equal(out.text, "onnanoko basha");
});

test("a line the tokens do not cover is left alone", () => {
  const line: LyricLine = { t: 0, text: "夢に見た" };
  assert.equal(applyReadings([line], [[{ s: "夢", r: "ユメ" }]])[0], line);
});

test("hanzi romanize to pinyin, spaced", () => {
  const out = romanizeLines(applyPinyin([{ t: 0, text: "听见你的声音 ok" }]));
  assert.equal(out[0].text, "ting jian ni de sheng yin ok");
});

test("Both keeps the line and puts its romanization under it", () => {
  const lines: LyricLine[] = [{ t: 0, text: "きみ" }, { t: 1, text: "hello" }];
  const out = withRomaLines(lines, romanizeLines(lines));
  assert.deepEqual(out.map((l) => [l.text, l.roma?.text]), [["きみ", "kimi"], ["hello", undefined]]);
});

test("Latin-only word-timed lines are not re-segmented or given a roma line", () => {
  const w = (text: string, t: number) => ({ text, t, end: t + 0.2 });
  const lines: LyricLine[] = [
    {
      t: 0,
      text: "Close your eyes, you'll be here soon",
      words: [
        w("Close", 0),
        w("your", 1),
        w("eyes,", 2),
        w("you'll", 3),
        w("be", 4),
        w("here", 5),
        w("soon", 6),
      ],
    },
    { t: 7, text: "一二三四五分" },
  ];
  const out = withRomaLines(lines, romaji(lines));
  assert.equal(out[0].roma, undefined);
  assert.equal(out[1].roma?.text, "ichi ni san shi gofun");
});
