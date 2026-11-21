//! Port of the one path of `pinyin-pro` 3.29.5 (MIT, see `data/pinyin-pro-LICENSE.txt`)
//! that `src/lyrics/romanize.ts` uses: `pinyin(text, { type: "all", toneType: "none" })`,
//! with every other option at its default (surname "off", segmentit MaxProbability,
//! toneSandhi on with only the yi/bu rules, nonZh "spaced", no traditional, no multiple).
//!
//! Its data is exported from `node_modules/pinyin-pro` by `scripts/golden/gen-pinyin-data.mjs`:
//! `data/pinyin-dict1.json` (DICT1, char -> readings) and `data/pinyin-words.json` (the word
//! patterns of dict5/4/3/2 and the number rules, in `PatternsNormal` order). The surname
//! patterns are left out: with surname "off" the matcher skips them.
//!
//! The source files followed are `dist/esm/core/pinyin/{index,handle,middlewares}.mjs`,
//! `common/segmentit/{index,max-probability}.mjs` and `data/special.mjs`. pinyin-pro splits
//! the text with `splitString` (code points, surrogate pairs kept whole), which is
//! `str::chars` here, and only compares/looks up whole characters, so no UTF-16 indexing
//! is needed.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// One element of `pinyin(..., { type: "all" })`, the three fields romanize.ts reads
/// (`origin`, `pinyin`, `isZh`).
#[derive(Clone, Debug, PartialEq)]
pub struct PinyinChar {
    pub origin: String,
    pub pinyin: String,
    pub is_zh: bool,
}

// ── Data ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WordGroup {
    probability: f64,
    words: Vec<(String, String)>,
}

/// dict1.mjs `DICT1`
static DICT1: LazyLock<HashMap<char, String>> = LazyLock::new(|| {
    let raw: HashMap<String, String> = serde_json::from_str(include_str!("../data/pinyin-dict1.json")).expect("data/pinyin-dict1.json");
    raw.into_iter().map(|(k, v)| (k.chars().next().expect("empty key"), v)).collect()
});

struct Pattern {
    pinyin: String,
    probability: f64,
    length: usize,
}

struct Node {
    children: HashMap<char, usize>,
    fail: Option<usize>,
    /// indexes into `Ac::patterns`, best first (`insertPattern` order)
    patterns: Vec<usize>,
}

/// common/segmentit/index.mjs `AC`
struct Ac {
    nodes: Vec<Node>,
    patterns: Vec<Pattern>,
}

fn new_node() -> Node {
    Node { children: HashMap::new(), fail: None, patterns: Vec::new() }
}

impl Ac {
    // segmentit/index.mjs `build` = buildTrie + buildFailPointer over `PatternsNormal`
    fn build(groups: &[WordGroup]) -> Ac {
        let mut ac = Ac { nodes: vec![new_node()], patterns: Vec::new() };
        // queues[depth]: nodes in creation order
        let mut queues: Vec<Vec<usize>> = Vec::new();
        for g in groups {
            for (zh, py) in &g.words {
                let chars: Vec<char> = zh.chars().collect();
                let mut cur = 0usize;
                for (i, c) in chars.iter().enumerate() {
                    cur = match ac.nodes[cur].children.get(c) {
                        Some(&n) => n,
                        None => {
                            let n = ac.nodes.len();
                            ac.nodes.push(new_node());
                            ac.nodes[cur].children.insert(*c, n);
                            if queues.len() <= i + 1 {
                                queues.resize(i + 2, Vec::new());
                            }
                            queues[i + 1].push(n);
                            n
                        }
                    };
                }
                ac.patterns.push(Pattern { pinyin: py.clone(), probability: g.probability, length: chars.len() });
                let pi = ac.patterns.len() - 1;
                ac.insert_pattern(cur, pi);
            }
        }
        // buildFailPointer: parents are deeper-first-complete, so a node's parent's fail is set
        let mut parent_of = vec![0usize; ac.nodes.len()];
        for (n, node) in ac.nodes.iter().enumerate() {
            for &c in node.children.values() {
                parent_of[c] = n;
            }
        }
        let mut key_of = vec!['\0'; ac.nodes.len()];
        for node in ac.nodes.iter() {
            for (&k, &c) in node.children.iter() {
                key_of[c] = k;
            }
        }
        for queue in queues.iter().skip(1) {
            for &n in queue {
                let key = key_of[n];
                let mut fail_node = ac.nodes[parent_of[n]].fail;
                while let Some(f) = fail_node {
                    if ac.nodes[f].children.contains_key(&key) {
                        break;
                    }
                    fail_node = ac.nodes[f].fail;
                }
                ac.nodes[n].fail = Some(match fail_node {
                    None => 0,
                    Some(f) => ac.nodes[f].children[&key],
                });
            }
        }
        ac
    }

