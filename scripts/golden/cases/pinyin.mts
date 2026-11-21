// golden/pinyin.json: pinyin-pro's `pinyin(text, { type: "all", toneType: "none" })` (the one call romanize.ts makes),
// reduced to the fields romanize.ts reads (origin, pinyin, isZh), over lyric lines from every fixture, hand-made Han
// text, every word pattern of the dictionaries, runs of the single-char dictionary and seeded random mixes.
import { pinyin } from "pinyin-pro";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { Fixtures } from "../run.mts";

const here = dirname(fileURLToPath(import.meta.url));
const data = (f: string) => JSON.parse(readFileSync(join(here, "../../../src-gpui/logic/data", f), "utf8"));

export function pinyinCases(fx: Fixtures): string[] {
  const out = new Set<string>();
  const add = (s: string) => out.add(s);
  const strings = (v: unknown, f: (s: string) => void) => {
    if (typeof v === "string") f(v);
    else if (Array.isArray(v)) v.forEach((x) => strings(x, f));
    else if (v && typeof v === "object") Object.values(v).forEach((x) => strings(x, f));
  };
  const lineOf = (s: string) => {
    for (const ln of s.split(/\r?\n/)) add(ln.replace(/^(\[[^\]]*\])+/, "").replace(/\(\d+,\d+,\d+\)/g, ""));
  };
  for (const dir of ["lrclib", "netease", "spicy", "spicy-edge"]) {
    for (const f of fx.list(dir)) strings(fx.json(f), lineOf);
  }
  for (const f of fx.list("edge")) lineOf(fx.text(f));
  lineOf(fx.text("uroman/lines.txt"));

  // hand-made
  [
    "", " ", "  ", "\t", "\n", "a", "Hello world", "123", "!?，。、；：「」", "你好", "你好，世界", "你 好", "你好 world 你好", "abc你好def",
    "行", "银行", "行走", "银行行走", "我行我素", "重", "重要", "重复", "重庆", "长", "长大", "长度", "校长", "了", "了解", "我走了", "吃了饭",
    "的", "我的", "的确", "目的", "还", "还有", "还钱", "还是", "好", "好看", "爱好", "为", "为什么", "因为", "没", "没有", "沉没", "着", "看着",
    "着急", "得", "觉得", "得到", "跑得快", "地", "土地", "慢慢地", "都", "首都", "都是", "乐", "音乐", "快乐", "和", "和平", "和面", "会", "会计", "开会",
    "一", "一个", "一样", "一起", "一定", "一二三", "一一", "说一说", "看一看", "不", "不是", "不对", "不好", "不不", "说不说", "好不好", "一不", "不一",
    "一的", "不的", "一还", "不还", "不是的", "十一", "第一", "一十", "统一", "唯一", "万一", "一万", "二", "三四五", "四", "一二三四", "一三四",
    "零一", "〇一", "一十一", "一行", "一重", "三行", "两行", "百更", "千斗",
    "々", "人々", "時々", "々々", "日々夜々", "abc々", "々人", "。々",
    "謝謝你", "愛你", "體驗", "國家", "學習", "電腦", "龍", "鳥", "歲月", "們",
    "龘", "䶿", "㐀", "𠀀", "𠀁你好", "你𠀀好", "𪚥", "𫝆", "𰻞", "𰻞𰻞面", "😀", "你😀好", "a😀b", "👍🏽", "\u{20000}\u{2A6DF}",
    "窗外的雨下了一整夜", "我还在等你说再见", "风吹过长长的街道", "行人都已经回家了", "(你听见了吗)", "我们的故事还没有结束",
    "为什么", "实际上", "巴尔干半岛", "布尔什维克", "何乐而不为", "苛政猛于虎", "蒙得维的亚",
    "こんにちは世界", "憧れ", "안녕", "ABC 汉字 123", "汉字,汉字.汉字", "汉　字", "汉 字", "汉́字", "ü", "女", "绿", "略", "嗯", "欸", "呒", "姆",
    "不 好", "一 个", "了 解", "好\n看", "a一b", "a不b", "1一2", "上一个不对的人", "不管三七二十一", "一不做二不休", "一模一样", "一心一意", "不见不散",
    "不一样", "一不小心", "说不定", "来不及", "看不见", "好不好吃", "一重要", "一行行", "行行行", "重重", "长长长", "还还", "了了", "的的", "着着",
    "乐乐", "朝朝朝落", "解放思想", "曲意逢迎", "难为情", "为主", "增长", "任务", "皇上", "这个", "那个", "称为", "作为",
  ].forEach(add);

  // every word pattern, bare and with a one-char neighbour on each side
  const groups: { words: [string, string][] }[] = data("pinyin-words.json");
  const dict1: Record<string, string> = data("pinyin-dict1.json");
  const sides = ["一", "不", "了", "的", "A", "，", "好", "𠀀"];
  let k = 0;
  for (const g of groups) {
    for (const [zh] of g.words) {
      add(zh);
      add(sides[k % sides.length] + zh);
      add(zh + sides[(k + 3) % sides.length]);
      k++;
    }
  }
  // consecutive runs of the single-char dictionary
  const chars = Object.keys(dict1);
  for (let i = 0; i < chars.length; i += 40) add(chars.slice(i, i + 40).join(""));
  // 一/不 followed by every third char (tone sandhi), and between pairs
  chars.forEach((c, i) => {
    if (i % 3) return;
    add("一" + c);
    add("不" + c);
    add(c + "一" + chars[(i + 7) % chars.length]);
  });

  // seeded random mixes of words, chars, Latin, digits, punctuation, spaces, astral
  let seed = 12345;
  const rnd = (n: number) => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed % n;
  };
  const words = groups.flatMap((g) => g.words.map((w) => w[0]));
  const misc = ["a", "Z", "7", " ", ",", "。", "-", "é", "😀", "𠀀", "々", "一", "不", "了", "的", "\n", "ー", "あ"];
  for (let i = 0; i < 3000; i++) {
    let s = "";
    const n = 1 + rnd(12);
    for (let j = 0; j < n; j++) {
      const r = rnd(10);
      s += r < 3 ? words[rnd(words.length)] : r < 8 ? chars[rnd(chars.length)] : misc[rnd(misc.length)];
    }
    add(s);
  }
  return [...out];
}

export default (fx: Fixtures) =>
  pinyinCases(fx).map((s) => ({
    in: s,
    // one string per element: origin, pinyin, isZh (1/0) joined by tabs (split from the right)
    out: (pinyin(s, { type: "all", toneType: "none" }) as { origin: string; pinyin: string; isZh: boolean }[]).map(
      (c) => `${c.origin}	${c.pinyin}	${c.isZh ? 1 : 0}`,
    ),
  }));
