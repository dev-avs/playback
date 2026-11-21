//! Port of `src/lyrics/uroman.ts`: a universal romanizer (a port of uroman's
//! lattice romanizer, https://github.com/isi-nlp/uroman; "This project uses the
//! universal romanizer software 'uroman' written by Ulf Hermjakob, USC
//! Information Sciences Institute (2015-2020)"). Its data is `data/uroman.json`
//! (exported from `src/vendor/uroman-data.ts` by `scripts/golden/gen-uroman-data.mjs`),
//! licence in `data/uroman-LICENSE.txt`.
//!
//! JavaScript string semantics: the TypeScript works on `Array.from(text)`, i.e.
//! code points, so the lattice holds a `Vec<char>`. Where the TS indexes or
//! measures a romanization with `[i]` / `.length` / `.slice` (UTF-16 units),
//! the data's targets are all in the BMP, so a `char` is one unit; the one place
//! an edge text's last unit is tested (`is_last_unit_alpha`) treats a non-BMP
//! last char as a lone surrogate (not a letter), as JS does.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

// src/lyrics/uroman.ts:19 (UromanData), parsed from data/uroman.json
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UromanData {
    /// source string -> [target, restriction flags, language codes?, target at syllable end?]
    rules: HashMap<String, Vec<Vec<Value>>>,
    abugida: HashMap<String, Vec<String>>,
    scripts: HashMap<String, String>,
    vowel_sign: String,
    medial: String,
    virama: String,
    tone_mark: String,
    pre_post: String,
    nukta: String,
    subjoined: String,
    vocalic: String,
    digits: Vec<String>,
}

// src/lyrics/uroman.ts:39
const ONLY_START: u32 = 1;
const DONT_START: u32 = 2;
const ONLY_END: u32 = 4;
const DONT_END: u32 = 8;
const WHOLE_WORD: u32 = 16;

// src/lyrics/uroman.ts:45
struct Rule {
    t: String,
    flags: u32,
    n_restr: u32,
    lcodes: Vec<String>,
    t_eos: Option<String>,
}

// src/lyrics/uroman.ts:47 (the `tables` shape; abugida vowel regexes are built once here)
struct Tables {
    rules: HashMap<String, Vec<Rule>>,
    prefix: HashSet<String>,
    script: HashMap<char, String>,
    abugida: HashMap<String, Vec<String>>,
    abugida_re: HashMap<String, (Regex, Regex)>,
    vowel_sign: HashSet<char>,
    medial: HashSet<char>,
    virama: HashSet<char>,
    tone_mark: HashSet<char>,
    pre_post: HashSet<char>,
    nukta: HashSet<char>,
    subjoined: HashSet<char>,
    vocalic: HashSet<char>,
    digit: HashMap<char, String>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(build_tables);

// src/lyrics/uroman.ts:65
fn load() -> &'static Tables {
    &TABLES
}

