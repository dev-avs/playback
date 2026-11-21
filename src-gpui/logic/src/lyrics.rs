//! src/lyrics/lyrics.ts
//!
//! The types are written here first (before the port of the functions) so the
//! other modules can share them. Field names serialize as the TypeScript's
//! (camelCase), and every optional field stays an `Option`, so a value that is
//! absent in TypeScript is absent here too.

use serde::{Deserialize, Serialize};

/// `TIER` (src/lyrics/lyrics.ts:8): how finely a source knows the timing.
pub mod tier {
    pub const NONE: u8 = 0;
    pub const LINE: u8 = 1;
    pub const WORD: u8 = 2;
    pub const SYLLABLE: u8 = 3;
}
pub type Tier = u8;

/// `LyricWord` (src/lyrics/lyrics.ts:11)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricWord {
    pub t: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_next: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_join: Option<bool>,
}

/// `LyricLine` (src/lyrics/lyrics.ts:22)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub t: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<LyricWord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roma: Option<Box<LyricLine>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opposite: Option<bool>,
}

// ── JavaScript string semantics ───────────────────────────────
// JS `\s`, `trim()` and `[^...]`/`.` behave on UTF-16 code units with their own
// whitespace set, which is not `char::is_whitespace` (JS has U+FEFF, lacks
// U+0085). `.` excludes the four line terminators.

/// The set `\s` and `String.prototype.trim` use (WhiteSpace + LineTerminator).
pub(crate) fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `str.trim()`
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// What `.` does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `str.length`
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `\d` (ASCII only in JS)
fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && is_js_ws(chars[i]) {
        i += 1;
    }
    i
}

fn skip_digits(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && is_digit(chars[i]) {
        i += 1;
    }
    i
}

fn collect(chars: &[char]) -> String {
    chars.iter().collect()
}

