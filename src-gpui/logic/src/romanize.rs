//! src/lyrics/romanize.ts
//!
//! Ported function for function, in file order. JavaScript semantics are noted where the
//! TypeScript's UTF-16 indexing, regex flags or `Math.round` matter.

use crate::lyrics::{is_js_ws, js_trim, LyricLine, LyricWord};
use crate::pinyin::pinyin_all;
use crate::uroman::{has_uroman_script, uromanize, uromanize_parts};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

// src/lyrics/romanize.ts:5 Script detection. The TypeScript's regexes have no
// `u` flag, so they test UTF-16 code units: these classes are all in the BMP,
// and a surrogate (any astral character) falls in none of them, which is the
// same as testing each `char` against the ranges.
// src/lyrics/romanize.ts:6
pub(crate) const HIRAGANA: &[(char, char)] = &[('\u{3040}', '\u{309F}')];
// src/lyrics/romanize.ts:7
pub(crate) const KATAKANA: &[(char, char)] = &[('\u{30A0}', '\u{30FF}')];
// src/lyrics/romanize.ts:8
pub(crate) const HANGUL: &[(char, char)] = &[('\u{AC00}', '\u{D7A3}')];
// src/lyrics/romanize.ts:9
pub(crate) const HAN: &[(char, char)] = &[('\u{4E00}', '\u{9FFF}'), ('\u{3400}', '\u{4DBF}')];

/// `REGEX.test(text)` for a character class written as `[a-b…]`.
pub(crate) fn class_test(class: &[(char, char)], text: &str) -> bool {
    text.chars().any(|c| class.iter().any(|&(lo, hi)| lo <= c && c <= hi))
}

// src/lyrics/romanize.ts:11
pub fn has_non_latin_script(text: &str) -> bool {
    class_test(HIRAGANA, text) || class_test(KATAKANA, text) || class_test(HANGUL, text) || class_test(HAN, text)
}

// src/lyrics/romanize.ts:17
pub fn has_han_script(text: &str) -> bool {
    class_test(HAN, text)
}

// src/lyrics/romanize.ts:26
pub fn lines_need_romanization(lines: &[LyricLine]) -> bool {
    // sample the start rather than scanning an entire song
    lines.iter().take(30).any(|l| has_non_latin_script(&l.text) || has_uroman_script(&l.text))
}

// src/lyrics/romanize.ts:44 (`lines.slice(0, 30).map(text).join("")`)
pub fn detect_lyric_lang(lines: &[LyricLine]) -> Option<&'static str> {
    let sample: String = lines.iter().take(30).map(|l| l.text.as_str()).collect();
    if class_test(HIRAGANA, &sample) || class_test(KATAKANA, &sample) {
        return Some("ja");
    }
    if class_test(HANGUL, &sample) {
        return Some("ko");
    }
    if class_test(HAN, &sample) {
        return Some("zh");
    }
    None
}

// ── Japanese kana → romaji (modified Hepburn) ─────────────────
// The tables below are generated from the TypeScript object literals, in order.

// src/lyrics/romanize.ts:59
const DIGRAPH_LIST: &[(&str, &str)] = &[("きゃ", "kya"), ("きゅ", "kyu"), ("きょ", "kyo"), ("ぎゃ", "gya"), ("ぎゅ", "gyu"), ("ぎょ", "gyo"), ("しゃ", "sha"), ("しゅ", "shu"), ("しょ", "sho"), ("しぇ", "she"), ("じゃ", "ja"), ("じゅ", "ju"), ("じょ", "jo"), ("じぇ", "je"), ("ちゃ", "cha"), ("ちゅ", "chu"), ("ちょ", "cho"), ("ちぇ", "che"), ("ぢゃ", "ja"), ("ぢゅ", "ju"), ("ぢょ", "jo"), ("にゃ", "nya"), ("にゅ", "nyu"), ("にょ", "nyo"), ("ひゃ", "hya"), ("ひゅ", "hyu"), ("ひょ", "hyo"), ("びゃ", "bya"), ("びゅ", "byu"), ("びょ", "byo"), ("ぴゃ", "pya"), ("ぴゅ", "pyu"), ("ぴょ", "pyo"), ("みゃ", "mya"), ("みゅ", "myu"), ("みょ", "myo"), ("りゃ", "rya"), ("りゅ", "ryu"), ("りょ", "ryo"), ("ふぁ", "fa"), ("ふぃ", "fi"), ("ふぇ", "fe"), ("ふぉ", "fo"), ("うぃ", "wi"), ("うぇ", "we"), ("うぉ", "wo"), ("ゔぁ", "va"), ("ゔぃ", "vi"), ("ゔぇ", "ve"), ("ゔぉ", "vo"), ("てぃ", "ti"), ("でぃ", "di"), ("とぅ", "tu"), ("どぅ", "du")];