    // segmentit/index.mjs `insertPattern` (all patterns here have the same priority, Normal)
    fn insert_pattern(&mut self, node: usize, pi: usize) {
        let prob = self.patterns[pi].probability;
        let list = &mut self.nodes[node].patterns;
        let mut at = 0;
        for i in (0..list.len()).rev() {
            if prob >= self.patterns[list[i]].probability {
                continue;
            }
            at = i + 1;
            break;
        }
        list.insert(at, pi);
    }

    // segmentit/index.mjs `match` with surname "off": the first non-surname pattern
    // of the node, then of each node along its fail chain.
    fn matches(&self, chars: &[char]) -> Vec<Match> {
        let mut cur = 0usize;
        let mut result = Vec::new();
        for (i, c) in chars.iter().enumerate() {
            let mut next = self.nodes[cur].children.get(c).copied();
            while next.is_none() && cur != 0 {
                match self.nodes[cur].fail {
                    None => {
                        cur = 0;
                        break;
                    }
                    Some(f) => {
                        cur = f;
                        next = self.nodes[cur].children.get(c).copied();
                    }
                }
            }
            if let Some(n) = next {
                cur = n;
                if let Some(&p) = self.nodes[cur].patterns.first() {
                    result.push(Match { pattern: p, index: i + 1 - self.patterns[p].length });
                }
                let mut fail_node = self.nodes[cur].fail;
                while let Some(f) = fail_node {
                    if let Some(&p) = self.nodes[f].patterns.first() {
                        result.push(Match { pattern: p, index: i + 1 - self.patterns[p].length });
                    }
                    fail_node = self.nodes[f].fail;
                }
            }
        }
        result
    }
}

#[derive(Clone, Copy)]
struct Match {
    pattern: usize,
    index: usize,
}

static AC: LazyLock<Ac> = LazyLock::new(|| {
    // PatternsNormal order: dict5, dict4, dict3, dict2, number rules (surnames dropped)
    let groups: Vec<WordGroup> = serde_json::from_str(include_str!("../data/pinyin-words.json")).expect("data/pinyin-words.json");
    Ac::build(&groups)
});

// ── Segmentation: common/segmentit/max-probability.mjs ────────

#[derive(Clone, Copy)]
struct Dp {
    probability: f64,
    decimal: f64,
    pattern: Option<Match>,
    next_index: usize,
}

// max-probability.mjs `getMaxProbability`
fn get_max_probability(a: Option<Dp>, b: Dp) -> Dp {
    match a {
        None => b,
        Some(a) => {
            if a.decimal < b.decimal {
                a
            } else if a.decimal == b.decimal {
                if a.probability > b.probability { a } else { b }
            } else {
                b
            }
        }
    }
}

// max-probability.mjs `checkDecimal`
fn check_decimal(p: &mut Dp) {
    if p.probability < 1e-300 {
        p.probability *= 1e300;
        p.decimal += 1.0;
    }
}

// max-probability.mjs `maxProbability`
fn max_probability(ac: &Ac, patterns: &[Match], length: usize) -> Vec<Match> {
    let mut dp: Vec<Option<Dp>> = vec![None; length];
    let terminal = Dp { probability: 1.0, decimal: 0.0, pattern: None, next_index: length };
    let mut pattern_index = patterns.len() as isize - 1;
    let end_of = |m: &Match| m.index + ac.patterns[m.pattern].length - 1;
    for i in (0..length).rev() {
        let suffix = if i + 1 >= length { terminal } else { dp[i + 1].unwrap() };
        let next_index = if suffix.pattern.is_some() { i + 1 } else { suffix.next_index };
        while pattern_index >= 0 && end_of(&patterns[pattern_index as usize]) == i {
            let m = patterns[pattern_index as usize];
            let start = m.index;
            let mut cur = Dp {
                probability: ac.patterns[m.pattern].probability * suffix.probability,
                decimal: suffix.decimal + 0.0,
                pattern: Some(m),
                next_index,
            };
            check_decimal(&mut cur);
            dp[start] = Some(get_max_probability(dp[start], cur));
            pattern_index -= 1;
        }
        let mut i_dp = Dp { probability: 1e-13 * suffix.probability, decimal: 0.0, pattern: None, next_index };
        check_decimal(&mut i_dp);
        dp[i] = Some(get_max_probability(dp[i], i_dp));
    }
    let mut result = Vec::new();
    let mut index = 0;
    while index < length {
        let item = dp[index].unwrap();
        if let Some(p) = item.pattern {
            result.push(p);
        }
        index = item.next_index;
    }
    result
}

// ── Tones: core/pinyin/handle.mjs ─────────────────────────────

