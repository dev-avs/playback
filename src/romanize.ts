import type { LyricLine } from "./lyrics";
import { hasUromanScript, uromanize, uromanizeParts } from "./uroman.ts";

// ── Script detection ──────────────────────────────────────────
const HIRAGANA = /[぀-ゟ]/;
const KATAKANA = /[゠-ヿ]/;
const HANGUL = /[가-힣]/;
const HAN = /[一-鿿㐀-䶿]/;

export function hasNonLatinScript(text: string): boolean {
  return (
    HIRAGANA.test(text) || KATAKANA.test(text) || HANGUL.test(text) || HAN.test(text)
  );
}

/**
 * True when the text contains Han characters, which have no algorithmic
 * reading — romanizing them needs a dictionary, so `romanizeText` leaves them
 * alone and the caller has to fall back to a source-supplied transliteration.
 */
export function hasHanScript(text: string): boolean {
  return HAN.test(text);
}

export function linesNeedRomanization(lines: LyricLine[]): boolean {
  // sample the start rather than scanning an entire song
  return lines.slice(0, 30).some((l) => hasNonLatinScript(l.text) || hasUromanScript(l.text));
}

/**
 * BCP-47 tag for the lyrics, or undefined when they are Latin.
 *
 * Han characters are shared between Chinese and Japanese but are *drawn*
 * differently in each, and a font stack cannot express that: the browser takes
 * the first family covering the codepoint, so a Japanese face listed ahead of a
 * Chinese one renders Chinese lyrics in Japanese letterforms. Tagging the
 * element lets :lang() pick the right face per song instead.
 *
 * Kana and hangul are decisive, so they are checked first; Han on its own means
 * Chinese. Sampled over the same 30 lines as linesNeedRomanization, since one
 * stray character should not retag a whole song.
 */
export function detectLyricLang(lines: LyricLine[]): "ja" | "ko" | "zh" | undefined {
  const sample = lines
    .slice(0, 30)
    .map((l) => l.text)
    .join("");
  if (HIRAGANA.test(sample) || KATAKANA.test(sample)) return "ja";
  if (HANGUL.test(sample)) return "ko";
  if (HAN.test(sample)) return "zh";
  return undefined;
}

// ── Japanese kana → romaji (modified Hepburn) ─────────────────
// Kana only. Kanji has no algorithmic reading — it needs a dictionary, so it
// is passed through untouched; the NetEase `romalrc` source covers that case.

const DIGRAPHS: Record<string, string> = {
  きゃ: "kya", きゅ: "kyu", きょ: "kyo",
  ぎゃ: "gya", ぎゅ: "gyu", ぎょ: "gyo",
  しゃ: "sha", しゅ: "shu", しょ: "sho", しぇ: "she",
  じゃ: "ja", じゅ: "ju", じょ: "jo", じぇ: "je",
  ちゃ: "cha", ちゅ: "chu", ちょ: "cho", ちぇ: "che",
  ぢゃ: "ja", ぢゅ: "ju", ぢょ: "jo",
  にゃ: "nya", にゅ: "nyu", にょ: "nyo",
  ひゃ: "hya", ひゅ: "hyu", ひょ: "hyo",
  びゃ: "bya", びゅ: "byu", びょ: "byo",
  ぴゃ: "pya", ぴゅ: "pyu", ぴょ: "pyo",
  みゃ: "mya", みゅ: "myu", みょ: "myo",
  りゃ: "rya", りゅ: "ryu", りょ: "ryo",
  ふぁ: "fa", ふぃ: "fi", ふぇ: "fe", ふぉ: "fo",
  うぃ: "wi", うぇ: "we", うぉ: "wo",
  ゔぁ: "va", ゔぃ: "vi", ゔぇ: "ve", ゔぉ: "vo",
  てぃ: "ti", でぃ: "di", とぅ: "tu", どぅ: "du",
};