// src/lyrics/romanize.ts:78
const KANA_LIST: &[(&str, &str)] = &[("あ", "a"), ("い", "i"), ("う", "u"), ("え", "e"), ("お", "o"), ("か", "ka"), ("き", "ki"), ("く", "ku"), ("け", "ke"), ("こ", "ko"), ("が", "ga"), ("ぎ", "gi"), ("ぐ", "gu"), ("げ", "ge"), ("ご", "go"), ("さ", "sa"), ("し", "shi"), ("す", "su"), ("せ", "se"), ("そ", "so"), ("ざ", "za"), ("じ", "ji"), ("ず", "zu"), ("ぜ", "ze"), ("ぞ", "zo"), ("た", "ta"), ("ち", "chi"), ("つ", "tsu"), ("て", "te"), ("と", "to"), ("だ", "da"), ("ぢ", "ji"), ("づ", "zu"), ("で", "de"), ("ど", "do"), ("な", "na"), ("に", "ni"), ("ぬ", "nu"), ("ね", "ne"), ("の", "no"), ("は", "ha"), ("ひ", "hi"), ("ふ", "fu"), ("へ", "he"), ("ほ", "ho"), ("ば", "ba"), ("び", "bi"), ("ぶ", "bu"), ("べ", "be"), ("ぼ", "bo"), ("ぱ", "pa"), ("ぴ", "pi"), ("ぷ", "pu"), ("ぺ", "pe"), ("ぽ", "po"), ("ま", "ma"), ("み", "mi"), ("む", "mu"), ("め", "me"), ("も", "mo"), ("や", "ya"), ("ゆ", "yu"), ("よ", "yo"), ("ら", "ra"), ("り", "ri"), ("る", "ru"), ("れ", "re"), ("ろ", "ro"), ("わ", "wa"), ("ゐ", "wi"), ("ゑ", "we"), ("を", "o"), ("ん", "n"), ("ゔ", "vu"), ("ぁ", "a"), ("ぃ", "i"), ("ぅ", "u"), ("ぇ", "e"), ("ぉ", "o"), ("ゃ", "ya"), ("ゅ", "yu"), ("ょ", "yo")];

static DIGRAPHS: LazyLock<HashMap<(char, char), &'static str>> = LazyLock::new(|| {
    DIGRAPH_LIST
        .iter()
        .map(|&(k, v)| {
            let mut c = k.chars();
            ((c.next().unwrap(), c.next().unwrap()), v)
        })
        .collect()
});

static KANA: LazyLock<HashMap<char, &'static str>> = LazyLock::new(|| KANA_LIST.iter().map(|&(k, v)| (k.chars().next().unwrap(), v)).collect());

// src/lyrics/romanize.ts:99
fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'i' | 'u' | 'e' | 'o')
}

// src/lyrics/romanize.ts:101 (`/[ァ-ヶ]/g`, U+30A1..U+30F6)
pub fn katakana_to_hiragana(text: &str) -> String {
    text.chars().map(|c| if ('\u{30A1}'..='\u{30F6}').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap() } else { c }).collect()
}