/// `getPinyinWithoutTone`: the replacements run in this order.
fn get_pinyin_without_tone(pinyin: &str) -> String {
    const MAP: &[(&[&str], &str)] = &[
        (&["\u{101}", "\u{e1}", "\u{1ce}", "\u{e0}"], "a"),
        (&["\u{14d}", "\u{f3}", "\u{1d2}", "\u{f2}"], "o"),
        (&["\u{113}", "\u{e9}", "\u{11b}", "\u{e8}"], "e"),
        (&["\u{12b}", "\u{ed}", "\u{1d0}", "\u{ec}"], "i"),
        (&["\u{16b}", "\u{fa}", "\u{1d4}", "\u{f9}"], "u"),
        (&["\u{1d6}", "\u{1d8}", "\u{1da}", "\u{1dc}"], "\u{fc}"),
        (&["n\u{304}", "\u{144}", "\u{148}", "\u{1f9}"], "n"),
        (&["m\u{304}", "\u{1e3f}", "m\u{30c}", "m\u{300}"], "m"),
        (&["\u{ea}\u{304}", "\u{1ebf}", "\u{ea}\u{30c}", "\u{1ec1}"], "\u{ea}"),
    ];
    let mut s = pinyin.to_string();
    for (alts, to) in MAP {
        for a in *alts {
            s = s.replace(a, to);
        }
    }
    s
}

/// `getNumOfTone` for one syllable (the only form used here: no spaces), as the
/// string `Number(...)` then reads.
fn get_num_of_tone(pinyin: &str) -> &'static str {
    const T1: &[&str] = &["\u{101}", "\u{14d}", "\u{113}", "\u{12b}", "\u{16b}", "\u{1d6}", "n\u{304}", "m\u{304}", "\u{ea}\u{304}"];
    const T2: &[&str] = &["\u{e1}", "\u{f3}", "\u{e9}", "\u{ed}", "\u{fa}", "\u{1d8}", "\u{144}", "\u{1e3f}", "\u{1ebf}"];
    const T3: &[&str] = &["\u{1ce}", "\u{1d2}", "\u{11b}", "\u{1d0}", "\u{1d4}", "\u{1da}", "\u{148}", "m\u{30c}", "\u{ea}\u{30c}"];
    const T4: &[&str] = &["\u{e0}", "\u{f2}", "\u{e8}", "\u{ec}", "\u{f9}", "\u{1dc}", "\u{1f9}", "m\u{300}", "\u{1ec1}"];
    const T0: &[&str] = &["a", "o", "e", "i", "u", "\u{fc}", "\u{ea}"];
    let has = |set: &[&str]| set.iter().any(|a| pinyin.contains(a));
    if has(T1) {
        "1"
    } else if has(T2) {
        "2"
    } else if has(T3) {
        "3"
    } else if has(T4) {
        "4"
    } else if has(T0) || pinyin.ends_with('n') || pinyin.ends_with('m') {
        "0"
    } else {
        ""
    }
}

/// `getSingleWordPinyin`: the first reading, or the character itself when unknown.
fn get_single_word_pinyin(ch: &str) -> String {
    let c = ch.chars().next();
    match c.and_then(|c| DICT1.get(&c)) {
        Some(py) => py.split(' ').next().unwrap_or("").to_string(),
        None => ch.to_string(),
    }
}

fn in_dict1(ch: &str) -> bool {
    ch.chars().next().is_some_and(|c| DICT1.contains_key(&c))
}

// special.mjs `toneSandhiIgnoreSuffix`
fn tone_sandhi_ignore_suffix(cur: &str) -> &'static [&'static str] {
    if cur == "\u{4e0d}" {
        &["\u{7684}", "\u{800c}", "\u{4e4b}", "\u{540e}", "\u{4e5f}", "\u{8fd8}", "\u{5730}"]
    } else {
        &["\u{7684}", "\u{800c}", "\u{4e4b}", "\u{540e}", "\u{4e5f}", "\u{8fd8}", "\u{662f}"]
    }
}

// special.mjs `toneSandhiMap`, in key order: pinyin -> tones that trigger it
fn tone_sandhi_map(cur: &str) -> &'static [(&'static str, &'static [u32])] {
    if cur == "\u{4e0d}" {
        &[("b\u{fa}", &[4])]
    } else {
        &[("y\u{ed}", &[4]), ("y\u{ec}", &[1, 2, 3])]
    }
}

/// `Number(getNumOfTone(p))`
fn tone_number(p: &str) -> f64 {
    match get_num_of_tone(p) {
        "" => 0.0,
        s => s.parse().unwrap(),
    }
}

