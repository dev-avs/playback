#![allow(dead_code)] // each test binary uses only some helpers

//! Shared by the golden tests: loads `golden/<module>.json` (written by
//! `node scripts/golden/run.mts`).

use serde_json::Value;
use std::path::PathBuf;

pub fn golden(module: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden").join(format!("{module}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (run `node scripts/golden/run.mts`)", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// The fixture file under scripts/golden/fixtures, as text.
pub fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/golden/fixtures").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Asserts two JSON values are the same, with numbers compared as f64 (so
/// JavaScript's `12` equals Rust's `12.0`) and exactly. Panics with the JSON
/// path of the first difference.
pub fn assert_same(actual: &Value, expected: &Value) {
    if let Err(at) = same(actual, expected, String::from("$")) {
        panic!("golden mismatch at {at}");
    }
}

fn same(a: &Value, e: &Value, at: String) -> Result<(), String> {
    match (a, e) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if x == y { Ok(()) } else { Err(format!("{at}: {x} != expected {y}")) }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{at}: length {} != expected {}", x.len(), y.len()));
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same(p, q, format!("{at}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(x), Value::Object(y)) => {
            for k in y.keys() {
                if !x.contains_key(k) {
                    return Err(format!("{at}.{k}: missing"));
                }
            }
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => same(v, w, format!("{at}.{k}"))?,
                    None => return Err(format!("{at}.{k}: unexpected ({v})")),
                }
            }
            Ok(())
        }
        _ if a == e => Ok(()),
        _ => Err(format!("{at}: {a} != expected {e}")),
    }
}