// src/lyrics/romanize.ts:107. The TypeScript walks UTF-16 units; an astral
// character is two units that neither start a digraph nor are kana, are copied
// through one by one and never read as `n`/a vowel, so one `char` per step
// gives the same output.
pub fn romanize_japanese(input: &str) -> String {
    let text: Vec<char> = katakana_to_hiragana(input).chars().collect();
    let mut out = String::new();
    let mut sokuon = false; // っ pending: doubles the next consonant

    let mut i = 0;
    while i < text.len() {
        let mut romaji: Option<&str> = if i + 1 < text.len() { DIGRAPHS.get(&(text[i], text[i + 1])).copied() } else { None };
        if romaji.is_some() {
            i += 1;
        } else if text[i] == '\u{3063}' {
            sokuon = true;
            i += 1;
            continue;
        } else if text[i] == '\u{30FC}' {
            // chōonpu: hold the preceding vowel
            if let Some(last) = out.chars().last() {
                if is_vowel(last) {
                    out.push(last);
                }
            }
            i += 1;
            continue;
        } else {
            romaji = KANA.get(&text[i]).copied();
        }

        let Some(romaji) = romaji else {
            // not kana (kanji, latin, punctuation) — leave it alone
            out.push(text[i]);
            sokuon = false;
            i += 1;
            continue;
        };

        if sokuon {
            // っち → tchi, everything else doubles its initial consonant
            if romaji.starts_with("ch") {
                out.push('t');
            } else {
                out.push(romaji.chars().next().unwrap());
            }
            sokuon = false;
        }

        // ん needs an apostrophe before a vowel or y, or it reads as one syllable
        let first = romaji.chars().next().unwrap();
        if out.ends_with('n') && text[i] != '\u{3093}' && (is_vowel(first) || first == 'y') && i > 0 && KANA.get(&text[i - 1]).copied() == Some("n") {
            out.push('\'');
        }

        out.push_str(romaji);
        i += 1;
    }

    out
}

// ── Korean hangul → Revised Romanization ──────────────────────

// src/lyrics/romanize.ts:162
const INITIALS: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];

// src/lyrics/romanize.ts:167
const MEDIALS: [&str; 21] = ["a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i"];

// src/lyrics/romanize.ts:172
const FINALS: [&str; 28] = ["", "k", "k", "ks", "n", "nj", "nh", "t", "l", "lg", "lm", "lb", "ls", "lt", "lp", "lh", "m", "p", "ps", "s", "ss", "ng", "j", "ch", "k", "t", "p", "h"];

// src/lyrics/romanize.ts:178. `ch.charCodeAt(0)` of an astral character is its high
// surrogate, outside the syllable block, as its code point is here.
pub fn romanize_korean(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        let code = ch as u32;
        if !(0xac00..=0xd7a3).contains(&code) {
            out.push(ch);
            continue;
        }
        let n = (code - 0xac00) as usize;
        out.push_str(INITIALS[n / 588]);
        out.push_str(MEDIALS[(n % 588) / 28]);
        out.push_str(FINALS[n % 28]);
    }
    out
}

// ── Han readings (dictionary) ─────────────────────────────────

/// `ReadingToken` (src/lyrics/romanize.ts:205): one word of a line as
/// electron/readings.cjs gives it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReadingToken {
    pub s: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
}

// src/lyrics/romanize.ts:213
pub fn attaches(tok: &ReadingToken) -> bool {
    let (b, d) = (tok.b.as_deref(), tok.d.as_deref());
    match tok.p.as_deref() {
        Some("助動詞") => b != Some("です") && b != Some("だ"),
        Some("動詞") => d == Some("接尾") || (d == Some("非自立") && b != Some("いる") && b != Some("ある")),
        Some("助詞") => d == Some("接続助詞"),
        Some("名詞") => d == Some("接尾"),
        Some("記号") => true,
        _ => false,
    }
}

// src/lyrics/romanize.ts:230 (`/^\s+$/`)
pub fn is_space(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_js_ws)
}

// src/lyrics/romanize.ts:232 (`/[一-鿿㐀-䶿々]/`)
const JA_KANJI: &[(char, char)] = &[('\u{4E00}', '\u{9FFF}'), ('\u{3400}', '\u{4DBF}'), ('\u{3005}', '\u{3005}')];

