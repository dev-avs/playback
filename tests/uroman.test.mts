// Expected values are uroman's own output (python uroman 1.3.1.1, 86a196e), so
// these check the port in src/uroman.ts against the original (except the silent
// final he, which is ours).
import { test } from "node:test";
import assert from "node:assert/strict";
import { uromanize, hasUromanScript } from "../src/uroman.ts";
import { romanizeText, romanizeLines, linesNeedRomanization } from "../src/romanize.ts";

const cases: [string, string][] = [
  ["הָבָה נָגִילָה וְנִשְׂמְחָה", "hava nagila venismecha"],
  ["حبيبي يا نور العين", "hbyby ya nwr al'yn"],
  ["Привет, как дела?", "Privet, kak dela?"],
  ["Καλημέρα κόσμε", "Kalemera kosme"],
  ["สวัสดีครับ", "swatdiikrap"],
  ["नमस्ते दुनिया", "namaste duniyaa"],
  ["ভালোবাসি", "bhaalobaasi"],
  ["ﻻ", "la"],
];

test("matches uroman across scripts", () => {
  for (const [src, want] of cases) assert.equal(uromanize(src), want, src);
});

test("leaves Latin and CJK alone", () => {
  assert.equal(uromanize("Hava Nagila, ä"), "Hava Nagila, ä");
  assert.equal(hasUromanScript("愛してる 사랑해"), false);
});

test("romanizeText runs uroman after kana and hangul", () => {
  assert.equal(romanizeText("사랑 Привет"), "sarang Privet");
  assert.equal(linesNeedRomanization([{ t: 0, text: "הָבָה" }]), true);
  assert.equal(linesNeedRomanization([{ t: 0, text: "hello" }]), false);
});

test("final he after a vowel is silent", () => {
  assert.equal(uromanize("הָבָה נָגִילָה"), "hava nagila");
});

test("glued syllables romanize as one word", () => {
  const [line] = romanizeLines([
    {
      t: 0,
      text: "נָגִילָה",
      words: [
        { t: 0, text: "נָגִי", joinNext: true },
        { t: 1, text: "לָה" },
      ],
    },
  ]);
  assert.deepEqual(line.words?.map((w) => w.text), ["nagi", "la"]);
  assert.equal(line.text, "nagila");
});
