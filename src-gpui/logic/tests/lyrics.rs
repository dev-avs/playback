//! Golden tests for src/lyrics.rs against golden/lyrics.json (scripts/golden/cases/lyrics.mts).

mod common;

use common::{assert_same, golden};
use playback_logic::lyrics::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

fn cases(g: &Value, key: &str) -> Vec<Value> {
    g[key].as_array().unwrap_or_else(|| panic!("no golden {key}")).clone()
}

fn from<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}

fn check(key: &str, name: &Value, actual: Value, expected: &Value) {
    let r = std::panic::catch_unwind(|| assert_same(&actual, expected));
    if let Err(e) = r {
        let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
        panic!("{key} [{name}]: {msg}");
    }
}

fn lines_of(v: &Value) -> Vec<LyricLine> {
    from(v)
}

#[test]
fn parse_mm_ss_matches() {
    let g = golden("lyrics");
    let cs = cases(&g, "parseMmSs");
    assert!(!cs.is_empty());
    for c in &cs {
        let (a, b) = (c["in"][0].as_str().unwrap(), c["in"][1].as_str().unwrap());
        check("parseMmSs", &c["in"], json!(parse_mm_ss(a, b)), &c["out"]);
    }
}

#[test]
fn is_credit_line_matches() {
    let g = golden("lyrics");
    let cs = cases(&g, "isCreditLine");
    assert!(cs.len() > 100);
    for c in &cs {
        check("isCreditLine", &c["in"], json!(is_credit_line(c["in"].as_str().unwrap())), &c["out"]);
    }
}

#[test]
fn self_delimiting_matches() {
    let g = golden("lyrics");
    for c in cases(&g, "selfDelimiting") {
        let ch = c["in"].as_str().unwrap().chars().next_back().unwrap();
        // the TypeScript tests the last UTF-16 unit, which never matches for an astral character
        let actual = (ch as u32) <= 0xffff && self_delimiting(ch);
        check("selfDelimiting", &c["in"], json!(actual), &c["out"]);
    }
}

#[test]
fn parse_lrc_matches() {
    let g = golden("lyrics");
    let cs = cases(&g, "parseLrc");
    assert!(cs.len() > 50);
    for c in &cs {
        check("parseLrc", &c["name"], serde_json::to_value(parse_lrc(c["in"].as_str().unwrap())).unwrap(), &c["out"]);
    }
}

#[test]
fn parse_yrc_matches() {
    let g = golden("lyrics");
    for c in &cases(&g, "parseYrc") {
        check("parseYrc", &c["name"], serde_json::to_value(parse_yrc(c["in"].as_str().unwrap())).unwrap(), &c["out"]);
    }
}

#[test]
fn extract_credits_matches() {
    let g = golden("lyrics");
    let cs = cases(&g, "extractCredits");
    assert!(cs.len() > 100);
    for c in &cs {
        let out = extract_credits(&lines_of(&c["in"]));
        check("extractCredits", &c["name"], serde_json::to_value(out).unwrap(), &c["out"]);
    }
}

#[test]
fn mark_parenthetical_backing_vocals_matches() {
    let g = golden("lyrics");
    for c in &cases(&g, "markParentheticalBackingVocals") {
        let out = mark_parenthetical_backing_vocals(&lines_of(&c["in"]));
        check("markParentheticalBackingVocals", &c["name"], serde_json::to_value(out).unwrap(), &c["out"]);
    }
}

#[test]
fn split_trailing_aside_matches() {
    let g = golden("lyrics");
    for c in &cases(&g, "splitTrailingAside") {
        let line: LyricLine = from(&c["in"]);
        let actual = match split_trailing_aside(&line) {
            Some((lead, bg)) => json!({ "lead": lead, "bg": bg }),
            None => Value::Null,
        };
        check("splitTrailingAside", &c["in"]["text"], actual, &c["out"]);
    }
}

#[test]
fn aside_word_index_and_strip_brackets_match() {
    let g = golden("lyrics");
    for c in &cases(&g, "asideWordIndex") {
        let words: Vec<LyricWord> = from(&c["in"]);
        check("asideWordIndex", &c["name"], json!(aside_word_index(&words)), &c["out"]);
    }
    for c in &cases(&g, "stripBrackets") {
        let words: Vec<LyricWord> = from(&c["in"]);
        check("stripBrackets", &c["name"], serde_json::to_value(strip_brackets(&words)).unwrap(), &c["out"]);
    }
}

#[test]
fn chars_in_matches() {
    let g = golden("lyrics");
    for c in &cases(&g, "charsIn") {
        check("charsIn", &c["in"], json!(chars_in(c["in"].as_str().unwrap())), &c["out"]);
    }
}

#[test]
fn estimate_rate_and_word_timings_match() {
    let g = golden("lyrics");
    let cs = cases(&g, "estimateWordTimings");
    assert!(cs.len() > 100);
    for c in &cases(&g, "estimateRate") {
        check("estimateRate", &c["name"], json!(estimate_rate(&lines_of(&c["in"]))), &c["out"]);
    }
    for c in &cs {
        let out = estimate_word_timings(&lines_of(&c["in"]));
        check("estimateWordTimings", &c["name"], serde_json::to_value(out).unwrap(), &c["out"]);
    }
}

/// `window.player.fetchLyrics` answering with the response recorded in the golden file.
struct Recorded(Option<LyricsResponse>);

impl LyricsSource for Recorded {
    fn fetch_lyrics(&self, _req: &LyricsRequest) -> Option<LyricsResponse> {
        self.0.clone()
    }
}

#[test]
fn load_lyrics_matches() {
    let g = golden("lyrics");
    let cs = cases(&g, "loadLyrics");
    assert!(cs.len() > 30);
    let req = LyricsRequest { artist: "A".into(), title: "T".into(), ..Default::default() };
    for c in &cs {
        let src = Recorded(if c["res"].is_null() { None } else { Some(from(&c["res"])) });
        let bridge: Option<&dyn LyricsSource> = if c["bridge"].as_bool().unwrap() { Some(&src) } else { None };
        let actual = match load_lyrics(bridge, &req) {
            Some(l) => serde_json::to_value(l).unwrap(),
            None => Value::Null,
        };
        check("loadLyrics", &c["name"], actual, &c["out"]);
    }
}

#[test]
fn fixtures_load_through_the_ladder() {
    // the recorded NetEase bohemian response takes the syllable tier
    let g = golden("lyrics");
    let c = cases(&g, "loadLyrics").into_iter().find(|c| c["name"] == "netease/bohemian.json yrc+lrc").unwrap();
    assert_eq!(c["out"]["tier"], 3);
    assert_eq!(c["out"]["source"], "netease/yrc");
}