const KANA: Record<string, string> = {
  あ: "a", い: "i", う: "u", え: "e", お: "o",
  か: "ka", き: "ki", く: "ku", け: "ke", こ: "ko",
  が: "ga", ぎ: "gi", ぐ: "gu", げ: "ge", ご: "go",
  さ: "sa", し: "shi", す: "su", せ: "se", そ: "so",
  ざ: "za", じ: "ji", ず: "zu", ぜ: "ze", ぞ: "zo",
  た: "ta", ち: "chi", つ: "tsu", て: "te", と: "to",
  だ: "da", ぢ: "ji", づ: "zu", で: "de", ど: "do",
  な: "na", に: "ni", ぬ: "nu", ね: "ne", の: "no",
  は: "ha", ひ: "hi", ふ: "fu", へ: "he", ほ: "ho",
  ば: "ba", び: "bi", ぶ: "bu", べ: "be", ぼ: "bo",
  ぱ: "pa", ぴ: "pi", ぷ: "pu", ぺ: "pe", ぽ: "po",
  ま: "ma", み: "mi", む: "mu", め: "me", も: "mo",
  や: "ya", ゆ: "yu", よ: "yo",
  ら: "ra", り: "ri", る: "ru", れ: "re", ろ: "ro",
  わ: "wa", ゐ: "wi", ゑ: "we", を: "o", ん: "n",
  ゔ: "vu",
  ぁ: "a", ぃ: "i", ぅ: "u", ぇ: "e", ぉ: "o",
  ゃ: "ya", ゅ: "yu", ょ: "yo",
};

const VOWELS = new Set(["a", "i", "u", "e", "o"]);

function katakanaToHiragana(text: string): string {
  return text.replace(/[ァ-ヶ]/g, (c) =>
    String.fromCharCode(c.charCodeAt(0) - 0x60),
  );
}

function romanizeJapanese(input: string): string {
  const text = katakanaToHiragana(input);
  let out = "";
  let sokuon = false; // っ pending: doubles the next consonant

  for (let i = 0; i < text.length; i++) {
    const pair = text.slice(i, i + 2);
    let romaji: string | undefined = DIGRAPHS[pair];
    if (romaji) {
      i++;
    } else if (text[i] === "っ") {
      sokuon = true;
      continue;
    } else if (text[i] === "ー" || text[i] === "ー") {
      // chōonpu: hold the preceding vowel
      const last = out[out.length - 1];
      if (last && VOWELS.has(last)) out += last;
      continue;
    } else {
      romaji = KANA[text[i]];
    }

    if (!romaji) {
      // not kana (kanji, latin, punctuation) — leave it alone
      out += text[i];
      sokuon = false;
      continue;
    }

    if (sokuon) {
      // っち → tchi, everything else doubles its initial consonant
      out += romaji.startsWith("ch") ? "t" : romaji[0];
      sokuon = false;
    }

    // ん needs an apostrophe before a vowel or y, or it reads as one syllable
    if (
      out.endsWith("n") &&
      text[i] !== "ん" &&
      (VOWELS.has(romaji[0]) || romaji[0] === "y") &&
      KANA[text[i - 1]] === "n"
    ) {
      out += "'";
    }

    out += romaji;
  }

  return out;
}

// ── Korean hangul → Revised Romanization ──────────────────────
// Fully algorithmic: each syllable block decomposes into initial/medial/final.
// Inter-syllable assimilation is not applied, which is fine for reading along.

const INITIALS = [
  "g", "kk", "n", "d", "tt", "r", "m", "b", "pp",
  "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h",
];

const MEDIALS = [
  "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa",
  "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i",
];

const FINALS = [
  "", "k", "k", "ks", "n", "nj", "nh", "t", "l", "lg", "lm",
  "lb", "ls", "lt", "lp", "lh", "m", "p", "ps", "s", "ss",
  "ng", "j", "ch", "k", "t", "p", "h",
];

function romanizeKorean(text: string): string {
  let out = "";
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (code < 0xac00 || code > 0xd7a3) {
      out += ch;
      continue;
    }
    const n = code - 0xac00;
    out += INITIALS[Math.floor(n / 588)];
    out += MEDIALS[Math.floor((n % 588) / 28)];
    out += FINALS[n % 28];
  }
  return out;
}