fn is_ja_kanji(c: char) -> bool {
    JA_KANJI.iter().any(|&(lo, hi)| lo <= c && c <= hi)
}

// src/lyrics/romanize.ts:233 (`JA_KANJI_RUNS`, global match): alternating runs of
// kanji and non-kanji, which between them cover the whole string.
fn ja_kanji_runs(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut kind: Option<bool> = None;
    for c in text.chars() {
        let k = is_ja_kanji(c);
        if kind == Some(k) {
            parts.last_mut().unwrap().push(c);
        } else {
            parts.push(c.to_string());
            kind = Some(k);
        }
    }
    parts
}

/// The text a line's readings are looked up for: its words run together, so each character lines up with one.
// src/lyrics/romanize.ts:236
pub fn line_chars(line: &LyricLine) -> String {
    match &line.words {
        Some(w) if !w.is_empty() => w.iter().map(|w| w.text.as_str()).collect(),
        _ => line.text.clone(),
    }
}

/// Texts to ask electron/readings.cjs about, one per line.
// src/lyrics/romanize.ts:241
pub fn reading_texts(lines: &[LyricLine]) -> Vec<String> {
    lines.iter().map(line_chars).collect()
}

/// True for Japanese lyrics: readings give kanji their sound and the romaji its word breaks.
// src/lyrics/romanize.ts:246
pub fn lines_need_readings(lines: &[LyricLine]) -> bool {
    detect_lyric_lang(lines) == Some("ja")
}

// src/lyrics/romanize.ts:251 (`/[ゃゅょぁぃぅぇぉゎー]/`)
pub fn morae(kana: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut carry = String::new();
    for ch in kana.chars() {
        if "ゃゅょぁぃぅぇぉゎー".contains(ch) && !out.is_empty() && carry.is_empty() {
            out.last_mut().unwrap().push(ch);
        } else if ch == 'っ' {
            carry.push(ch);
        } else {
            out.push(format!("{carry}{ch}"));
            carry.clear();
        }
    }
    if !carry.is_empty() {
        if let Some(last) = out.last_mut() {
            last.push_str(&carry);
        } else {
            out.push(carry);
        }
    }
    out
}

/// `Math.round` for the non-negative values used here (rounds half up).
fn js_round(x: f64) -> usize {
    let f = x.floor();
    (if x - f >= 0.5 { f + 1.0 } else { f }) as usize
}

/// `arr.slice(a, b)` for non-negative bounds.
fn slice_range<T>(v: &[T], a: usize, b: usize) -> &[T] {
    let b = b.min(v.len());
    let a = a.min(b);
    &v[a..b]
}

// src/lyrics/romanize.ts:270
pub fn spread(reading: &str, n: usize) -> Vec<String> {
    if n <= 1 {
        return vec![reading.to_string()];
    }
    let m = morae(reading);
    if m.len() < n {
        let mut v = vec![reading.to_string()];
        v.extend(std::iter::repeat_n(String::new(), n - 1));
        return v;
    }
    (0..n)
        .map(|i| slice_range(&m, js_round((i * m.len()) as f64 / n as f64), js_round(((i + 1) * m.len()) as f64 / n as f64)).join(""))
        .collect()
}

// src/lyrics/romanize.ts:286
pub fn token_chars(tok: &ReadingToken) -> Vec<String> {
    let chars: Vec<String> = tok.s.chars().map(String::from).collect();
    let r = match &tok.r {
        Some(r) if !r.is_empty() && tok.s.chars().any(is_ja_kanji) => r,
        _ => return chars,
    };
    let reading = katakana_to_hiragana(r);
    let parts = ja_kanji_runs(&katakana_to_hiragana(&tok.s));
    // JS `.` does not match the four line terminators
    let any = "([^\\n\\r\\u{2028}\\u{2029}]+)";
    let pattern = format!(
        "^{}$",
        parts.iter().map(|p| if p.chars().any(is_ja_kanji) { any.to_string() } else { regex::escape(p) }).collect::<String>()
    );
    let m = Regex::new(&pattern).expect("reading pattern").captures(&reading);
    let Some(m) = m else {
        return spread(&reading, chars.len());
    };
    let mut out: Vec<String> = Vec::new();
    let mut g = 1;
    for p in &parts {
        let pc: Vec<String> = p.chars().map(String::from).collect();
        if p.chars().any(is_ja_kanji) {
            out.extend(spread(m.get(g).unwrap().as_str(), pc.len()));
            g += 1;
        } else {
            out.extend(pc);
        }
    }
    out
}