/// `parseInt` of a string the regexes already proved is all digits.
fn parse_int(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `parseFloat` of a string the regexes already proved is `\d+(\.\d+)?`.
fn parse_float(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `/[\p{L}\p{N}]/u.test(c)`. Table generated from Node's own `\p{L}|\p{N}`
/// (Unicode 17.0) so it is the reference's answer, not Rust's `is_alphabetic`.
fn is_letter_or_number(c: char) -> bool {
    let c = c as u32;
    LETTER_OR_NUMBER
        .binary_search_by(|&(lo, hi)| {
            if c < lo {
                std::cmp::Ordering::Greater
            } else if c > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

// src/lyrics/lyrics.ts:44
pub fn parse_mm_ss(min: &str, sec: &str) -> f64 {
    parse_int(min) * 60.0 + parse_float(sec)
}

// ── Credits ───────────────────────────────────────────────────

// src/lyrics/lyrics.ts:57 `CREDIT_ROLE` (alternatives, in order; matched case-insensitively)
const CREDIT_ROLE: &[&str] = &[
    "作词", "作曲", "编曲", "填词", "制作人", "制作", "监制", "出品人", "出品", "发行", "策划", "统筹", "企划", "录音师",
    "录音室", "录音", "混音", "母带后期处理工程师", "母带", "和声", "吉他", "贝斯", "鼓", "键盘", "钢琴", "弦乐", "词",
    "曲", "演唱", "OP", "SP", "Lyricist", "Composer", "Arranger", "Producer", "Mixing", "Mastering", "Recording",
    "Vocal", "Vocals", "Guitar", "Bass", "Drums", "Piano", "Keyboard", "Keyboards", "Strings", "Remix",
];

// src/lyrics/lyrics.ts:61 `CREDIT_BY`
const CREDIT_BY: &[&str] =
    &["written", "lyrics", "composed", "music", "arranged", "produced", "mixed", "mastered", "performed", "recorded"];

// src/lyrics/lyrics.ts:90 `CREDIT_ROLE_LAST`
const CREDIT_ROLE_LAST: &[&str] = &[
    "songwriter", "songwriters", "writer", "writers", "lyricist", "lyricists", "composer", "composers", "producer",
    "producers", "co-producer", "co-producers", "arranger", "arrangers", "engineer", "engineers", "mixer", "mixers",
];

/// Length of `alt` when `chars[i..]` starts with it, ignoring ASCII case (JS `i` flag, no `u`).
fn match_ci(chars: &[char], i: usize, alt: &str) -> Option<usize> {
    let mut n = 0;
    for (k, a) in alt.chars().enumerate() {
        match chars.get(i + k) {
            Some(c) if c.eq_ignore_ascii_case(&a) => n += 1,
            _ => return None,
        }
    }
    Some(n)
}

fn is_colon(c: char) -> bool {
    c == ':' || c == '：'
}

/// `/^\s*(?:alts)\s*[:：]/i`
fn credit_role(chars: &[char]) -> bool {
    let i = skip_ws(chars, 0);
    for alt in CREDIT_ROLE {
        if let Some(n) = match_ci(chars, i, alt) {
            let j = skip_ws(chars, i + n);
            if chars.get(j).is_some_and(|&c| is_colon(c)) {
                return true;
            }
        }
    }
    false
}

/// `/^\s*(?:alts)\s+by\b/i`
fn credit_by(chars: &[char]) -> bool {
    let i = skip_ws(chars, 0);
    for alt in CREDIT_BY {
        if let Some(n) = match_ci(chars, i, alt) {
            let j = skip_ws(chars, i + n);
            if j == i + n {
                continue;
            }
            if let Some(m) = match_ci(chars, j, "by") {
                let after = chars.get(j + m);
                if !after.is_some_and(|&c| c.is_ascii_alphanumeric() || c == '_') {
                    return true;
                }
            }
        }
    }
    false
}

/// `/^\s*[一-鿿]{1,8}\s*[:：]\s*\S/`
fn credit_cjk_label(chars: &[char]) -> bool {
    let i = skip_ws(chars, 0);
    let mut j = i;
    while j < chars.len() && ('\u{4e00}'..='\u{9fff}').contains(&chars[j]) {
        j += 1;
    }
    let n = j - i;
    if !(1..=8).contains(&n) {
        return false;
    }
    let j = skip_ws(chars, j);
    if !chars.get(j).is_some_and(|&c| is_colon(c)) {
        return false;
    }
    let j = skip_ws(chars, j + 1);
    j < chars.len()
}

/// `/,\s*(?:songwriters?|…)\s*$/i`
fn credit_role_last(chars: &[char]) -> bool {
    for (i, &c) in chars.iter().enumerate() {
        if c != ',' {
            continue;
        }
        let rest = collect(&chars[i + 1..]);
        let rest = js_trim(&rest);
        if CREDIT_ROLE_LAST.iter().any(|w| rest.eq_ignore_ascii_case(w)) {
            return true;
        }
    }
    false
}

// src/lyrics/lyrics.ts:92
pub fn is_credit_line(text: &str) -> bool {
    let t = js_trim(text);
    if t.is_empty() {
        return false;
    }
    let chars: Vec<char> = t.chars().collect();
    credit_role(&chars) || credit_by(&chars) || credit_cjk_label(&chars) || credit_role_last(&chars)
}

// src/lyrics/lyrics.ts:115
pub fn mark_parenthetical_backing_vocals(lines: &[LyricLine]) -> Vec<LyricLine> {
    let mut seen_lead = false;
    let mut out: Vec<LyricLine> = Vec::new();

    for line in lines {
        if line.gap == Some(true) || line.bg == Some(true) {
            out.push(line.clone());
            continue;
        }

        let t = js_trim(&line.text);
        let wrapped = (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('[') && t.ends_with(']'));
        // Reject "(a) b (c)" — the opening bracket has to survive to the end.
        let closer = if t.starts_with('(') { ')' } else { ']' };
        let wraps_whole_line = wrapped && utf16_len(t) >= 3 && t.find(closer) == Some(t.len() - 1);

        if wraps_whole_line {
            if seen_lead {
                let mut l = line.clone();
                l.bg = Some(true);
                out.push(l);
            } else {
                out.push(line.clone());
            }
            continue;
        }

        seen_lead = true;
        match split_trailing_aside(line) {
            None => {
                out.push(line.clone());
                continue;
            }
            Some((lead, bg)) => {
                out.push(lead);
                out.push(bg);
            }
        }
    }

    out
}

// src/lyrics/lyrics.ts:151 `TRAILING_ASIDE` = /^(.*\S)\s*\(([^()]{2,})\)$/ is matched inside
// `split_trailing_aside`: the `(` must be the last one in the string, the `)` the last character.

// src/lyrics/lyrics.ts:163
pub fn aside_word_index(words: &[LyricWord]) -> isize {
    for i in 1..words.len() {
        if !words[i].text.starts_with('(') {
            continue;
        }
        let tail: String = words[i..].iter().map(|w| w.text.as_str()).collect();
        if tail.ends_with(')') {
            return i as isize;
        }
    }
    -1
}

// src/lyrics/lyrics.ts:172
pub fn strip_brackets(words: &[LyricWord]) -> Vec<LyricWord> {
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let mut text = w.text.clone();
            if i == 0 {
                text = text.trim_start_matches('(').to_string();
            }
            if i == words.len() - 1 {
                text = text.trim_end_matches(')').to_string();
            }
            LyricWord { text, ..w.clone() }
        })
        .filter(|w| !w.text.is_empty())
        .collect()
}

// src/lyrics/lyrics.ts:199
pub fn split_trailing_aside(line: &LyricLine) -> Option<(LyricLine, LyricLine)> {
    // `line.text.trim().match(TRAILING_ASIDE)`
    let t = js_trim(&line.text);
    let chars: Vec<char> = t.chars().collect();
    let n = chars.len();
    if n == 0 || chars[n - 1] != ')' {
        return None;
    }
    let open = chars.iter().rposition(|&c| c == '(')?;
    let inner = &chars[open + 1..n - 1];
    if inner.contains(&')') {
        return None;
    }
    let inner_text = collect(inner);
    if utf16_len(&inner_text) < 2 {
        return None;
    }
    let prefix = &chars[..open];
    let last = prefix.iter().rposition(|&c| !is_js_ws(c))?;
    let g1 = &prefix[..=last];
    if g1.iter().any(|&c| is_line_terminator(c)) {
        return None;
    }
    let (m1, m2) = (collect(g1), inner_text);

    let lead_text = js_trim(&m1).to_string();
    let aside_text = js_trim(&m2).to_string();
    // A lead of punctuation alone is not a lead; leave the line as it was.
    if lead_text.is_empty() || aside_text.is_empty() || !lead_text.chars().any(is_letter_or_number) {
        return None;
    }

    let words: Vec<LyricWord> = line.words.clone().unwrap_or_default();
    let cut: isize = if !words.is_empty() { aside_word_index(&words) } else { -1 };
    // The aside sits inside a word rather than between two, so there is no clean
    // cut. Splitting anyway would mean dropping this line's word timings, and
    // real per-word sync is worth more than the shape, so leave the line alone.
    if !words.is_empty() && cut < 1 {
        return None;
    }

    let lead_words: Vec<LyricWord> = if cut > 0 { words[..cut as usize].to_vec() } else { Vec::new() };
    let bg_words: Vec<LyricWord> = if cut > 0 { strip_brackets(&words[cut as usize..]) } else { Vec::new() };

    let mut lead = line.clone();
    lead.text = lead_text;
    if !lead_words.is_empty() {
        lead.end = lead_words[lead_words.len() - 1].end.or(line.end);
        lead.words = Some(lead_words);
    } else {
        lead.words = None;
    }

    let mut bg = line.clone();
    bg.text = aside_text;
    bg.bg = Some(true);
    bg.t = if !bg_words.is_empty() { bg_words[0].t } else { line.t };
    bg.end = if !bg_words.is_empty() { bg_words[bg_words.len() - 1].end.or(line.end) } else { line.end };
    bg.words = if !bg_words.is_empty() { Some(bg_words) } else { None };

    Some((lead, bg))
}

// src/lyrics/lyrics.ts:244
const HEADER_SCAN: usize = 10;

// src/lyrics/lyrics.ts:254
const TRAILER_SCAN: usize = 15;

// src/lyrics/lyrics.ts:260
const MAX_HEADER_GAP: usize = 2;

/// The `{ lines, credits }` that `extractCredits` returns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtractedCredits {
    pub lines: Vec<LyricLine>,
    pub credits: Vec<String>,
}

// src/lyrics/lyrics.ts:283
pub fn extract_credits(lines: &[LyricLine]) -> ExtractedCredits {
    // Trailing block first (see the TypeScript for why the order matters).
    let mut end = lines.len();
    let floor = lines.len().saturating_sub(TRAILER_SCAN);
    while end > floor && is_credit_line(&lines[end - 1].text) {
        end -= 1;
    }

    // Then the header, tolerating the couple of non-credit lines NetEase puts
    // above the credits but stopping once real lyrics start.
    let mut start = 0;
    let mut last_header: isize = -1;
    let mut run = 0;
    for (i, line) in lines.iter().enumerate().take(end.min(HEADER_SCAN)) {
        if is_credit_line(&line.text) {
            last_header = i as isize;
            run = 0;
        } else {
            run += 1;
            if run > MAX_HEADER_GAP {
                break;
            }
        }
    }
    if last_header >= 0 {
        start = ((last_header + 1) as usize).min(end);
    }

    if start == 0 && end == lines.len() {
        return ExtractedCredits { lines: lines.to_vec(), credits: Vec::new() };
    }

    ExtractedCredits {
        lines: lines[start..end].to_vec(),
        credits: lines[..start]
            .iter()
            .chain(lines[end..].iter())
            .filter(|l| is_credit_line(&l.text))
            .map(|l| js_trim(&l.text).to_string())
            .collect(),
    }
}

// ── LRC parser ────────────────────────────────────────────────

/// One `\[(\d+):(\d+(?:\.\d+)?)\]` at `i`: (minutes, seconds, index after the `]`).
fn lrc_timestamp(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let a = skip_digits(chars, i + 1);
    if a == i + 1 || chars.get(a) != Some(&':') {
        return None;
    }
    let mut b = skip_digits(chars, a + 1);
    if b == a + 1 {
        return None;
    }
    if chars.get(b) == Some(&'.') {
        let c = skip_digits(chars, b + 1);
        if c == b + 1 {
            return None;
        }
        b = c;
    }
    if chars.get(b) != Some(&']') {
        return None;
    }
    Some((collect(&chars[i + 1..a]), collect(&chars[a + 1..b]), b + 1))
}

/// `matchAll(/<(\d+):(\d+(?:\.\d+)?)>([^<]*)/g)`: (minutes, seconds, text).
fn lrc_word_tokens(chars: &[char]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            let a = skip_digits(chars, i + 1);
            if a > i + 1 && chars.get(a) == Some(&':') {
                let mut b = skip_digits(chars, a + 1);
                let mut ok = b > a + 1;
                if ok && chars.get(b) == Some(&'.') {
                    let c = skip_digits(chars, b + 1);
                    if c == b + 1 {
                        ok = false;
                    } else {
                        b = c;
                    }
                }
                if ok && chars.get(b) == Some(&'>') {
                    let mut e = b + 1;
                    while e < chars.len() && chars[e] != '<' {
                        e += 1;
                    }
                    out.push((collect(&chars[i + 1..a]), collect(&chars[a + 1..b]), collect(&chars[b + 1..e])));
                    i = e;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// `body.replace(/<[^>]+>/g, "")`
fn strip_angle_tags(chars: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] != '>' {
            if let Some(off) = chars[i + 1..].iter().position(|&c| c == '>') {
                i = i + 1 + off + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A JS `(a, b) => a - b` comparator: equal (or NaN) is 0.
fn cmp_t(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

// src/lyrics/lyrics.ts:331
pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = Vec::new();
    // A timestamped line with no text is how LRC marks the moment singing stops.
    let mut end_marks: Vec<f64> = Vec::new();

    for raw in lrc.split('\n') {
        // /((?:\[\d+:\d+(?:\.\d+)?\]\s*)+)(.*)/ — unanchored, so the first timestamp anywhere.
        let chars: Vec<char> = raw.chars().collect();
        let Some(first) = (0..chars.len()).find(|&s| lrc_timestamp(&chars, s).is_some()) else {
            continue;
        };
        // A repeated line carries every time it is sung: [00:12.00][00:45.00]text.
        let mut times: Vec<f64> = Vec::new();
        let mut pos = first;
        while let Some((mm, ss, next)) = lrc_timestamp(&chars, pos) {
            times.push(parse_mm_ss(&mm, &ss));
            pos = skip_ws(&chars, next);
        }
        let mut stop = pos;
        while stop < chars.len() && !is_line_terminator(chars[stop]) {
            stop += 1;
        }
        let t = times[0];
        let body_raw = collect(&chars[pos..stop]);
        let body = js_trim(&body_raw);
        if body.is_empty() {
            end_marks.extend(times.iter().copied());
            continue;
        }

        // word-level: <mm:ss.xx>word tokens
        let body_chars: Vec<char> = body.chars().collect();
        let word_tokens = lrc_word_tokens(&body_chars);
        if !word_tokens.is_empty() {
            let mut words: Vec<LyricWord> = word_tokens
                .iter()
                .map(|(mm, ss, text)| LyricWord {
                    t: parse_mm_ss(mm, ss),
                    text: js_trim(text).to_string(),
                    ..Default::default()
                })
                .filter(|w| !w.text.is_empty())
                .collect();
            // Enhanced LRC only marks starts, so a word runs until the next begins.
            for i in 0..words.len().saturating_sub(1) {
                words[i].end = Some(words[i + 1].t);
            }
            let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            lines.push(LyricLine { t, text, words: Some(words), ..Default::default() });
        } else {
            // strip any stray <...> tags (malformed enhanced LRC)
            let stripped = strip_angle_tags(&body_chars);
            let text = js_trim(&stripped);
            if !text.is_empty() {
                for &at in &times {
                    lines.push(LyricLine { t: at, text: text.to_string(), ..Default::default() });
                }
            }
        }
    }

    lines.sort_by(|a, b| cmp_t(a.t, b.t));
    end_marks.sort_by(|a, b| cmp_t(*a, *b));

    // A marker seals the line it follows, and only while it still falls before the next line starts.
    for i in 0..lines.len() {
        let next = lines.get(i + 1).map(|l| l.t);
        let li_t = lines[i].t;
        let mark = end_marks.iter().find(|&&m| m > li_t && next.is_none_or(|nt| m <= nt)).copied();
        if let Some(m) = mark {
            lines[i].end = Some(m);
        }
    }

    lines
}

// src/lyrics/lyrics.ts:402 `SELF_DELIMITING` = /[぀-ゟ゠-ヿ㐀-䶿一-鿿豈-﫿가-힯]/, tested on one UTF-16
// code unit (the last one of the text), so an astral character never matches.
pub fn self_delimiting(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{309f}'
            | '\u{30a0}'..='\u{30ff}'
            | '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{f900}'..='\u{faff}'
            | '\u{ac00}'..='\u{d7af}'
    )
}

/// `matchAll(/\((\d+),(\d+),\d+\)([^(]*)/g)`: (start, duration, text).
fn yrc_tokens(chars: &[char]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '(' {
            let a = skip_digits(chars, i + 1);
            if a > i + 1 && chars.get(a) == Some(&',') {
                let b = skip_digits(chars, a + 1);
                if b > a + 1 && chars.get(b) == Some(&',') {
                    let c = skip_digits(chars, b + 1);
                    if c > b + 1 && chars.get(c) == Some(&')') {
                        let mut e = c + 1;
                        while e < chars.len() && chars[e] != '(' {
                            e += 1;
                        }
                        out.push((collect(&chars[i + 1..a]), collect(&chars[a + 1..b]), collect(&chars[c + 1..e])));
                        i = e;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// `/^\[(\d+),(\d+)\](.*)$/`: (start, duration, rest). `.` stops at \r, so a CRLF line does not match.
fn yrc_head(chars: &[char]) -> Option<(String, String, Vec<char>)> {
    if chars.first() != Some(&'[') {
        return None;
    }
    let a = skip_digits(chars, 1);
    if a == 1 || chars.get(a) != Some(&',') {
        return None;
    }
    let b = skip_digits(chars, a + 1);
    if b == a + 1 || chars.get(b) != Some(&']') {
        return None;
    }
    let rest = &chars[b + 1..];
    if rest.iter().any(|&c| is_line_terminator(c)) {
        return None;
    }
    Some((collect(&chars[1..a]), collect(&chars[a + 1..b]), rest.to_vec()))
}

// src/lyrics/lyrics.ts:420
pub fn parse_yrc(yrc: &str) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw in yrc.split('\n') {
        let raw_chars: Vec<char> = raw.chars().collect();
        let Some((h1, h2, h3)) = yrc_head(&raw_chars) else {
            continue;
        };

        let start = parse_int(&h1) / 1000.0;
        let duration = parse_int(&h2) / 1000.0;
        let tokens = yrc_tokens(&h3);
        if tokens.is_empty() {
            continue;
        }

        let mut words: Vec<LyricWord> = Vec::new();
        let mut buffer = String::new();
        let mut buffer_start = 0.0;
        let mut buffer_end = 0.0;

        for (t_start, t_dur, text) in &tokens {
            if buffer.is_empty() {
                buffer_start = parse_int(t_start) / 1000.0;
            }
            // a fused word ends when its final fragment ends
            buffer_end = (parse_int(t_start) + parse_int(t_dur)) / 1000.0;
            buffer.push_str(text);

            let trimmed = js_trim(&buffer).to_string();
            // trailing whitespace marks a word boundary in space-delimited scripts;
            // in kana/Han/Hangul every token is already its own unit
            let spaced = text.chars().next_back().is_some_and(is_js_ws);
            let self_delimited = !trimmed.is_empty() && trimmed.chars().next_back().is_some_and(self_delimiting);

            if spaced || self_delimited {
                if !trimmed.is_empty() {
                    words.push(LyricWord {
                        t: buffer_start,
                        end: Some(buffer_end),
                        text: trimmed,
                        // No space separated these, so the renderer must not insert one.
                        join_next: Some(!spaced),
                        read_join: None,
                    });
                }
                buffer.clear();
            }
        }
        if !js_trim(&buffer).is_empty() {
            words.push(LyricWord {
                t: buffer_start,
                end: Some(buffer_end),
                text: js_trim(&buffer).to_string(),
                ..Default::default()
            });
        }
        if words.is_empty() {
            continue;
        }

        // Rebuild the display text honouring joinNext.
        let mut text = String::new();
        for i in 0..words.len() {
            text.push_str(&words[i].text);
            if i < words.len() - 1 && words[i].join_next != Some(true) {
                text.push(' ');
            }
        }

        lines.push(LyricLine {
            t: start,
            end: Some(start + duration),
            text,
            words: Some(words),
            ..Default::default()
        });
    }

    lines.sort_by(|a, b| cmp_t(a.t, b.t));
    lines
}

/// `LoadedLyrics` (src/lyrics/lyrics.ts:483)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedLyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_estimated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<Vec<String>>,
    pub tier: Tier,
    pub source: String,
}

// src/lyrics/lyrics.ts:506
pub fn chars_in(text: &str) -> usize {
    // text.replace(/\s+/g, "").length
    text.chars().filter(|&c| !is_js_ws(c)).map(char::len_utf16).sum()
}

// src/lyrics/lyrics.ts:522
pub fn estimate_rate(lines: &[LyricLine]) -> f64 {
    let mut samples: Vec<f64> = Vec::new();
    for i in 0..lines.len().saturating_sub(1) {
        let chars = chars_in(&lines[i].text) as f64;
        let gap = lines[i + 1].t - lines[i].t;
        if chars >= 4.0 && gap > 0.4 && gap <= 8.0 {
            samples.push(gap / chars);
        }
    }
    if samples.is_empty() {
        return 0.11; // ~9 characters a second
    }
    samples.sort_by(|a, b| cmp_t(*a, *b));
    samples[(samples.len() as f64 * 0.3).floor() as usize]
}

// src/lyrics/lyrics.ts:534
// The TypeScript has a second `return lines.map(...)` (end = last word + 0.5)
// after this one. It is unreachable, so it is not ported.
pub fn estimate_word_timings(lines: &[LyricLine]) -> Vec<LyricLine> {
    const MAX_SUNG_SPAN: f64 = 12.0; // seconds
    let rate = estimate_rate(lines);

    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line.words.as_ref().is_some_and(|w| !w.is_empty()) {
                return line.clone();
            }

            let tokens: Vec<&str> = line.text.split(is_js_ws).filter(|s| !s.is_empty()).collect();
            if tokens.len() < 2 {
                return line.clone();
            }

            let next_start = lines.get(i + 1).map(|l| l.t);
            let to_next = next_start.unwrap_or(line.t + 4.0) - line.t;
            // How long this line is actually sung — never longer than the room
            // available before the next line.
            let sung = (chars_in(&line.text) as f64 * rate).max(0.5).min(to_next);
            let span = (match line.end {
                Some(end) => end - line.t,
                None => sung,
            })
            .min(MAX_SUNG_SPAN);
            if span <= 0.2 {
                return line.clone();
            }

            // +1 per word so single letters still occupy a slot
            let weights: Vec<f64> = tokens.iter().map(|w| (utf16_len(w) + 1) as f64).collect();
            let total = weights.iter().fold(0.0, |a, b| a + b);

            let mut cursor = line.t;
            let words: Vec<LyricWord> = tokens
                .iter()
                .enumerate()
                .map(|(wi, text)| {
                    let dur = (weights[wi] / total) * span;
                    let word = LyricWord {
                        t: cursor,
                        end: Some(cursor + dur),
                        text: text.to_string(),
                        ..Default::default()
                    };
                    cursor += dur;
                    word
                })
                .collect();

            LyricLine { words: Some(words), ..line.clone() }
        })
        .collect()
}

/// The request `loadLyrics` takes (`LyricsRequest` in src/types/player.d.ts:39).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsRequest {
    pub artist: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// ask just this provider ("netease" | "lrcmux" | "lrclib") instead of the whole chain
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<String>,
}

/// `LyricsResponse` (src/types/player.d.ts:48)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_lyrics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_lyrics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_lyrics: Option<String>,
    /// "lrclib" | "netease" | "lrcmux"
    pub source: String,
}

/// The one `window.player` call `loadLyrics` makes (`fetchLyrics`). The GPUI
/// build implements this on playback-core; the tests implement it with a
/// recorded response.
pub trait LyricsSource {
    /// `bridge.fetchLyrics(req)`: `None` is the bridge's `null`.
    fn fetch_lyrics(&self, req: &LyricsRequest) -> Option<LyricsResponse>;
}

// src/lyrics/lyrics.ts:580
// `bridge` is `window.player`; `None` is its absence. The TypeScript is async
// only because the bridge call is; here the call is synchronous, the decisions
// are the same.
pub fn load_lyrics(bridge: Option<&dyn LyricsSource>, req: &LyricsRequest) -> Option<LoadedLyrics> {
    let bridge = bridge?;

    let res = bridge.fetch_lyrics(req)?;

    // Best available first: syllable timing, then word, then line.
    if let Some(word_lyrics) = res.word_lyrics.as_deref().filter(|s| !s.is_empty()) {
        let ExtractedCredits { lines, credits } = extract_credits(&parse_yrc(word_lyrics));
        if !lines.is_empty() {
            return Some(LoadedLyrics {
                lines,
                credits: Some(credits),
                synced: true,
                words_estimated: None,
                tier: tier::SYLLABLE,
                source: format!("{}/yrc", res.source),
            });
        }
    }
    if let Some(synced_lyrics) = res.synced_lyrics.as_deref().filter(|s| !s.is_empty()) {
        // Credits come off before anything else (they would skew the singing-rate estimate).
        let ExtractedCredits { lines, credits } = extract_credits(&parse_lrc(synced_lyrics));
        if !lines.is_empty() {
            // Enhanced LRC already carries words with real end times. Plain LRC gets
            // estimates so the word animation still has something to follow.
            let has_words = lines.iter().any(|l| l.words.as_ref().is_some_and(|w| !w.is_empty()));
            return Some(if has_words {
                LoadedLyrics {
                    lines,
                    credits: Some(credits),
                    synced: true,
                    words_estimated: None,
                    tier: tier::WORD,
                    source: res.source,
                }
            } else {
                LoadedLyrics {
                    lines: estimate_word_timings(&lines),
                    credits: Some(credits),
                    synced: true,
                    words_estimated: Some(true),
                    tier: tier::LINE,
                    source: res.source,
                }
            });
        }
    }
    // Untimed lyrics are deliberately not used — they cannot be followed.
    None
}

/// `/[\p{L}\p{N}]/u` as inclusive code point ranges, sorted.
const LETTER_OR_NUMBER: &[(u32, u32)] = &[
    (0x30, 0x39), (0x41, 0x5a), (0x61, 0x7a), (0xaa, 0xaa), (0xb2, 0xb3), (0xb5, 0xb5), (0xb9, 0xba),
    (0xbc, 0xbe), (0xc0, 0xd6), (0xd8, 0xf6), (0xf8, 0x2c1), (0x2c6, 0x2d1), (0x2e0, 0x2e4), (0x2ec, 0x2ec),
    (0x2ee, 0x2ee), (0x370, 0x374), (0x376, 0x377), (0x37a, 0x37d), (0x37f, 0x37f), (0x386, 0x386),
    (0x388, 0x38a), (0x38c, 0x38c), (0x38e, 0x3a1), (0x3a3, 0x3f5), (0x3f7, 0x481), (0x48a, 0x52f),
    (0x531, 0x556), (0x559, 0x559), (0x560, 0x588), (0x5d0, 0x5ea), (0x5ef, 0x5f2), (0x620, 0x64a),
    (0x660, 0x669), (0x66e, 0x66f), (0x671, 0x6d3), (0x6d5, 0x6d5), (0x6e5, 0x6e6), (0x6ee, 0x6fc),
    (0x6ff, 0x6ff), (0x710, 0x710), (0x712, 0x72f), (0x74d, 0x7a5), (0x7b1, 0x7b1), (0x7c0, 0x7ea),
    (0x7f4, 0x7f5), (0x7fa, 0x7fa), (0x800, 0x815), (0x81a, 0x81a), (0x824, 0x824), (0x828, 0x828),
    (0x840, 0x858), (0x860, 0x86a), (0x870, 0x887), (0x889, 0x88f), (0x8a0, 0x8c9), (0x904, 0x939),
    (0x93d, 0x93d), (0x950, 0x950), (0x958, 0x961), (0x966, 0x96f), (0x971, 0x980), (0x985, 0x98c),
    (0x98f, 0x990), (0x993, 0x9a8), (0x9aa, 0x9b0), (0x9b2, 0x9b2), (0x9b6, 0x9b9), (0x9bd, 0x9bd),
    (0x9ce, 0x9ce), (0x9dc, 0x9dd), (0x9df, 0x9e1), (0x9e6, 0x9f1), (0x9f4, 0x9f9), (0x9fc, 0x9fc),
    (0xa05, 0xa0a), (0xa0f, 0xa10), (0xa13, 0xa28), (0xa2a, 0xa30), (0xa32, 0xa33), (0xa35, 0xa36),
    (0xa38, 0xa39), (0xa59, 0xa5c), (0xa5e, 0xa5e), (0xa66, 0xa6f), (0xa72, 0xa74), (0xa85, 0xa8d),
    (0xa8f, 0xa91), (0xa93, 0xaa8), (0xaaa, 0xab0), (0xab2, 0xab3), (0xab5, 0xab9), (0xabd, 0xabd),
    (0xad0, 0xad0), (0xae0, 0xae1), (0xae6, 0xaef), (0xaf9, 0xaf9), (0xb05, 0xb0c), (0xb0f, 0xb10),
    (0xb13, 0xb28), (0xb2a, 0xb30), (0xb32, 0xb33), (0xb35, 0xb39), (0xb3d, 0xb3d), (0xb5c, 0xb5d),
    (0xb5f, 0xb61), (0xb66, 0xb6f), (0xb71, 0xb77), (0xb83, 0xb83), (0xb85, 0xb8a), (0xb8e, 0xb90),
    (0xb92, 0xb95), (0xb99, 0xb9a), (0xb9c, 0xb9c), (0xb9e, 0xb9f), (0xba3, 0xba4), (0xba8, 0xbaa),
    (0xbae, 0xbb9), (0xbd0, 0xbd0), (0xbe6, 0xbf2), (0xc05, 0xc0c), (0xc0e, 0xc10), (0xc12, 0xc28),
    (0xc2a, 0xc39), (0xc3d, 0xc3d), (0xc58, 0xc5a), (0xc5c, 0xc5d), (0xc60, 0xc61), (0xc66, 0xc6f),
    (0xc78, 0xc7e), (0xc80, 0xc80), (0xc85, 0xc8c), (0xc8e, 0xc90), (0xc92, 0xca8), (0xcaa, 0xcb3),
    (0xcb5, 0xcb9), (0xcbd, 0xcbd), (0xcdc, 0xcde), (0xce0, 0xce1), (0xce6, 0xcef), (0xcf1, 0xcf2),
    (0xd04, 0xd0c), (0xd0e, 0xd10), (0xd12, 0xd3a), (0xd3d, 0xd3d), (0xd4e, 0xd4e), (0xd54, 0xd56),
    (0xd58, 0xd61), (0xd66, 0xd78), (0xd7a, 0xd7f), (0xd85, 0xd96), (0xd9a, 0xdb1), (0xdb3, 0xdbb),
    (0xdbd, 0xdbd), (0xdc0, 0xdc6), (0xde6, 0xdef), (0xe01, 0xe30), (0xe32, 0xe33), (0xe40, 0xe46),
    (0xe50, 0xe59), (0xe81, 0xe82), (0xe84, 0xe84), (0xe86, 0xe8a), (0xe8c, 0xea3), (0xea5, 0xea5),
    (0xea7, 0xeb0), (0xeb2, 0xeb3), (0xebd, 0xebd), (0xec0, 0xec4), (0xec6, 0xec6), (0xed0, 0xed9),
    (0xedc, 0xedf), (0xf00, 0xf00), (0xf20, 0xf33), (0xf40, 0xf47), (0xf49, 0xf6c), (0xf88, 0xf8c),
    (0x1000, 0x102a), (0x103f, 0x1049), (0x1050, 0x1055), (0x105a, 0x105d), (0x1061, 0x1061),
    (0x1065, 0x1066), (0x106e, 0x1070), (0x1075, 0x1081), (0x108e, 0x108e), (0x1090, 0x1099),
    (0x10a0, 0x10c5), (0x10c7, 0x10c7), (0x10cd, 0x10cd), (0x10d0, 0x10fa), (0x10fc, 0x1248),
    (0x124a, 0x124d), (0x1250, 0x1256), (0x1258, 0x1258), (0x125a, 0x125d), (0x1260, 0x1288),
    (0x128a, 0x128d), (0x1290, 0x12b0), (0x12b2, 0x12b5), (0x12b8, 0x12be), (0x12c0, 0x12c0),
    (0x12c2, 0x12c5), (0x12c8, 0x12d6), (0x12d8, 0x1310), (0x1312, 0x1315), (0x1318, 0x135a),
    (0x1369, 0x137c), (0x1380, 0x138f), (0x13a0, 0x13f5), (0x13f8, 0x13fd), (0x1401, 0x166c),
    (0x166f, 0x167f), (0x1681, 0x169a), (0x16a0, 0x16ea), (0x16ee, 0x16f8), (0x1700, 0x1711),
    (0x171f, 0x1731), (0x1740, 0x1751), (0x1760, 0x176c), (0x176e, 0x1770), (0x1780, 0x17b3),
    (0x17d7, 0x17d7), (0x17dc, 0x17dc), (0x17e0, 0x17e9), (0x17f0, 0x17f9), (0x1810, 0x1819),
    (0x1820, 0x1878), (0x1880, 0x1884), (0x1887, 0x18a8), (0x18aa, 0x18aa), (0x18b0, 0x18f5),
    (0x1900, 0x191e), (0x1946, 0x196d), (0x1970, 0x1974), (0x1980, 0x19ab), (0x19b0, 0x19c9),
    (0x19d0, 0x19da), (0x1a00, 0x1a16), (0x1a20, 0x1a54), (0x1a80, 0x1a89), (0x1a90, 0x1a99),
    (0x1aa7, 0x1aa7), (0x1b05, 0x1b33), (0x1b45, 0x1b4c), (0x1b50, 0x1b59), (0x1b83, 0x1ba0),
    (0x1bae, 0x1be5), (0x1c00, 0x1c23), (0x1c40, 0x1c49), (0x1c4d, 0x1c7d), (0x1c80, 0x1c8a),
    (0x1c90, 0x1cba), (0x1cbd, 0x1cbf), (0x1ce9, 0x1cec), (0x1cee, 0x1cf3), (0x1cf5, 0x1cf6),
    (0x1cfa, 0x1cfa), (0x1d00, 0x1dbf), (0x1e00, 0x1f15), (0x1f18, 0x1f1d), (0x1f20, 0x1f45),
    (0x1f48, 0x1f4d), (0x1f50, 0x1f57), (0x1f59, 0x1f59), (0x1f5b, 0x1f5b), (0x1f5d, 0x1f5d),
    (0x1f5f, 0x1f7d), (0x1f80, 0x1fb4), (0x1fb6, 0x1fbc), (0x1fbe, 0x1fbe), (0x1fc2, 0x1fc4),
    (0x1fc6, 0x1fcc), (0x1fd0, 0x1fd3), (0x1fd6, 0x1fdb), (0x1fe0, 0x1fec), (0x1ff2, 0x1ff4),
    (0x1ff6, 0x1ffc), (0x2070, 0x2071), (0x2074, 0x2079), (0x207f, 0x2089), (0x2090, 0x209c),
    (0x2102, 0x2102), (0x2107, 0x2107), (0x210a, 0x2113), (0x2115, 0x2115), (0x2119, 0x211d),
    (0x2124, 0x2124), (0x2126, 0x2126), (0x2128, 0x2128), (0x212a, 0x212d), (0x212f, 0x2139),
    (0x213c, 0x213f), (0x2145, 0x2149), (0x214e, 0x214e), (0x2150, 0x2189), (0x2460, 0x249b),
    (0x24ea, 0x24ff), (0x2776, 0x2793), (0x2c00, 0x2ce4), (0x2ceb, 0x2cee), (0x2cf2, 0x2cf3),
    (0x2cfd, 0x2cfd), (0x2d00, 0x2d25), (0x2d27, 0x2d27), (0x2d2d, 0x2d2d), (0x2d30, 0x2d67),
    (0x2d6f, 0x2d6f), (0x2d80, 0x2d96), (0x2da0, 0x2da6), (0x2da8, 0x2dae), (0x2db0, 0x2db6),
    (0x2db8, 0x2dbe), (0x2dc0, 0x2dc6), (0x2dc8, 0x2dce), (0x2dd0, 0x2dd6), (0x2dd8, 0x2dde),
    (0x2e2f, 0x2e2f), (0x3005, 0x3007), (0x3021, 0x3029), (0x3031, 0x3035), (0x3038, 0x303c),
    (0x3041, 0x3096), (0x309d, 0x309f), (0x30a1, 0x30fa), (0x30fc, 0x30ff), (0x3105, 0x312f),
    (0x3131, 0x318e), (0x3192, 0x3195), (0x31a0, 0x31bf), (0x31f0, 0x31ff), (0x3220, 0x3229),
    (0x3248, 0x324f), (0x3251, 0x325f), (0x3280, 0x3289), (0x32b1, 0x32bf), (0x3400, 0x4dbf),
    (0x4e00, 0xa48c), (0xa4d0, 0xa4fd), (0xa500, 0xa60c), (0xa610, 0xa62b), (0xa640, 0xa66e),
    (0xa67f, 0xa69d), (0xa6a0, 0xa6ef), (0xa717, 0xa71f), (0xa722, 0xa788), (0xa78b, 0xa7dc),
    (0xa7f1, 0xa801), (0xa803, 0xa805), (0xa807, 0xa80a), (0xa80c, 0xa822), (0xa830, 0xa835),
    (0xa840, 0xa873), (0xa882, 0xa8b3), (0xa8d0, 0xa8d9), (0xa8f2, 0xa8f7), (0xa8fb, 0xa8fb),
    (0xa8fd, 0xa8fe), (0xa900, 0xa925), (0xa930, 0xa946), (0xa960, 0xa97c), (0xa984, 0xa9b2),
    (0xa9cf, 0xa9d9), (0xa9e0, 0xa9e4), (0xa9e6, 0xa9fe), (0xaa00, 0xaa28), (0xaa40, 0xaa42),
    (0xaa44, 0xaa4b), (0xaa50, 0xaa59), (0xaa60, 0xaa76), (0xaa7a, 0xaa7a), (0xaa7e, 0xaaaf),
    (0xaab1, 0xaab1), (0xaab5, 0xaab6), (0xaab9, 0xaabd), (0xaac0, 0xaac0), (0xaac2, 0xaac2),
    (0xaadb, 0xaadd), (0xaae0, 0xaaea), (0xaaf2, 0xaaf4), (0xab01, 0xab06), (0xab09, 0xab0e),
    (0xab11, 0xab16), (0xab20, 0xab26), (0xab28, 0xab2e), (0xab30, 0xab5a), (0xab5c, 0xab69),
    (0xab70, 0xabe2), (0xabf0, 0xabf9), (0xac00, 0xd7a3), (0xd7b0, 0xd7c6), (0xd7cb, 0xd7fb),
    (0xf900, 0xfa6d), (0xfa70, 0xfad9), (0xfb00, 0xfb06), (0xfb13, 0xfb17), (0xfb1d, 0xfb1d),
    (0xfb1f, 0xfb28), (0xfb2a, 0xfb36), (0xfb38, 0xfb3c), (0xfb3e, 0xfb3e), (0xfb40, 0xfb41),
    (0xfb43, 0xfb44), (0xfb46, 0xfbb1), (0xfbd3, 0xfd3d), (0xfd50, 0xfd8f), (0xfd92, 0xfdc7),
    (0xfdf0, 0xfdfb), (0xfe70, 0xfe74), (0xfe76, 0xfefc), (0xff10, 0xff19), (0xff21, 0xff3a),
    (0xff41, 0xff5a), (0xff66, 0xffbe), (0xffc2, 0xffc7), (0xffca, 0xffcf), (0xffd2, 0xffd7),
    (0xffda, 0xffdc), (0x10000, 0x1000b), (0x1000d, 0x10026), (0x10028, 0x1003a), (0x1003c, 0x1003d),
    (0x1003f, 0x1004d), (0x10050, 0x1005d), (0x10080, 0x100fa), (0x10107, 0x10133), (0x10140, 0x10178),
    (0x1018a, 0x1018b), (0x10280, 0x1029c), (0x102a0, 0x102d0), (0x102e1, 0x102fb), (0x10300, 0x10323),
    (0x1032d, 0x1034a), (0x10350, 0x10375), (0x10380, 0x1039d), (0x103a0, 0x103c3), (0x103c8, 0x103cf),
    (0x103d1, 0x103d5), (0x10400, 0x1049d), (0x104a0, 0x104a9), (0x104b0, 0x104d3), (0x104d8, 0x104fb),
    (0x10500, 0x10527), (0x10530, 0x10563), (0x10570, 0x1057a), (0x1057c, 0x1058a), (0x1058c, 0x10592),
    (0x10594, 0x10595), (0x10597, 0x105a1), (0x105a3, 0x105b1), (0x105b3, 0x105b9), (0x105bb, 0x105bc),
    (0x105c0, 0x105f3), (0x10600, 0x10736), (0x10740, 0x10755), (0x10760, 0x10767), (0x10780, 0x10785),
    (0x10787, 0x107b0), (0x107b2, 0x107ba), (0x10800, 0x10805), (0x10808, 0x10808), (0x1080a, 0x10835),
    (0x10837, 0x10838), (0x1083c, 0x1083c), (0x1083f, 0x10855), (0x10858, 0x10876), (0x10879, 0x1089e),
    (0x108a7, 0x108af), (0x108e0, 0x108f2), (0x108f4, 0x108f5), (0x108fb, 0x1091b), (0x10920, 0x10939),
    (0x10940, 0x10959), (0x10980, 0x109b7), (0x109bc, 0x109cf), (0x109d2, 0x10a00), (0x10a10, 0x10a13),
    (0x10a15, 0x10a17), (0x10a19, 0x10a35), (0x10a40, 0x10a48), (0x10a60, 0x10a7e), (0x10a80, 0x10a9f),
    (0x10ac0, 0x10ac7), (0x10ac9, 0x10ae4), (0x10aeb, 0x10aef), (0x10b00, 0x10b35), (0x10b40, 0x10b55),
    (0x10b58, 0x10b72), (0x10b78, 0x10b91), (0x10ba9, 0x10baf), (0x10c00, 0x10c48), (0x10c80, 0x10cb2),
    (0x10cc0, 0x10cf2), (0x10cfa, 0x10d23), (0x10d30, 0x10d39), (0x10d40, 0x10d65), (0x10d6f, 0x10d85),
    (0x10e60, 0x10e7e), (0x10e80, 0x10ea9), (0x10eb0, 0x10eb1), (0x10ec2, 0x10ec7), (0x10f00, 0x10f27),
    (0x10f30, 0x10f45), (0x10f51, 0x10f54), (0x10f70, 0x10f81), (0x10fb0, 0x10fcb), (0x10fe0, 0x10ff6),
    (0x11003, 0x11037), (0x11052, 0x1106f), (0x11071, 0x11072), (0x11075, 0x11075), (0x11083, 0x110af),
    (0x110d0, 0x110e8), (0x110f0, 0x110f9), (0x11103, 0x11126), (0x11136, 0x1113f), (0x11144, 0x11144),
    (0x11147, 0x11147), (0x11150, 0x11172), (0x11176, 0x11176), (0x11183, 0x111b2), (0x111c1, 0x111c4),
    (0x111d0, 0x111da), (0x111dc, 0x111dc), (0x111e1, 0x111f4), (0x11200, 0x11211), (0x11213, 0x1122b),
    (0x1123f, 0x11240), (0x11280, 0x11286), (0x11288, 0x11288), (0x1128a, 0x1128d), (0x1128f, 0x1129d),
    (0x1129f, 0x112a8), (0x112b0, 0x112de), (0x112f0, 0x112f9), (0x11305, 0x1130c), (0x1130f, 0x11310),
    (0x11313, 0x11328), (0x1132a, 0x11330), (0x11332, 0x11333), (0x11335, 0x11339), (0x1133d, 0x1133d),
    (0x11350, 0x11350), (0x1135d, 0x11361), (0x11380, 0x11389), (0x1138b, 0x1138b), (0x1138e, 0x1138e),
    (0x11390, 0x113b5), (0x113b7, 0x113b7), (0x113d1, 0x113d1), (0x113d3, 0x113d3), (0x11400, 0x11434),
    (0x11447, 0x1144a), (0x11450, 0x11459), (0x1145f, 0x11461), (0x11480, 0x114af), (0x114c4, 0x114c5),
    (0x114c7, 0x114c7), (0x114d0, 0x114d9), (0x11580, 0x115ae), (0x115d8, 0x115db), (0x11600, 0x1162f),
    (0x11644, 0x11644), (0x11650, 0x11659), (0x11680, 0x116aa), (0x116b8, 0x116b8), (0x116c0, 0x116c9),
    (0x116d0, 0x116e3), (0x11700, 0x1171a), (0x11730, 0x1173b), (0x11740, 0x11746), (0x11800, 0x1182b),
    (0x118a0, 0x118f2), (0x118ff, 0x11906), (0x11909, 0x11909), (0x1190c, 0x11913), (0x11915, 0x11916),
    (0x11918, 0x1192f), (0x1193f, 0x1193f), (0x11941, 0x11941), (0x11950, 0x11959), (0x119a0, 0x119a7),
    (0x119aa, 0x119d0), (0x119e1, 0x119e1), (0x119e3, 0x119e3), (0x11a00, 0x11a00), (0x11a0b, 0x11a32),
    (0x11a3a, 0x11a3a), (0x11a50, 0x11a50), (0x11a5c, 0x11a89), (0x11a9d, 0x11a9d), (0x11ab0, 0x11af8),
    (0x11bc0, 0x11be0), (0x11bf0, 0x11bf9), (0x11c00, 0x11c08), (0x11c0a, 0x11c2e), (0x11c40, 0x11c40),
    (0x11c50, 0x11c6c), (0x11c72, 0x11c8f), (0x11d00, 0x11d06), (0x11d08, 0x11d09), (0x11d0b, 0x11d30),
    (0x11d46, 0x11d46), (0x11d50, 0x11d59), (0x11d60, 0x11d65), (0x11d67, 0x11d68), (0x11d6a, 0x11d89),
    (0x11d98, 0x11d98), (0x11da0, 0x11da9), (0x11db0, 0x11ddb), (0x11de0, 0x11de9), (0x11ee0, 0x11ef2),
    (0x11f02, 0x11f02), (0x11f04, 0x11f10), (0x11f12, 0x11f33), (0x11f50, 0x11f59), (0x11fb0, 0x11fb0),
    (0x11fc0, 0x11fd4), (0x12000, 0x12399), (0x12400, 0x1246e), (0x12480, 0x12543), (0x12f90, 0x12ff0),
    (0x13000, 0x1342f), (0x13441, 0x13446), (0x13460, 0x143fa), (0x14400, 0x14646), (0x16100, 0x1611d),
    (0x16130, 0x16139), (0x16800, 0x16a38), (0x16a40, 0x16a5e), (0x16a60, 0x16a69), (0x16a70, 0x16abe),
    (0x16ac0, 0x16ac9), (0x16ad0, 0x16aed), (0x16b00, 0x16b2f), (0x16b40, 0x16b43), (0x16b50, 0x16b59),
    (0x16b5b, 0x16b61), (0x16b63, 0x16b77), (0x16b7d, 0x16b8f), (0x16d40, 0x16d6c), (0x16d70, 0x16d79),
    (0x16e40, 0x16e96), (0x16ea0, 0x16eb8), (0x16ebb, 0x16ed3), (0x16f00, 0x16f4a), (0x16f50, 0x16f50),
    (0x16f93, 0x16f9f), (0x16fe0, 0x16fe1), (0x16fe3, 0x16fe3), (0x16ff2, 0x16ff6), (0x17000, 0x18cd5),
    (0x18cff, 0x18d1e), (0x18d80, 0x18df2), (0x1aff0, 0x1aff3), (0x1aff5, 0x1affb), (0x1affd, 0x1affe),
    (0x1b000, 0x1b122), (0x1b132, 0x1b132), (0x1b150, 0x1b152), (0x1b155, 0x1b155), (0x1b164, 0x1b167),
    (0x1b170, 0x1b2fb), (0x1bc00, 0x1bc6a), (0x1bc70, 0x1bc7c), (0x1bc80, 0x1bc88), (0x1bc90, 0x1bc99),
    (0x1ccf0, 0x1ccf9), (0x1d2c0, 0x1d2d3), (0x1d2e0, 0x1d2f3), (0x1d360, 0x1d378), (0x1d400, 0x1d454),
    (0x1d456, 0x1d49c), (0x1d49e, 0x1d49f), (0x1d4a2, 0x1d4a2), (0x1d4a5, 0x1d4a6), (0x1d4a9, 0x1d4ac),
    (0x1d4ae, 0x1d4b9), (0x1d4bb, 0x1d4bb), (0x1d4bd, 0x1d4c3), (0x1d4c5, 0x1d505), (0x1d507, 0x1d50a),
    (0x1d50d, 0x1d514), (0x1d516, 0x1d51c), (0x1d51e, 0x1d539), (0x1d53b, 0x1d53e), (0x1d540, 0x1d544),
    (0x1d546, 0x1d546), (0x1d54a, 0x1d550), (0x1d552, 0x1d6a5), (0x1d6a8, 0x1d6c0), (0x1d6c2, 0x1d6da),
    (0x1d6dc, 0x1d6fa), (0x1d6fc, 0x1d714), (0x1d716, 0x1d734), (0x1d736, 0x1d74e), (0x1d750, 0x1d76e),
    (0x1d770, 0x1d788), (0x1d78a, 0x1d7a8), (0x1d7aa, 0x1d7c2), (0x1d7c4, 0x1d7cb), (0x1d7ce, 0x1d7ff),
    (0x1df00, 0x1df1e), (0x1df25, 0x1df2a), (0x1e030, 0x1e06d), (0x1e100, 0x1e12c), (0x1e137, 0x1e13d),
    (0x1e140, 0x1e149), (0x1e14e, 0x1e14e), (0x1e290, 0x1e2ad), (0x1e2c0, 0x1e2eb), (0x1e2f0, 0x1e2f9),
    (0x1e4d0, 0x1e4eb), (0x1e4f0, 0x1e4f9), (0x1e5d0, 0x1e5ed), (0x1e5f0, 0x1e5fa), (0x1e6c0, 0x1e6de),
    (0x1e6e0, 0x1e6e2), (0x1e6e4, 0x1e6e5), (0x1e6e7, 0x1e6ed), (0x1e6f0, 0x1e6f4), (0x1e6fe, 0x1e6ff),
    (0x1e7e0, 0x1e7e6), (0x1e7e8, 0x1e7eb), (0x1e7ed, 0x1e7ee), (0x1e7f0, 0x1e7fe), (0x1e800, 0x1e8c4),
    (0x1e8c7, 0x1e8cf), (0x1e900, 0x1e943), (0x1e94b, 0x1e94b), (0x1e950, 0x1e959), (0x1ec71, 0x1ecab),
    (0x1ecad, 0x1ecaf), (0x1ecb1, 0x1ecb4), (0x1ed01, 0x1ed2d), (0x1ed2f, 0x1ed3d), (0x1ee00, 0x1ee03),
    (0x1ee05, 0x1ee1f), (0x1ee21, 0x1ee22), (0x1ee24, 0x1ee24), (0x1ee27, 0x1ee27), (0x1ee29, 0x1ee32),
    (0x1ee34, 0x1ee37), (0x1ee39, 0x1ee39), (0x1ee3b, 0x1ee3b), (0x1ee42, 0x1ee42), (0x1ee47, 0x1ee47),
    (0x1ee49, 0x1ee49), (0x1ee4b, 0x1ee4b), (0x1ee4d, 0x1ee4f), (0x1ee51, 0x1ee52), (0x1ee54, 0x1ee54),
    (0x1ee57, 0x1ee57), (0x1ee59, 0x1ee59), (0x1ee5b, 0x1ee5b), (0x1ee5d, 0x1ee5d), (0x1ee5f, 0x1ee5f),
    (0x1ee61, 0x1ee62), (0x1ee64, 0x1ee64), (0x1ee67, 0x1ee6a), (0x1ee6c, 0x1ee72), (0x1ee74, 0x1ee77),
    (0x1ee79, 0x1ee7c), (0x1ee7e, 0x1ee7e), (0x1ee80, 0x1ee89), (0x1ee8b, 0x1ee9b), (0x1eea1, 0x1eea3),
    (0x1eea5, 0x1eea9), (0x1eeab, 0x1eebb), (0x1f100, 0x1f10c), (0x1fbf0, 0x1fbf9), (0x20000, 0x2a6df),
    (0x2a700, 0x2b81d), (0x2b820, 0x2cead), (0x2ceb0, 0x2ebe0), (0x2ebf0, 0x2ee5d), (0x2f800, 0x2fa1d),
    (0x30000, 0x3134a), (0x31350, 0x33479),
];