// handle.mjs `processToneSandhi` (yi / bu); None is `undefined`
fn process_tone_sandhi(cur: &str, pre: Option<&str>, next: Option<&str>) -> Option<String> {
    if cur != "\u{4e00}" && cur != "\u{4e0d}" {
        return Some(get_single_word_pinyin(cur));
    }
    // light tone in reduplication: 说不说, 说一说
    if let Some(p) = pre {
        if pre == next && get_single_word_pinyin(p) != p {
            return Some(get_pinyin_without_tone(&get_single_word_pinyin(cur)));
        }
    }
    if let Some(n) = next {
        if !tone_sandhi_ignore_suffix(cur).contains(&n) {
            let next_pinyin = get_single_word_pinyin(n);
            if next_pinyin != n {
                let next_tone = tone_number(&next_pinyin);
                for (pinyin, tones) in tone_sandhi_map(cur) {
                    if tones.iter().any(|&t| f64::from(t) == next_tone) {
                        return Some((*pinyin).to_string());
                    }
                }
            }
        }
    }
    None
}

// handle.mjs `processToneSandhiLiao`
fn process_tone_sandhi_liao(cur: &str, pre: Option<&str>) -> Option<String> {
    if cur == "\u{4e86}" && pre.is_none_or(|p| !in_dict1(p)) {
        return Some("li\u{1ce}o".to_string());
    }
    None
}

// handle.mjs `processReduplicationChar`
fn process_reduplication_char(cur: &str, pre: Option<&str>) -> Option<String> {
    if cur == "\u{3005}" {
        return Some(match pre {
            Some(p) if in_dict1(p) => DICT1[&p.chars().next().unwrap()].split(' ').next().unwrap().to_string(),
            _ => "t\u{f3}ng".to_string(),
        });
    }
    None
}

// handle.mjs `processSepecialPinyin` with `getProcessFuncs(text)`
fn process_special_pinyin(cur: &str, pre: Option<&str>, next: Option<&str>, has_reduplication: bool) -> String {
    // `if (res)`: an empty result falls through to the next function
    if has_reduplication {
        if let Some(r) = process_reduplication_char(cur, pre).filter(|r| !r.is_empty()) {
            return r;
        }
    }
    if let Some(r) = process_tone_sandhi_liao(cur, pre).filter(|r| !r.is_empty()) {
        return r;
    }
    if let Some(r) = process_tone_sandhi(cur, pre, next).filter(|r| !r.is_empty()) {
        return r;
    }
    get_single_word_pinyin(cur)
}

// handle.mjs `getPinyin` (surname off, not traditional)
fn get_pinyin(chars: &[char]) -> Vec<(String, String, bool)> {
    let ac = &*AC;
    let matches = max_probability(ac, &ac.matches(chars), chars.len());
    let has_reduplication = chars.contains(&'\u{3005}');
    let s = |i: usize| chars[i].to_string();
    let single = |i: usize| -> (String, String, bool) {
        let cur = s(i);
        let pre = if i > 0 { Some(s(i - 1)) } else { None };
        let next = if i + 1 < chars.len() { Some(s(i + 1)) } else { None };
        let py = process_special_pinyin(&cur, pre.as_deref(), next.as_deref(), has_reduplication);
        let is_zh = py != cur;
        (cur, py, is_zh)
    };
    let mut list: Vec<(String, String, bool)> = Vec::with_capacity(chars.len());
    let mut match_index = 0;
    let mut i = 0;
    while i < chars.len() {
        let m = matches.get(match_index);
        if let Some(m) = m.filter(|m| m.index == i) {
            let pat = &ac.patterns[m.pattern];
            if pat.length == 1 {
                // `match.length === 1 && priority <= Normal`
                list.push(single(i));
                i += 1;
                match_index += 1;
                continue;
            }
            let pinyins: Vec<&str> = pat.pinyin.split(' ').collect();
            for j in 0..pat.length {
                // `pinyins[pinyinIndex] || ""`
                list.push((s(i + j), pinyins.get(j).copied().unwrap_or("").to_string(), true));
            }
            i += pat.length;
            match_index += 1;
        } else {
            list.push(single(i));
            i += 1;
        }
    }
    list
}

// core/pinyin/index.mjs `pinyin(word, { type: "all", toneType: "none" })`
pub fn pinyin_all(word: &str) -> Vec<PinyinChar> {
    if word.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = word.chars().collect();
    // middlewareToneSandhi (yi/bu on, thirdTone off), middleWareNonZh ("spaced") and
    // middlewareMultiple change nothing with these options; middlewareToneType "none"
    // strips the tone from Chinese results.
    get_pinyin(&chars)
        .into_iter()
        .map(|(origin, result, is_zh)| {
            let result = if is_zh { get_pinyin_without_tone(&result) } else { result };
            // middlewareType "all": `pinyin = item.isZh ? item.result : ""`
            PinyinChar { origin, pinyin: if is_zh { result } else { String::new() }, is_zh }
        })
        .collect()
}
