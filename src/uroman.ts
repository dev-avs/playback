import { UROMAN_DATA } from "./vendor/uroman-data.ts";

/**
 * Universal romanizer: a port of uroman's lattice romanizer
 * (https://github.com/isi-nlp/uroman, uroman/uroman.py — "This project uses the
 * universal romanizer software 'uroman' written by Ulf Hermjakob, USC
 * Information Sciences Institute (2015-2020)").
 *
 * It covers the scripts romanize.ts has no hand-written rules for — Arabic,
 * Hebrew, Cyrillic, Greek, Thai, the Indic abugidas and the rest. Its data is
 * uroman's own tables, exported by scripts/uroman-data.py. Ported: rule lookup
 * with word-position restrictions, the abugida default vowel, virama, Thai
 * vowel placement and syllable-final forms, and the fallbacks. Left out: number
 * parsing (digits map one by one), Braille, Tibetan, Coptic, Han / kana / hangul
 * (romanize.ts) and language-specific rules (no language code is known here).
 * Unvowelled Arabic and Hebrew come out as consonants, as they do in uroman.
 */

export type UromanData = {
  /** source string → [target, restriction flags, language codes?, target at syllable end?] */
  rules: Record<string, [string, number, string[]?, string?][]>;
  /** abugida script → its default vowels */
  abugida: Record<string, string[]>;
  /** script → its characters, for the abugidas and Thai only */
  scripts: Record<string, string>;
  vowelSign: string;
  medial: string;
  virama: string;
  toneMark: string;
  /** Thai vowels written before the consonant but spoken after it */
  prePost: string;
  nukta: string;
  subjoined: string;
  vocalic: string;
  /** digits of every script, grouped by value 0–9 */
  digits: string[];
};

const ONLY_START = 1;
const DONT_START = 2;
const ONLY_END = 4;
const DONT_END = 8;
const WHOLE_WORD = 16;

type Rule = { t: string; flags: number; nRestr: number; lcodes: string[]; tEos?: string };

let tables:
  | {
      rules: Map<string, Rule[]>;
      prefix: Set<string>;
      script: Map<string, string>;
      abugida: Record<string, string[]>;
      vowelSign: Set<string>;
      medial: Set<string>;
      virama: Set<string>;
      toneMark: Set<string>;
      prePost: Set<string>;
      nukta: Set<string>;
      subjoined: Set<string>;
      vocalic: Set<string>;
      digit: Map<string, string>;
    }
  | undefined;

function load() {
  if (tables) return tables;
  const d = UROMAN_DATA;
  const rules = new Map<string, Rule[]>();
  const prefix = new Set<string>();
  for (const [s, rs] of Object.entries(d.rules)) {
    rules.set(
      s,
      rs.map(([t, flags, lcodes = [], tEos]) => {
        let nRestr = lcodes.length ? 1 : 0;
        for (let f = flags; f; f &= f - 1) nRestr++;
        return { t, flags, nRestr, lcodes, tEos };
      }),
    );
    const cs = Array.from(s);
    for (let i = 1; i <= cs.length; i++) prefix.add(cs.slice(0, i).join(""));
  }
  const script = new Map<string, string>();
  for (const [name, chars] of Object.entries(d.scripts)) for (const c of chars) script.set(c, name);
  const set = (chars: string) => new Set(Array.from(chars));
  const digit = new Map<string, string>();
  d.digits.forEach((chars, value) => {
    for (const c of chars) digit.set(c, String(value));
  });
  tables = {
    rules,
    prefix,
    script,
    abugida: d.abugida,
    vowelSign: set(d.vowelSign),
    medial: set(d.medial),
    virama: set(d.virama),
    toneMark: set(d.toneMark),
    prePost: set(d.prePost),
    nukta: set(d.nukta),
    subjoined: set(d.subjoined),
    vocalic: set(d.vocalic),
    digit,
  };
  return tables;
}

