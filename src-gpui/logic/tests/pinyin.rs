
mod common;

use playback_logic::pinyin::pinyin_all;
use serde_json::{json, Value};

/// The golden's inputs are the corpus (scripts/golden/cases/pinyin.mts); each is run through the port.
#[test]
fn pinyin_matches_golden() {
    let golden = common::golden("pinyin");
    let cases = golden.as_array().unwrap();
    assert!(cases.len() > 10000, "{} cases", cases.len());
    let actual: Vec<Value> = cases
        .iter()
        .map(|c| {
            let input = c["in"].as_str().unwrap();
            let out: Vec<String> = pinyin_all(input)
                .iter()
                .map(|p| format!("{}\t{}\t{}", p.origin, p.pinyin, u8::from(p.is_zh)))
                .collect();
            json!({"in": input, "out": out})
        })
        .collect();
    common::assert_same(&Value::Array(actual), &golden);
}