fn build_tables() -> Tables {
    let d: UromanData = serde_json::from_str(include_str!("../data/uroman.json")).expect("data/uroman.json");
    let mut rules: HashMap<String, Vec<Rule>> = HashMap::new();
    let mut prefix: HashSet<String> = HashSet::new();
    for (s, rs) in &d.rules {
        rules.insert(
            s.clone(),
            rs.iter()
                .map(|r| {
                    let t = r[0].as_str().unwrap().to_string();
                    let flags = r[1].as_u64().unwrap() as u32;
                    let lcodes: Vec<String> = match r.get(2) {
                        Some(Value::Array(a)) => a.iter().map(|x| x.as_str().unwrap().to_string()).collect(),
                        _ => Vec::new(),
                    };
                    let t_eos = r.get(3).and_then(|x| x.as_str()).map(|x| x.to_string());
                    let mut n_restr = if !lcodes.is_empty() { 1 } else { 0 };
                    n_restr += flags.count_ones();
                    Rule { t, flags, n_restr, lcodes, t_eos }
                })
                .collect(),
        );
        let cs: Vec<char> = s.chars().collect();
        for i in 1..=cs.len() {
            prefix.insert(cs[..i].iter().collect());
        }
    }
    let mut script: HashMap<char, String> = HashMap::new();
    for (name, chars) in &d.scripts {
        for c in chars.chars() {
            script.insert(c, name.clone());
        }
    }
    let set = |chars: &str| -> HashSet<char> { chars.chars().collect() };
    let mut digit: HashMap<char, String> = HashMap::new();
    for (value, chars) in d.digits.iter().enumerate() {
        for c in chars.chars() {
            digit.insert(c, value.to_string());
        }
    }
    let mut abugida_re = HashMap::new();
    for (name, vowels) in &d.abugida {
        if vowels.is_empty() {
            continue;
        }
        let plus: Vec<String> = vowels.iter().map(|v| format!("{v}+")).collect();
        let m1 = Regex::new(&format!("^([cfghkmnqrstxy]?y)({})-?$", plus.join("|"))).unwrap();
        let m2 = Regex::new(&format!("^([bcdfghjklmnpqrstvwxyz]+)({})-?$", vowels.join("|"))).unwrap();
        abugida_re.insert(name.clone(), (m1, m2));
    }
    Tables {
        rules,
        prefix,
        script,
        abugida: d.abugida.clone(),
        abugida_re,
        vowel_sign: set(&d.vowel_sign),
        medial: set(&d.medial),
        virama: set(&d.virama),
        tone_mark: set(&d.tone_mark),
        pre_post: set(&d.pre_post),
        nukta: set(&d.nukta),
        subjoined: set(&d.subjoined),
        vocalic: set(&d.vocalic),
        digit,
    }
}

// src/lyrics/uroman.ts:107 (letters in scripts this romanizer handles: not Latin, not CJK)
static UROMAN_SCRIPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\p{L}--\p{Script=Latin}--\p{Script=Han}--\p{Script=Hiragana}--\p{Script=Katakana}--\p{Script=Hangul}]").unwrap()
});
static RE_L: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{L}").unwrap());
static RE_LM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{M}]").unwrap());
static RE_MN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Mn}").unwrap());
static RE_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{Mn}\p{Cf}\p{Co}]").unwrap());
static RE_PLUS_TAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\+(m|ng|n|h|r)").unwrap());
static RE_R_VOWEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"r[aeiou]").unwrap());
static RE_DOUBLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(ch|[bcdfghjklmnpqrstwz])").unwrap());

fn test_char(re: &Regex, c: char) -> bool {
    let mut b = [0u8; 4];
    re.is_match(c.encode_utf8(&mut b))
}

// src/lyrics/uroman.ts:111
pub fn has_uroman_script(text: &str) -> bool {
    UROMAN_SCRIPT.is_match(text)
}

// src/lyrics/uroman.ts:115
fn is_alpha(c: Option<char>) -> bool {
    matches!(c, Some(c) if test_char(&RE_L, c))
}
// src/lyrics/uroman.ts:116
fn is_letter_or_mark(c: Option<char>) -> bool {
    matches!(c, Some(c) if test_char(&RE_LM, c))
}
// src/lyrics/uroman.ts:117
fn is_upper(s: &str) -> bool {
    s != s.to_lowercase() && s == s.to_uppercase()
}
// src/lyrics/uroman.ts:118
fn is_lower(s: &str) -> bool {
    s != s.to_uppercase() && s == s.to_lowercase()
}
// src/lyrics/uroman.ts:119
fn is_mn(c: Option<char>) -> bool {
    matches!(c, Some(c) if test_char(&RE_MN, c))
}
// src/lyrics/uroman.ts:120 (/^[bcdfghjklmnpqrstvwxyz]+$/)
fn consonants(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| "bcdfghjklmnpqrstvwxyz".contains(c))
}
// src/lyrics/uroman.ts:195 / 172: `\p{L}` on the last UTF-16 unit of a string (a non-BMP last char is a lone surrogate)
fn is_last_unit_alpha(txt: &str) -> bool {
    match txt.chars().last() {
        Some(c) if (c as u32) <= 0xFFFF => test_char(&RE_L, c),
        _ => false,
    }
}
fn has(set: &HashSet<char>, c: Option<char>) -> bool {
    matches!(c, Some(c) if set.contains(&c))
}