/** Letters in scripts this romanizer handles (not Latin, not CJK). */
const UROMAN_SCRIPT =
  /[^\P{L}\p{Script=Latin}\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;

export function hasUromanScript(text: string): boolean {
  return UROMAN_SCRIPT.test(text);
}

const isAlpha = (c: string | undefined) => !!c && /^\p{L}/u.test(c);
const isLetterOrMark = (c: string | undefined) => !!c && /^[\p{L}\p{M}]/u.test(c);
const isUpper = (s: string) => s !== s.toLowerCase() && s === s.toUpperCase();
const isLower = (s: string) => s !== s.toUpperCase() && s === s.toLowerCase();
const isMn = (c: string | undefined) => !!c && /^\p{Mn}$/u.test(c);
const CONSONANTS = /^[bcdfghjklmnpqrstvwxyz]+$/;

type Edge = { start: number; end: number; txt: string; type: string };

class Lattice {
  private t = load();
  private s: string[];
  private n: number;
  private spans = new Map<string, Edge[]>();
  private rights = new Map<number, Set<number>>();
  private lefts = new Map<number, Set<number>>();
  private precededByAlpha = new Map<number, boolean>();
  private followedByAlpha = new Map<number, boolean>();
  private topCache = new Map<string, string>();

  constructor(text: string) {
    this.s = Array.from(text);
    this.n = this.s.length;
  }

  private sub(a: number, b: number) {
    return this.s.slice(a, b).join("");
  }

  private scriptOf(c: string | undefined) {
    return (c && this.t.script.get(c)) || "";
  }

  private addEdge(e: Edge) {
    const key = `${e.start}-${e.end}`;
    const list = this.spans.get(key) ?? [];
    if (!list.some((o) => o.txt === e.txt && o.type === e.type)) list.push(e);
    this.spans.set(key, list);
    if (!this.rights.has(e.start)) this.rights.set(e.start, new Set());
    this.rights.get(e.start)!.add(e.end);
    if (!this.lefts.has(e.end)) this.lefts.set(e.end, new Set());
    this.lefts.get(e.end)!.add(e.start);
  }

  private edges(a: number, b: number) {
    return this.spans.get(`${a}-${b}`) ?? [];
  }

  private isAtStartOfWord(position: number): boolean {
    const cached = this.precededByAlpha.get(position);
    if (cached !== undefined) return !cached;
    if (position && isAlpha(this.s[position - 1])) {
      this.precededByAlpha.set(position, true);
      return false;
    }
    for (const start of this.lefts.get(position) ?? []) {
      for (const e of this.edges(start, position)) {
        if (e.txt && isAlpha(e.txt[e.txt.length - 1])) {
          this.precededByAlpha.set(position, true);
          return false;
        }
      }
    }
    this.precededByAlpha.set(position, false);
    return true;
  }

  private isAtEndOfWord(position: number): boolean {
    const cached = this.followedByAlpha.get(position);
    if (cached !== undefined) return !cached;
    let start = position;
    if (start < this.n && isAlpha(this.s[start])) {
      this.followedByAlpha.set(position, true);
      return false;
    }
    while (start + 1 < this.n && isMn(this.s[start]) && this.t.nukta.has(this.s[start])) start++;
    for (let end = start + 1; end <= this.n; end++) {
      const sub = this.sub(start, end);
      if (!this.t.prefix.has(sub)) break;
      for (const r of this.t.rules.get(sub) ?? []) {
        if (!(r.flags & ONLY_START) && /\p{L}/u.test(r.t)) {
          this.followedByAlpha.set(position, true);
          return false;
        }
      }
    }
    this.followedByAlpha.set(position, false);
    return true;
  }

  private isAtEndOfSyllable(position: number): boolean {
    const s = this.s;
    const prev = position >= 2 ? s[position - 2] : undefined;
    let next = position < this.n ? s[position] : undefined;
    let adj = position;
    if (next && this.t.toneMark.has(next)) {
      adj = position + 1;
      next = adj < this.n ? s[adj] : undefined;
    }
    if (prev === undefined) return false;
    if (!isLetterOrMark(prev)) return false;
    if (this.t.prePost.has(prev)) return false;
    if (next && this.t.prePost.has(next)) return true;
    if (adj >= this.n) return true;
    if (!isLetterOrMark(next)) return true;
    if (position > 0) {
      const left = this.bestLeftNeighbor(position - 1);
      if (left && /[bcdfghjklmnpqrstvxz]$/.test(left.txt)) return false;
    }
    const nextRom =
      this.topCandidate(adj, adj + 2, true) ?? this.topCandidate(adj, adj + 1, true) ?? "?";
    if (!/^[aeiou]/.test(nextRom.toLowerCase())) return true;
    if (next === "อ" && adj + 1 < this.n) {
      // THAI CHARACTER O ANG: a consonant when a vowel follows
      const next2Rom = this.topCandidate(adj + 1, adj + 2, true) ?? "?";
      if (/^[aeiou]/.test(next2Rom.toLowerCase())) return true;
    }
    return false;
  }

  private firstRule(c: string) {
    return this.t.rules.get(c)?.[0]?.t;
  }

  private valid(r: Rule, start: number, end: number) {
    if (r.flags & DONT_START && this.isAtStartOfWord(start)) return false;
    if (r.flags & ONLY_START && !this.isAtStartOfWord(start)) return false;
    if (r.flags & DONT_END && this.isAtEndOfWord(end)) return false;
    if (r.flags & ONLY_END && !this.isAtEndOfWord(end)) return false;
    if (r.flags & WHOLE_WORD && !(this.isAtStartOfWord(start) && this.isAtEndOfWord(end)))
      return false;
    // no language code is known, so language-specific rules never apply
    return !r.lcodes.length;
  }

  private topCandidate(start: number, end: number, simple = false): string | undefined {
    if (start < 0 || end > this.n) return undefined;
    const key = `${start}-${end}`;
    const cached = this.topCache.get(key);
    if (cached !== undefined) return cached;
    let best: Rule | undefined;
    for (const r of this.t.rules.get(this.sub(start, end)) ?? []) {
      if (this.valid(r, start, end) && (!best || r.nRestr > best.nRestr)) best = r;
    }
    if (simple) return best?.t;
    let cand = best?.t;
    if (best?.tEos !== undefined && this.isAtEndOfSyllable(end)) cand = best.tEos;
    if (cand !== undefined) this.topCache.set(key, cand);
    return cand;
  }

  private addAbugidaVowel(rom: string, start: number, end: number, annotation: string): string {
    const s = this.s;
    const scriptName = this.scriptOf(s[start]);
    const vowels = this.t.abugida[scriptName];
    if (!vowels?.length) return rom;
    let base: string | undefined;
    let basePlus: string;
    const m1 = rom.match(new RegExp(`^([cfghkmnqrstxy]?y)(${vowels.map((v) => v + "+").join("|")})-?$`));
    const m2 = rom.match(new RegExp(`^([bcdfghjklmnpqrstvwxyz]+)(${vowels.join("|")})-?$`));
    if (m1) {
      base = m1[1];
      basePlus = base + m1[2];
    } else if (m2) {
      base = m2[1];
      basePlus = base + m2[2];
      if (rom.endsWith("-") && start + 1 === end && isAlpha(rom[0])) rom = rom.slice(0, -1);
    } else {
      base = rom;
      basePlus = base + vowels[0];
    }
    if (!CONSONANTS.test(base)) return rom;
    if (annotation.includes("tail")) return rom;
    const prev = start >= 1 ? s[start - 1] : "";
    const next = s[end] ?? "";
    const next2 = s[end + 1] ?? "";
    if (next && ("bcdfghklmnpqrstvwz".includes(base) || base === "ng") && next === "យ") return base;
    if (this.t.vowelSign.has(next)) return base;
    if (this.t.medial.has(next)) return base;
    if (this.t.subjoined.has(next)) return base;
    if (isMn(next) && this.t.vowelSign.has(next2)) return base;
    if (this.t.virama.has(next)) return base;
    if (isMn(next) && this.t.virama.has(next2)) return base;
    if (this.t.virama.has(prev)) return basePlus;
    if (this.isAtStartOfWord(start) && !/r[aeiou]/.test(rom)) return basePlus;
    // Most Devanagari languages (and Assamese, Bengali, Gujarati, Kashmiri,
    // Punjabi) drop the final schwa; without a language code that is the rule
    // for Devanagari only, as in uroman.
    if (this.isAtEndOfWord(end)) return scriptName === "Devanagari" ? rom : basePlus;
    if (this.scriptOf(prev) !== scriptName) return basePlus;
    if (this.t.vocalic.has(s[end - 1])) return base;
    if (this.scriptOf(next) === scriptName) return basePlus;
    return rom;
  }

  private expand(rom: string, start: number, end: number): [string, number, number, string?] {
    const s = this.s;
    if (rom === "") return [rom, start, end];
    let prev = start >= 1 ? s[start - 1] : "";
    let first = s[start];
    const next = end < this.n ? s[end] : "";
    // THessalonike → Thessalonike, but THESSALONIKE stays
    if (start + 1 === end && isUpper(rom) && isLower(next)) {
      rom = rom[0].toUpperCase() + rom.slice(1).toLowerCase();
    }
    // Gurmukhi addak doubles the next consonant
    const dbl = prev === "ੱ" ? rom.match(/^(ch|[bcdfghjklmnpqrstwz])/) : null;
    if (dbl) {
      rom = dbl[1].replace("ch", "c") + rom;
      start--;
      first = s[start];
      prev = start >= 1 ? s[start - 1] : "";
    }
    if (this.scriptOf(first) === "Thai") {
      if (start + 1 === end && CONSONANTS.test(rom) && this.t.prePost.has(prev) && start >= 1) {
        for (const suffix of [3, 2, 1]) {
          if (end + suffix > this.n) continue;
          const pattern = s[start - 1] + "–" + this.sub(end, end + suffix);
          const vowel = this.t.rules.get(pattern)?.[0];
          if (vowel) return [rom + vowel.t, start - 1, end + suffix, "rom exp"];
        }
      }
      const vowelRom = this.scriptOf(prev) === "Thai" && this.t.prePost.has(prev) && this.firstRule(prev);
      if (vowelRom && /^[bcdfghjklmnpqrstvwxyz]/.test(rom)) {
        return [rom + vowelRom, start - 1, end, "rom exp"];
      }
      if (first === "อ" && end - start === 1) {
        const prevRom = this.pathBackwards(start, 1);
        const nextRom = this.firstRule(next) ?? "";
        const consonant = /^[bcdfghjklmnpqrstvwxz]+$/;
        if (
          !(
            this.scriptOf(prev) === "Thai" &&
            this.scriptOf(next) === "Thai" &&
            consonant.test(prevRom) &&
            consonant.test(nextRom)
          )
        ) {
          return ["", start, end, "rom del"];
        }
      }
    }
    if (this.t.virama.has(next)) return [rom, start, end + 1, "rom exp"];
    if (rom.startsWith(" ") && (start === 0 || prev === " ")) rom = rom.slice(1);
    if (rom.endsWith(" ") && next === " ") rom = rom.slice(0, -1);
    return [rom, start, end];
  }

  private addRomanization() {
    for (let start = 0; start < this.n; start++) {
      for (let end = start + 1; end <= this.n; end++) {
        if (!this.t.prefix.has(this.sub(start, end))) break;
        let rom = this.topCandidate(start, end);
        if (rom === undefined) continue;
        let annotation = "rom";
        if (/^\+(m|ng|n|h|r)/.test(rom)) {
          rom = rom.slice(1);
          annotation = "rom tail";
        }
        rom = this.addAbugidaVowel(rom, start, end, annotation);
        const [txt, a, b, exp] = this.expand(rom, start, end);
        this.addEdge({ start: a, end: b, txt, type: exp ?? annotation });
      }
    }
  }

  private addFallbackSingles() {
    for (let start = 0; start < this.n; start++) {
      const end = start + 1;
      if (this.edges(start, end).length) continue;
      const c = this.s[start];
      let rom = c;
      let type = "orig";
      if (/^[\p{Mn}\p{Cf}\p{Co}]$/u.test(c)) {
        rom = "";
        type = "mark";
      } else if (this.t.digit.has(c)) {
        rom = this.t.digit.get(c)!;
      } else if (c !== " ") {
        const single = this.topCandidate(start, end);
        if (single !== undefined) {
          rom = single.replace(/^\+(m|ng|n|h|r)/, "$1");
          type = "rom single";
        }
      }
      this.addEdge({ start, end, txt: rom, type });
    }
  }

  private bestInSpan(a: number, b: number): Edge | undefined {
    const list = this.edges(a, b);
    return list.find((e) => e.type.startsWith("rom")) ?? list[0];
  }

  private bestRightNeighbor(start: number) {
    for (const end of [...(this.rights.get(start) ?? [])].sort((x, y) => y - x)) {
      const e = this.bestInSpan(start, end);
      if (e) return e;
    }
    return undefined;
  }

  private bestLeftNeighbor(end: number) {
    for (const start of [...(this.lefts.get(end) ?? [])].sort((x, y) => x - y)) {
      const e = this.bestInSpan(start, end);
      if (e) return e;
    }
    return undefined;
  }

  private pathBackwards(end: number, minChar: number): string {
    let rom = "";
    let end2 = end;
    while (end2 > 0) {
      const old = end2;
      const e = this.bestLeftNeighbor(end2);
      if (e) {
        rom = e.txt + rom;
        end2 = e.start;
      }
      if (rom.length >= minChar) break;
      if (old >= end2) end2--;
    }
    return rom;
  }

  /** The best path through the lattice, as edges over code point offsets. */
  romanize(): Edge[] {
    this.addRomanization();
    this.addFallbackSingles();
    const path: Edge[] = [];
    for (let i = 0; i < this.n; ) {
      const e = this.bestRightNeighbor(i);
      if (!e) {
        i++;
        continue;
      }
      path.push(e);
      i = e.end;
    }
    return path;
  }
}

// Arabic presentation forms (ﻼ and friends) fold to the letters they draw;
// uroman reaches the same letters through their Unicode decomposition.
const PRESENTATION_FORMS = new RegExp("[" + String.fromCharCode(0xfb50) + "-" + String.fromCharCode(0xfdff) + String.fromCharCode(0xfe70) + "-" + String.fromCharCode(0xfeff) + "]", "g");

// A Hebrew word-final he after a vowel point is silent (הָבָה is "hava", not
// "havah"); uroman spells it out. A mappiq (a point on the he itself) keeps it.
// Swapped for a zero-width non-joiner, which romanizes to nothing and keeps
// every offset where it was.
const SILENT_HE = /(?<=\p{Mn})ה(?![\p{L}\p{M}])/gu;
const ZWNJ = String.fromCharCode(0x200c);

function prepare(text: string): string {
  return text.replace(PRESENTATION_FORMS, (c) => c.normalize("NFKC")).replace(SILENT_HE, ZWNJ);
}

export function uromanize(text: string): string {
  if (!hasUromanScript(text)) return text;
  return new Lattice(prepare(text))
    .romanize()
    .map((e) => e.txt)
    .join("");
}

/**
 * Romanize a word given as its timed syllables. Romanizing each syllable alone
 * treated every syllable as a whole word, so word-final rules fired mid-word
 * (נָגִי + לָה came out "nagiy" + "laha"). The word is romanized whole and each
 * piece of the result goes back to the syllable its source characters came from.
 */
export function uromanizeParts(parts: string[]): string[] {
  if (!parts.some(hasUromanScript)) return parts;
  const prepared = parts.map(prepare);
  const owner: number[] = [];
  prepared.forEach((p, i) => owner.push(...Array.from(p, () => i)));
  const out = parts.map(() => "");
  for (const e of new Lattice(prepared.join("")).romanize()) out[owner[e.start]] += e.txt;
  return out;
}