/// Each character's text, joined: romanized hanzi stand apart from each other
/// and from Latin, and `sp` puts a space before a character that starts a word.
// src/lyrics/romanize.ts:308
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharText {
    pub t: String,
    pub roma: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sp: Option<bool>,
}

// src/lyrics/romanize.ts:310
pub fn join_chars(chars: &[CharText]) -> String {
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(&chars[i - 1]) } else { None };
        if c.sp == Some(true) && !out.is_empty() && !out.chars().last().is_some_and(is_js_ws) {
            out.push(' ');
        }
        if let Some(prev) = prev {
            if !prev.t.is_empty() && !c.t.is_empty() && (prev.roma || c.roma) {
                // the last / first UTF-16 unit: a surrogate is never ASCII, as an astral char is not
                let a = if prev.roma { Some('a') } else { prev.t.chars().last() };
                let b = if c.roma { Some('a') } else { c.t.chars().next() };
                if a.is_some_and(|a| a.is_ascii_alphanumeric()) && b.is_some_and(|b| b.is_ascii_alphanumeric()) {
                    out.push(' ');
                }
            }
        }
        out.push_str(&c.t);
    }
    out
}

/// Puts per-character text back into the line, word by word, keeping each word's timing.
// src/lyrics/romanize.ts:326
pub fn with_chars(line: &LyricLine, chars: &[CharText]) -> LyricLine {
    let Some(line_words) = line.words.as_ref().filter(|w| !w.is_empty()) else {
        return LyricLine { text: join_chars(chars), ..line.clone() };
    };
    // Word breaks known (Japanese): a unit glues to the next unless a word starts there.
    let known = chars.iter().any(|c| c.sp.is_some());
    let mut i = 0;
    let words: Vec<LyricWord> = line_words
        .iter()
        .map(|w| {
            let n = w.text.chars().count();
            let own = slice_range(chars, i, i + n);
            // a space at the unit's own start is the gap between units, not inside it
            let own: Vec<CharText> = own.iter().enumerate().map(|(k, c)| if k == 0 { CharText { sp: Some(false), ..c.clone() } } else { c.clone() }).collect();
            let text = join_chars(&own);
            i += n;
            if !known {
                return LyricWord { text, ..w.clone() };
            }
            let next = chars.get(i);
            let join_next = next.is_some_and(|next| next.sp != Some(true)) && !text.chars().all(is_js_ws) && !next.is_some_and(|next| next.t.chars().next().is_some_and(is_js_ws));
            LyricWord { text, join_next: Some(join_next), read_join: Some(true), ..w.clone() }
        })
        // a kanji whose reading went to its neighbour (too few morae to share)
        .filter(|w| !w.text.is_empty())
        .collect();
    LyricLine { words: Some(words), ..line.clone() }
}

