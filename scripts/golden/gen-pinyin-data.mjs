// Exports pinyin-pro's dictionaries (the only ones `pinyin(text, { type: "all", toneType: "none" })`
// reads with surname off) to src-gpui/logic/data/, which src/pinyin.rs loads with include_str!.
//   pinyin-dict1.json   DICT1: char -> its readings, space separated (first is the default)
//   pinyin-words.json   word patterns in PatternsNormal order (dict5, dict4, dict3, dict2, number rules),
//                       one group per dict: { probability, words: [[zh, pinyin], ...] }
// The surname patterns are left out: they have priority Surname, which the "off" setting skips.
//   node scripts/golden/gen-pinyin-data.mjs
import { writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const esm = join(root, "node_modules/pinyin-pro/dist/esm");
const out = join(root, "src-gpui/logic/data");
const imp = (p) => import(pathToFileURL(join(esm, p)).href);

const { default: DICT1 } = await imp("data/dict1.mjs");
const dict1 = {};
DICT1.NumberDICT.forEach((py, code) => {
  dict1[String.fromCharCode(code)] = py;
});
for (const [ch, py] of DICT1.StringDICT) dict1[ch] = py;

const { Pattern5 } = await imp("data/dict5.mjs");
const { Pattern4 } = await imp("data/dict4.mjs");
const { Pattern3 } = await imp("data/dict3.mjs");
const { Pattern2 } = await imp("data/dict2.mjs");
const { PatternNumberDict } = await imp("data/special.mjs");
const groups = [Pattern5, Pattern4, Pattern3, Pattern2, PatternNumberDict].map((g) => {
  const probability = g[0].probability;
  for (const p of g) {
    if (p.probability !== probability || p.priority !== 1) throw new Error("non-uniform group");
    if ([...p.zh].length !== p.length) throw new Error("length mismatch " + p.zh);
  }
  return { probability, words: g.map((p) => [p.zh, p.pinyin]) };
});
for (const k of Object.keys(dict1)) if ([...k].length !== 1) throw new Error("bad key");

writeFileSync(join(out, "pinyin-dict1.json"), JSON.stringify(dict1) + "\n");
writeFileSync(join(out, "pinyin-words.json"), JSON.stringify(groups) + "\n");
copyFileSync(join(root, "node_modules/pinyin-pro/LICENSE"), join(out, "pinyin-pro-LICENSE.txt"));
console.log(Object.keys(dict1).length, "chars;", groups.map((g) => g.words.length).join("/"), "words");