// src/lyrics/uroman.ts:122
#[derive(Clone, Serialize)]
pub struct Edge {
    pub start: usize,
    pub end: usize,
    pub txt: String,
    pub r#type: String,
}

// src/lyrics/uroman.ts:124
pub struct Lattice {
    t: &'static Tables,
    s: Vec<char>,
    n: usize,
    spans: HashMap<(usize, usize), Vec<Edge>>,
    rights: HashMap<usize, BTreeSet<usize>>,
    lefts: HashMap<usize, BTreeSet<usize>>,
    preceded_by_alpha: HashMap<usize, bool>,
    followed_by_alpha: HashMap<usize, bool>,
    top_cache: HashMap<(usize, usize), String>,
}

impl Lattice {
    // src/lyrics/uroman.ts:135
    pub fn new(text: &str) -> Lattice {
        let s: Vec<char> = text.chars().collect();
        let n = s.len();
        Lattice {
            t: load(),
            s,
            n,
            spans: HashMap::new(),
            rights: HashMap::new(),
            lefts: HashMap::new(),
            preceded_by_alpha: HashMap::new(),
            followed_by_alpha: HashMap::new(),
            top_cache: HashMap::new(),
        }
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    // src/lyrics/uroman.ts:140
    fn sub(&self, a: usize, b: usize) -> String {
        let b = b.min(self.n);
        if a >= b {
            return String::new();
        }
        self.s[a..b].iter().collect()
    }

    // src/lyrics/uroman.ts:144
    fn script_of(&self, c: Option<char>) -> &'static str {
        match c.and_then(|c| self.t.script.get(&c)) {
            Some(name) => name.as_str(),
            None => "",
        }
    }

    // src/lyrics/uroman.ts:148
    fn add_edge(&mut self, e: Edge) {
        let list = self.spans.entry((e.start, e.end)).or_default();
        if !list.iter().any(|o| o.txt == e.txt && o.r#type == e.r#type) {
            list.push(e.clone());
        }
        self.rights.entry(e.start).or_default().insert(e.end);
        self.lefts.entry(e.end).or_default().insert(e.start);
    }

