//! Golden tests for src/romanize.rs against golden/romanize.json (scripts/golden/cases/romanize.mts).
//! The golden carries its own inputs (lines, tokens, texts), so the Rust port is fed exactly what the
//! TypeScript saw; every output is then compared with `assert_same`.


mod common;

use playback_logic::lyrics::LyricLine;
use playback_logic::romanize::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

fn from<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{e}: {v}"))
}

fn to<T: serde::Serialize + ?Sized>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

fn lines(v: &Value) -> Vec<LyricLine> {
    from(v)
}

fn rows(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap()
}

#[test]
fn romanize_matches_golden() {
    let g = common::golden("romanize");

    // texts
    let texts: Vec<Value> = rows(&g["texts"])
        .iter()
        .map(|r| {
            let s = r["in"].as_str().unwrap();
            json!({
                "in": s,
                "hasHan": has_han_script(s),
                "nonLatin": has_non_latin_script(s),
                "k2h": katakana_to_hiragana(s),
                "ja": romanize_japanese(s),
                "ko": romanize_korean(s),
                "kanaHangul": romanize_kana_hangul(s),
                "text": romanize_text(s),
            })
        })
        .collect();

    // line sets
    let sets: Vec<Value> = rows(&g["sets"])
        .iter()
        .map(|r| {
            let ls = lines(&r["lines"]);
            let roman = romanize_lines(&ls);
            let pin = apply_pinyin(&ls);
            let mut readings = Map::new();
            for (variant, v) in r["readings"].as_object().unwrap() {
                let tokens: Vec<Vec<ReadingToken>> = from(&v["tokens"]);
                let out = apply_readings(&ls, &tokens);
                let rom = romanize_lines(&out);
                readings.insert(variant.clone(), json!({"tokens": v["tokens"], "out": to(&out), "roman": to(&rom)}));
            }
            json!({
                "name": r["name"],
                "lines": r["lines"],
                "need": lines_need_romanization(&ls),
                "lang": detect_lyric_lang(&ls),
                "needReadings": lines_need_readings(&ls),
                "readingTexts": to(&reading_texts(&ls)),
                "lineChars": to(&ls.iter().map(line_chars).collect::<Vec<_>>()),
                "romanizeLines": to(&roman),
                "applyPinyin": to(&pin),
                "pinyinRoman": to(&romanize_lines(&pin)),
                "readings": readings,
                "withRoma": to(&with_roma_lines(&ls, &roman, 0.6)),
            })
        })
        .collect();

    // alignRomanizedToWords
    let align: Vec<Value> = rows(&g["align"])
        .iter()
        .map(|r| {
            let tol = r.get("tol").map_or(0.6, |t| t.as_f64().unwrap());
            let mut o = Map::new();
            for k in ["name", "lines", "roma"] {
                o.insert(k.into(), r[k].clone());
            }
            if let Some(t) = r.get("tol") {
                o.insert("tol".into(), t.clone());
            }
            o.insert("out".into(), to(&align_romanized_to_words(&lines(&r["lines"]), &lines(&r["roma"]), tol)));
            Value::Object(o)
        })
        .collect();

    // withRomaLines
    let with_roma: Vec<Value> = rows(&g["withRoma"])
        .iter()
        .map(|r| {
            let tol = r.get("tol").map_or(0.6, |t| t.as_f64().unwrap());
            let mut o = Map::new();
            for k in ["name", "lines", "roman"] {
                o.insert(k.into(), r[k].clone());
            }
            if let Some(t) = r.get("tol") {
                o.insert("tol".into(), t.clone());
            }
            o.insert("out".into(), to(&with_roma_lines(&lines(&r["lines"]), &lines(&r["roman"]), tol)));
            Value::Object(o)
        })
        .collect();

    // helpers
    let h = &g["helpers"];
    let tokens: Vec<Value> = rows(&h["tokens"])
        .iter()
        .map(|r| {
            let t: ReadingToken = from(&r["tok"]);
            json!({"tok": r["tok"], "attaches": attaches(&t), "tokenChars": to(&token_chars(&t))})
        })
        .collect();
    let is_space_rows: Vec<Value> = rows(&h["isSpace"]).iter().map(|r| json!({"in": r["in"], "out": is_space(r["in"].as_str().unwrap())})).collect();
    let morae_rows: Vec<Value> = rows(&h["morae"]).iter().map(|r| json!({"in": r["in"], "out": to(&morae(r["in"].as_str().unwrap()))})).collect();
    let spread_rows: Vec<Value> = rows(&h["spread"])
        .iter()
        .map(|r| json!({"reading": r["reading"], "n": r["n"], "out": to(&spread(r["reading"].as_str().unwrap(), r["n"].as_u64().unwrap() as usize))}))
        .collect();
    let join_rows: Vec<Value> = rows(&h["joinChars"])
        .iter()
        .map(|r| {
            let chars: Vec<CharText> = from(&r["chars"]);
            json!({"name": r["name"], "chars": r["chars"], "out": join_chars(&chars)})
        })
        .collect();
    let with_chars_rows: Vec<Value> = rows(&h["withChars"])
        .iter()
        .map(|r| {
            let line: LyricLine = from(&r["line"]);
            let chars: Vec<CharText> = from(&r["chars"]);
            json!({"name": r["name"], "line": r["line"], "chars": r["chars"], "out": to(&with_chars(&line, &chars))})
        })
        .collect();

    let actual = json!({
        "texts": texts,
        "sets": sets,
        "align": align,
        "withRoma": with_roma,
        "helpers": {
            "tokens": tokens,
            "isSpace": is_space_rows,
            "morae": morae_rows,
            "spread": spread_rows,
            "joinChars": join_rows,
            "withChars": with_chars_rows,
        },
    });
    common::assert_same(&actual, &g);
}