/// Japanese lines with kanji swapped for their readings in hiragana, from
/// electron/readings.cjs, and spaces where words start, so the romaji reads
/// "onegai ii desu ka" rather than "onegaiiidesuka".
// src/lyrics/romanize.ts:352
pub fn apply_readings(lines: &[LyricLine], tokens: &[Vec<ReadingToken>]) -> Vec<LyricLine> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let toks = match tokens.get(i) {
                Some(t) if !t.is_empty() => t,
                _ => return line.clone(),
            };
            let text = line_chars(line);
            if !has_non_latin_script(&text) {
                return line.clone();
            }
            // anything off (a dropped character) and the line is left as it was
            if toks.iter().map(|t| t.s.as_str()).collect::<String>() != text {
                return line.clone();
            }
            let mut chars: Vec<CharText> = Vec::new();
            for (k, tok) in toks.iter().enumerate() {
                let prev = if k > 0 { Some(&toks[k - 1]) } else { None };
                let sp = prev.is_some_and(|p| !is_space(&p.s)) && !is_space(&tok.s) && !attaches(tok);
                // the topic and direction particles are said wa and e
                let particle: Option<&str> = if tok.p.as_deref() == Some("助詞") {
                    if tok.s == "は" {
                        Some("わ")
                    } else if tok.s == "へ" {
                        Some("え")
                    } else {
                        None
                    }
                } else {
                    None
                };
                // In counting sequences (一二三四…), 四 is read し (shi), not IPADIC's default よん (yon).
                let counting_four = if tok.s == "四" && prev.is_some_and(|p| p.s == "三") { Some("し") } else { None };
                let own: Vec<String> = if let Some(p) = particle {
                    vec![p.to_string()]
                } else if let Some(c) = counting_four {
                    vec![c.to_string()]
                } else if is_space(&tok.s) {
                    tok.s.chars().map(|_| " ".to_string()).collect()
                } else {
                    token_chars(tok)
                };
                for (n, t) in own.into_iter().enumerate() {
                    chars.push(CharText { t, roma: false, sp: Some(n == 0 && sp) });
                }
            }
            if chars.len() != text.chars().count() {
                return line.clone();
            }
            with_chars(line, &chars)
        })
        .collect()
}

/// Chinese lines with hanzi swapped for pinyin (no tone marks).
// src/lyrics/romanize.ts:382
pub fn apply_pinyin(lines: &[LyricLine]) -> Vec<LyricLine> {
    lines
        .iter()
        .map(|line| {
            let text = line_chars(line);
            if !class_test(HAN, &text) {
                return line.clone();
            }
            let all = pinyin_all(&text);
            if all.len() != text.chars().count() {
                return line.clone();
            }
            let chars: Vec<CharText> = all
                .into_iter()
                .map(|c| if c.is_zh && !c.pinyin.is_empty() { CharText { t: c.pinyin, roma: true, sp: None } } else { CharText { t: c.origin, roma: false, sp: None } })
                .collect();
            with_chars(line, &chars)
        })
        .collect()
}

// ── Entry points ──────────────────────────────────────────────
// src/lyrics/romanize.ts:396
pub fn romanize_kana_hangul(text: &str) -> String {
    let mut out = text.to_string();
    if class_test(HANGUL, &out) {
        out = romanize_korean(&out);
    }
    if class_test(HIRAGANA, &out) || class_test(KATAKANA, &out) {
        out = romanize_japanese(&out);
    }
    out
}

// src/lyrics/romanize.ts:403
pub fn romanize_text(text: &str) -> String {
    // every other non-Latin script (Arabic, Hebrew, Cyrillic, Thai, Devanagari…)
    uromanize(&romanize_kana_hangul(text))
}