    // src/lyrics/uroman.ts:159
    fn edges(&self, a: usize, b: usize) -> &[Edge] {
        self.spans.get(&(a, b)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    // src/lyrics/uroman.ts:163
    fn is_at_start_of_word(&mut self, position: usize) -> bool {
        if let Some(&cached) = self.preceded_by_alpha.get(&position) {
            return !cached;
        }
        if position != 0 && is_alpha(self.at(position - 1)) {
            self.preceded_by_alpha.insert(position, true);
            return false;
        }
        if let Some(starts) = self.lefts.get(&position) {
            for &start in starts {
                for e in self.edges(start, position) {
                    if !e.txt.is_empty() && is_last_unit_alpha(&e.txt) {
                        self.preceded_by_alpha.insert(position, true);
                        return false;
                    }
                }
            }
        }
        self.preceded_by_alpha.insert(position, false);
        true
    }

    // src/lyrics/uroman.ts:182
    fn is_at_end_of_word(&mut self, position: usize) -> bool {
        if let Some(&cached) = self.followed_by_alpha.get(&position) {
            return !cached;
        }
        let mut start = position;
        if start < self.n && is_alpha(self.at(start)) {
            self.followed_by_alpha.insert(position, true);
            return false;
        }
        while start + 1 < self.n && is_mn(self.at(start)) && has(&self.t.nukta, self.at(start)) {
            start += 1;
        }
        let mut end = start + 1;
        while end <= self.n {
            let sub = self.sub(start, end);
            if !self.t.prefix.contains(&sub) {
                break;
            }
            if let Some(rs) = self.t.rules.get(&sub) {
                for r in rs {
                    if r.flags & ONLY_START == 0 && RE_L.is_match(&r.t) {
                        self.followed_by_alpha.insert(position, true);
                        return false;
                    }
                }
            }
            end += 1;
        }
        self.followed_by_alpha.insert(position, false);
        true
    }

    // src/lyrics/uroman.ts:205
    fn is_at_end_of_syllable(&mut self, position: usize) -> bool {
        let prev = if position >= 2 { self.at(position - 2) } else { None };
        let mut next = if position < self.n { self.at(position) } else { None };
        let mut adj = position;
        if has(&self.t.tone_mark, next) {
            adj = position + 1;
            next = if adj < self.n { self.at(adj) } else { None };
        }
        if prev.is_none() {
            return false;
        }
        if !is_letter_or_mark(prev) {
            return false;
        }
        if has(&self.t.pre_post, prev) {
            return false;
        }
        if has(&self.t.pre_post, next) {
            return true;
        }
        if adj >= self.n {
            return true;
        }
        if !is_letter_or_mark(next) {
            return true;
        }
        if position > 0 {
            let left = self.best_left_neighbor(position - 1);
            if let Some(left) = left {
                if matches!(left.txt.chars().last(), Some(c) if "bcdfghjklmnpqrstvxz".contains(c)) {
                    return false;
                }
            }
        }
        let next_rom = match self.top_candidate(adj, adj + 2, true) {
            Some(r) => r,
            None => self.top_candidate(adj, adj + 1, true).unwrap_or_else(|| "?".to_string()),
        };
        if !matches!(next_rom.to_lowercase().chars().next(), Some(c) if "aeiou".contains(c)) {
            return true;
        }
        if next == Some('อ') && adj + 1 < self.n {
            // THAI CHARACTER O ANG: a consonant when a vowel follows
            let next2_rom = self.top_candidate(adj + 1, adj + 2, true).unwrap_or_else(|| "?".to_string());
            if matches!(next2_rom.to_lowercase().chars().next(), Some(c) if "aeiou".contains(c)) {
                return true;
            }
        }
        false
    }

    // src/lyrics/uroman.ts:235
    fn first_rule(&self, c: &str) -> Option<&'static str> {
        self.t.rules.get(c).and_then(|v| v.first()).map(|r| r.t.as_str())
    }

    // src/lyrics/uroman.ts:239
    fn valid(&mut self, r: &Rule, start: usize, end: usize) -> bool {
        if r.flags & DONT_START != 0 && self.is_at_start_of_word(start) {
            return false;
        }
        if r.flags & ONLY_START != 0 && !self.is_at_start_of_word(start) {
            return false;
        }
        if r.flags & DONT_END != 0 && self.is_at_end_of_word(end) {
            return false;
        }
        if r.flags & ONLY_END != 0 && !self.is_at_end_of_word(end) {
            return false;
        }
        if r.flags & WHOLE_WORD != 0 && !(self.is_at_start_of_word(start) && self.is_at_end_of_word(end)) {
            return false;
        }
        // no language code is known, so language-specific rules never apply
        r.lcodes.is_empty()
    }

    // src/lyrics/uroman.ts:250
    fn top_candidate(&mut self, start: usize, end: usize, simple: bool) -> Option<String> {
        if end > self.n {
            return None;
        }
        let key = (start, end);
        if let Some(cached) = self.top_cache.get(&key) {
            return Some(cached.clone());
        }
        let mut best: Option<&'static Rule> = None;
        let t = self.t;
        if let Some(rs) = t.rules.get(&self.sub(start, end)) {
            for r in rs {
                if self.valid(r, start, end) && (best.is_none() || r.n_restr > best.unwrap().n_restr) {
                    best = Some(r);
                }
            }
        }
        if simple {
            return best.map(|b| b.t.clone());
        }
        let mut cand = best.map(|b| b.t.clone());
        if let Some(b) = best {
            if let Some(eos) = &b.t_eos {
                if self.is_at_end_of_syllable(end) {
                    cand = Some(eos.clone());
                }
            }
        }
        if let Some(c) = &cand {
            self.top_cache.insert(key, c.clone());
        }
        cand
    }

    // src/lyrics/uroman.ts:266
    fn add_abugida_vowel(&mut self, rom: String, start: usize, end: usize, annotation: &str) -> String {
        let mut rom = rom;
        let script_name = self.script_of(self.at(start));
        let vowels = match self.t.abugida.get(script_name) {
            Some(v) if !v.is_empty() => v,
            _ => return rom,
        };
        let (re1, re2) = &self.t.abugida_re[script_name];
        let base: String;
        let base_plus: String;
        if let Some(m1) = re1.captures(&rom) {
            base = m1[1].to_string();
            base_plus = format!("{}{}", base, &m1[2]);
        } else if let Some(m2) = re2.captures(&rom) {
            base = m2[1].to_string();
            base_plus = format!("{}{}", base, &m2[2]);
            if rom.ends_with('-') && start + 1 == end && is_alpha(rom.chars().next()) {
                rom.pop();
            }
        } else {
            base = rom.clone();
            base_plus = format!("{}{}", base, vowels[0]);
        }
        if !consonants(&base) {
            return rom;
        }
        if annotation.contains("tail") {
            return rom;
        }
        let prev = if start >= 1 { self.at(start - 1) } else { None };
        let next = self.at(end);
        let next2 = self.at(end + 1);
        if next.is_some() && ("bcdfghklmnpqrstvwz".contains(base.as_str()) || base == "ng") && next == Some('យ') {
            return base;
        }
        if has(&self.t.vowel_sign, next) {
            return base;
        }
        if has(&self.t.medial, next) {
            return base;
        }
        if has(&self.t.subjoined, next) {
            return base;
        }
        if is_mn(next) && has(&self.t.vowel_sign, next2) {
            return base;
        }
        if has(&self.t.virama, next) {
            return base;
        }
        if is_mn(next) && has(&self.t.virama, next2) {
            return base;
        }
        if has(&self.t.virama, prev) {
            return base_plus;
        }
        if self.is_at_start_of_word(start) && !RE_R_VOWEL.is_match(&rom) {
            return base_plus;
        }
        // Most Devanagari languages (and Assamese, Bengali, Gujarati, Kashmiri,
        // Punjabi) drop the final schwa; without a language code that is the rule
        // for Devanagari only, as in uroman.
        if self.is_at_end_of_word(end) {
            return if script_name == "Devanagari" { rom } else { base_plus };
        }
        if self.script_of(prev) != script_name {
            return base_plus;
        }
        if has(&self.t.vocalic, self.at(end - 1)) {
            return base;
        }
        if self.script_of(next) == script_name {
            return base_plus;
        }
        rom
    }

    // src/lyrics/uroman.ts:310
    fn expand(&mut self, rom: String, start: usize, end: usize) -> (String, usize, usize, Option<&'static str>) {
        let mut rom = rom;
        let mut start = start;
        if rom.is_empty() {
            return (rom, start, end, None);
        }
        let mut prev = if start >= 1 { self.at(start - 1) } else { None };
        let mut first = self.at(start);
        let next = if end < self.n { self.at(end) } else { None };
        let next_s: String = next.map(|c| c.to_string()).unwrap_or_default();
        // THessalonike -> Thessalonike, but THESSALONIKE stays
        if start + 1 == end && is_upper(&rom) && is_lower(&next_s) {
            let mut cs = rom.chars();
            let f = cs.next().unwrap();
            rom = f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase();
        }
        // Gurmukhi addak doubles the next consonant
        let dbl = if prev == Some('ੱ') { RE_DOUBLE.captures(&rom).map(|m| m[1].to_string()) } else { None };
        if let Some(d) = dbl {
            rom = d.replacen("ch", "c", 1) + &rom;
            start -= 1;
            first = self.at(start);
            prev = if start >= 1 { self.at(start - 1) } else { None };
        }
        if self.script_of(first) == "Thai" {
            if start + 1 == end && consonants(&rom) && has(&self.t.pre_post, prev) && start >= 1 {
                for suffix in [3usize, 2, 1] {
                    if end + suffix > self.n {
                        continue;
                    }
                    let pattern = format!("{}–{}", self.s[start - 1], self.sub(end, end + suffix));
                    let vowel = self.t.rules.get(&pattern).and_then(|v| v.first());
                    if let Some(vowel) = vowel {
                        return (format!("{}{}", rom, vowel.t), start - 1, end + suffix, Some("rom exp"));
                    }
                }
            }
            let vowel_rom = if self.script_of(prev) == "Thai" && has(&self.t.pre_post, prev) {
                self.first_rule(&prev.unwrap().to_string()).filter(|s| !s.is_empty())
            } else {
                None
            };
            if let Some(vowel_rom) = vowel_rom {
                if matches!(rom.chars().next(), Some(c) if "bcdfghjklmnpqrstvwxyz".contains(c)) {
                    return (format!("{}{}", rom, vowel_rom), start - 1, end, Some("rom exp"));
                }
            }
            if first == Some('อ') && end - start == 1 {
                let prev_rom = self.path_backwards(start, 1);
                let next_rom = self.first_rule(&next_s).unwrap_or("");
                if !(self.script_of(prev) == "Thai"
                    && self.script_of(next) == "Thai"
                    && thai_consonant(&prev_rom)
                    && thai_consonant(next_rom))
                {
                    return (String::new(), start, end, Some("rom del"));
                }
            }
        }
        if has(&self.t.virama, next) {
            return (rom, start, end + 1, Some("rom exp"));
        }
        if rom.starts_with(' ') && (start == 0 || prev == Some(' ')) {
            rom = rom[1..].to_string();
        }
        if rom.ends_with(' ') && next == Some(' ') {
            rom.pop();
        }
        (rom, start, end, None)
    }

    // src/lyrics/uroman.ts:363
    fn add_romanization(&mut self) {
        for start in 0..self.n {
            for end in start + 1..=self.n {
                if !self.t.prefix.contains(&self.sub(start, end)) {
                    break;
                }
                let mut rom = match self.top_candidate(start, end, false) {
                    Some(r) => r,
                    None => continue,
                };
                let mut annotation = "rom";
                if RE_PLUS_TAIL.is_match(&rom) {
                    rom = rom[1..].to_string();
                    annotation = "rom tail";
                }
                let rom = self.add_abugida_vowel(rom, start, end, annotation);
                let (txt, a, b, exp) = self.expand(rom, start, end);
                self.add_edge(Edge { start: a, end: b, txt, r#type: exp.unwrap_or(annotation).to_string() });
            }
        }
    }

    // src/lyrics/uroman.ts:381
    fn add_fallback_singles(&mut self) {
        for start in 0..self.n {
            let end = start + 1;
            if !self.edges(start, end).is_empty() {
                continue;
            }
            let c = self.s[start];
            let mut rom = c.to_string();
            let mut r#type = "orig";
            if test_char(&RE_MARK, c) {
                rom = String::new();
                r#type = "mark";
            } else if let Some(d) = self.t.digit.get(&c) {
                rom = d.clone();
            } else if c != ' ' {
                if let Some(single) = self.top_candidate(start, end, false) {
                    rom = match RE_PLUS_TAIL.captures(&single) {
                        Some(m) => format!("{}{}", &m[1], &single[m.get(0).unwrap().end()..]),
                        None => single,
                    };
                    r#type = "rom single";
                }
            }
            self.add_edge(Edge { start, end, txt: rom, r#type: r#type.to_string() });
        }
    }

    // src/lyrics/uroman.ts:404
    fn best_in_span(&self, a: usize, b: usize) -> Option<Edge> {
        let list = self.edges(a, b);
        list.iter().find(|e| e.r#type.starts_with("rom")).or_else(|| list.first()).cloned()
    }

    // src/lyrics/uroman.ts:409
    fn best_right_neighbor(&self, start: usize) -> Option<Edge> {
        if let Some(ends) = self.rights.get(&start) {
            for &end in ends.iter().rev() {
                if let Some(e) = self.best_in_span(start, end) {
                    return Some(e);
                }
            }
        }
        None
    }

    // src/lyrics/uroman.ts:417
    fn best_left_neighbor(&self, end: usize) -> Option<Edge> {
        if let Some(starts) = self.lefts.get(&end) {
            for &start in starts.iter() {
                if let Some(e) = self.best_in_span(start, end) {
                    return Some(e);
                }
            }
        }
        None
    }

    // src/lyrics/uroman.ts:425
    fn path_backwards(&self, end: usize, min_char: usize) -> String {
        let mut rom = String::new();
        let mut end2 = end;
        while end2 > 0 {
            let old = end2;
            let e = self.best_left_neighbor(end2);
            if let Some(e) = e {
                rom = e.txt + &rom;
                end2 = e.start;
            }
            if rom.encode_utf16().count() >= min_char {
                break;
            }
            if old >= end2 {
                end2 -= 1;
            }
        }
        rom
    }

    /// The best path through the lattice, as edges over code point offsets.
    // src/lyrics/uroman.ts:442
    pub fn romanize(&mut self) -> Vec<Edge> {
        self.add_romanization();
        self.add_fallback_singles();
        let mut path = Vec::new();
        let mut i = 0;
        while i < self.n {
            match self.best_right_neighbor(i) {
                None => {
                    i += 1;
                }
                Some(e) => {
                    i = e.end;
                    path.push(e);
                }
            }
        }
        path
    }
}

// src/lyrics/uroman.ts:344 (/^[bcdfghjklmnpqrstvwxz]+$/, no y)
fn thai_consonant(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| "bcdfghjklmnpqrstvwxz".contains(c))
}

// src/lyrics/uroman.ts:470 (presentation forms fold by NFKC; a Hebrew word-final he after a vowel
// point is swapped for a zero-width non-joiner; src/lyrics/uroman.ts:461-468)
pub fn prepare(text: &str) -> String {
    let folded: Vec<char> = text
        .chars()
        .flat_map(|c| {
            let u = c as u32;
            if (0xfb50..=0xfdff).contains(&u) || (0xfe70..=0xfeff).contains(&u) {
                c.to_string().nfkc().collect::<Vec<char>>()
            } else {
                vec![c]
            }
        })
        .collect();
    // /(?<=\p{Mn})ה(?![\p{L}\p{M}])/gu -> U+200C, tested against the unreplaced text
    let mut out = String::new();
    for (i, &c) in folded.iter().enumerate() {
        let silent = c == 'ה'
            && i > 0
            && test_char(&RE_MN, folded[i - 1])
            && !matches!(folded.get(i + 1), Some(&n) if test_char(&RE_LM, n));
        out.push(if silent { '\u{200c}' } else { c });
    }
    out
}

// src/lyrics/uroman.ts:474
pub fn uromanize(text: &str) -> String {
    if !has_uroman_script(text) {
        return text.to_string();
    }
    Lattice::new(&prepare(text)).romanize().into_iter().map(|e| e.txt).collect::<Vec<_>>().join("")
}

/// Romanize a word given as its timed syllables.
// src/lyrics/uroman.ts:488
pub fn uromanize_parts(parts: &[String]) -> Vec<String> {
    if !parts.iter().any(|p| has_uroman_script(p)) {
        return parts.to_vec();
    }
    let prepared: Vec<String> = parts.iter().map(|p| prepare(p)).collect();
    let mut owner: Vec<usize> = Vec::new();
    for (i, p) in prepared.iter().enumerate() {
        owner.extend(p.chars().map(|_| i));
    }
    let mut out: Vec<String> = parts.iter().map(|_| String::new()).collect();
    for e in Lattice::new(&prepared.concat()).romanize() {
        out[owner[e.start]].push_str(&e.txt);
    }
    out
}