// ── Entry points ──────────────────────────────────────────────
function romanizeKanaHangul(text: string): string {
  let out = text;
  if (HANGUL.test(out)) out = romanizeKorean(out);
  if (HIRAGANA.test(out) || KATAKANA.test(out)) out = romanizeJapanese(out);
  return out;
}

export function romanizeText(text: string): string {
  // every other non-Latin script (Arabic, Hebrew, Cyrillic, Thai, Devanagari…)
  return uromanize(romanizeKanaHangul(text));
}

/**
 * Lay a source-supplied romanization over lines that already have real timings.
 *
 * NetEase ships `romalrc` as plain line-timed LRC, so taking it wholesale threw
 * away whatever word timings the lyrics had — a syllable-timed Japanese track
 * dropped to a line sweep the moment romanization was switched on. Kanji and Han
 * have no algorithmic reading, so that track is the only way to romanize them,
 * but its *timing* is the worst on offer.
 *
 * Spicy-lyrics never faces this: its payload carries TransliteratedText on each
 * syllable, so the romanized text inherits the syllable's own window. This does
 * the same thing after the fact — it keeps the source's windows and only swaps
 * the text into them.
 *
 * Lines are paired by timestamp within a tolerance, and each romanized line's
 * tokens are spread across that line's words. An exact count maps one to one,
 * which is the common case for Han (one reading per character); otherwise the
 * tokens are shared out proportionally, which keeps the fill moving with the
 * voice even when the split is not clean.
 */
export function alignRomanizedToWords(
  lines: LyricLine[],
  roma: LyricLine[],
  toleranceS = 0.6,
): LyricLine[] {
  if (!lines.length || !roma.length) return lines;

  let j = 0;
  return lines.map((line) => {
    if (line.gap) return line;
    while (j < roma.length && roma[j].t < line.t - toleranceS) j++;

    const match = roma[j];
    if (!match || Math.abs(match.t - line.t) > toleranceS) return line;
    j++;

    const text = match.text.trim();
    if (!text) return line;

    const words = line.words;
    if (!words?.length) return { ...line, text };

    const tokens = text.split(/\s+/).filter(Boolean);
    if (!tokens.length) return line;

    const per = tokens.length / words.length;
    const next = words
      .map((w, i) => ({
        ...w,
        text: tokens
          .slice(Math.round(i * per), Math.round((i + 1) * per))
          .join(" "),
        // The romanized run is spaced Latin text, so nothing glues together.
        joinNext: false,
      }))
      .filter((w) => w.text);

    if (!next.length) return { ...line, text };
    return { ...line, text: next.map((w) => w.text).join(" "), words: next };
  });
}

/**
 * Romanize in place, keeping every timestamp so word-level highlighting still
 * lines up with playback.
 */
export function romanizeLines(lines: LyricLine[]): LyricLine[] {
  return lines.map((line) => {
    if (!line.words?.length) return { ...line, text: romanizeText(line.text) };
    // Each romanized CJK unit stands apart (一二三 → "ichi ni san"); glued, they
    // ran together into one unreadable word.
    const texts = line.words.map((w) => romanizeKanaHangul(w.text));
    // uroman's rules look at whole words, so glued syllables go in together
    for (let a = 0; a < texts.length; ) {
      let b = a + 1;
      while (b < texts.length && line.words[b - 1].joinNext) b++;
      texts.splice(a, b - a, ...uromanizeParts(texts.slice(a, b)));
      a = b;
    }
    const words = line.words.map((w, i) => ({
      ...w,
      text: texts[i],
      joinNext: w.joinNext && !hasNonLatinScript(w.text),
    }));
    let text = "";
    for (let i = 0; i < words.length; i++) {
      text += words[i].text;
      if (i < words.length - 1 && !words[i].joinNext) text += " ";
    }
    return { ...line, text, words };
  });
}