/// Lay a source-supplied romanization over lines that already have real timings.
/// Lines are paired by timestamp within a tolerance, and each romanized line's
/// tokens are spread across that line's words.
// src/lyrics/romanize.ts:428
pub fn align_romanized_to_words(lines: &[LyricLine], roma: &[LyricLine], tolerance_s: f64) -> Vec<LyricLine> {
    if lines.is_empty() || roma.is_empty() {
        return lines.to_vec();
    }

    let mut j = 0;
    lines
        .iter()
        .map(|line| {
            if line.gap == Some(true) {
                return line.clone();
            }
            while j < roma.len() && roma[j].t < line.t - tolerance_s {
                j += 1;
            }

            let Some(m) = roma.get(j) else { return line.clone() };
            if (m.t - line.t).abs() > tolerance_s {
                return line.clone();
            }
            j += 1;

            let text = js_trim(&m.text).to_string();
            if text.is_empty() {
                return line.clone();
            }

            let Some(words) = line.words.as_ref().filter(|w| !w.is_empty()) else {
                return LyricLine { text, ..line.clone() };
            };

            let tokens: Vec<&str> = text.split(is_js_ws).filter(|s| !s.is_empty()).collect();
            if tokens.is_empty() {
                return line.clone();
            }

            let per = tokens.len() as f64 / words.len() as f64;
            let next: Vec<LyricWord> = words
                .iter()
                .enumerate()
                .map(|(i, w)| LyricWord {
                    text: slice_range(&tokens, js_round(i as f64 * per), js_round((i + 1) as f64 * per)).join(" "),
                    // The romanized run is spaced Latin text, so nothing glues together.
                    join_next: Some(false),
                    ..w.clone()
                })
                .filter(|w| !w.text.is_empty())
                .collect();

            if next.is_empty() {
                return LyricLine { text, ..line.clone() };
            }
            LyricLine { text: next.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "), words: Some(next), ..line.clone() }
        })
        .collect()
}

/// Romanize "both": each line keeps its own text and timing, and carries its
/// romanization (from `roman`, the same lyrics romanized) to show under it.
// src/lyrics/romanize.ts:476
pub fn with_roma_lines(lines: &[LyricLine], roman: &[LyricLine], tolerance_s: f64) -> Vec<LyricLine> {
    let same = roman.len() == lines.len() && roman.iter().zip(lines).all(|(r, l)| r.t == l.t);
    let mut j = 0;
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line.gap == Some(true) {
                return line.clone();
            }
            if !has_non_latin_script(&line.text) && !has_uroman_script(&line.text) {
                return line.clone();
            }
            let mut m: Option<&LyricLine> = None;
            if same {
                m = roman.get(i);
            } else {
                while j < roman.len() && roman[j].t < line.t - tolerance_s {
                    j += 1;
                }
                if j < roman.len() && (roman[j].t - line.t).abs() <= tolerance_s {
                    m = Some(&roman[j]);
                    j += 1;
                }
            }
            match m {
                Some(m) => {
                    let text = js_trim(&m.text);
                    if !text.is_empty() && text != js_trim(&line.text) {
                        LyricLine { roma: Some(Box::new(m.clone())), ..line.clone() }
                    } else {
                        line.clone()
                    }
                }
                None => line.clone(),
            }
        })
        .collect()
}

/// Romanize in place, keeping every timestamp so word-level highlighting still
/// lines up with playback.
// src/lyrics/romanize.ts:497
pub fn romanize_lines(lines: &[LyricLine]) -> Vec<LyricLine> {
    lines
        .iter()
        .map(|line| {
            let Some(line_words) = line.words.as_ref().filter(|w| !w.is_empty()) else {
                return LyricLine { text: romanize_text(&line.text), ..line.clone() };
            };
            // Each romanized CJK unit stands apart (一二三 → "ichi ni san"); glued, they
            // ran together into one unreadable word.
            let mut texts: Vec<String> = line_words.iter().map(|w| romanize_kana_hangul(&w.text)).collect();
            // uroman's rules look at whole words, so glued syllables go in together
            let mut a = 0;
            while a < texts.len() {
                let mut b = a + 1;
                while b < texts.len() && line_words[b - 1].join_next == Some(true) {
                    b += 1;
                }
                let parts = uromanize_parts(&texts[a..b]);
                texts.splice(a..b, parts);
                a = b;
            }
            let words: Vec<LyricWord> = line_words
                .iter()
                .enumerate()
                .map(|(i, w)| LyricWord {
                    // `texts[i]`: uromanize_parts returns one string per part
                    text: texts[i].clone(),
                    // `w.joinNext && (w.readJoin || !hasNonLatinScript(w.text))`
                    join_next: w.join_next.map(|jn| jn && (w.read_join == Some(true) || !has_non_latin_script(&w.text))),
                    ..w.clone()
                })
                .collect();
            let mut text = String::new();
            for (i, w) in words.iter().enumerate() {
                text.push_str(&w.text);
                if i + 1 < words.len() && w.join_next != Some(true) {
                    text.push(' ');
                }
            }
            LyricLine { text, words: Some(words), ..line.clone() }
        })
        .collect()
}
