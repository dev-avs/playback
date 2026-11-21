//! src/lineState.ts (the line-state rules of src/App.tsx) against golden/line_state.json
//! (`node scripts/golden/run.mts line_state`).

mod common;

use common::{assert_same, golden};
use playback_logic::line_state::*;
use playback_logic::lyrics::LyricLine;
use serde_json::{json, Value};
use std::collections::HashSet;

fn lines_of(v: &Value) -> Vec<LyricLine> {
    serde_json::from_value(v.clone()).unwrap()
}

/// A number as the golden writes it: a JSON number, or "NaN" / "Infinity" / "-Infinity" / "-0".
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            other => other.parse().unwrap(),
        },
        // JSON has no NaN/Infinity: serde writes them as null
        Value::Null => f64::NAN,
        _ => panic!("not a number: {v}"),
    }
}

fn s_num(s: &str) -> f64 {
    num(&json!(s))
}

fn same_f64(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

fn check(what: &str, name: &str, actual: &Value, expected: &Value) {
    let r = std::panic::catch_unwind(|| assert_same(actual, expected));
    if let Err(e) = r {
        let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
        panic!("{what} [{name}]: {msg}");
    }
}

/// golden lines stored as an index into `base` (when equal to that line) or the line itself
fn decode(v: &Value, base: &[LyricLine]) -> Value {
    let out: Vec<LyricLine> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|e| match e.as_u64() {
            Some(k) => base[k as usize].clone(),
            None => serde_json::from_value(e.clone()).unwrap(),
        })
        .collect();
    serde_json::to_value(out).unwrap()
}

fn ids(s: &str) -> Vec<usize> {
    if s.is_empty() {
        vec![]
    } else {
        s.split(',').map(|x| x.parse().unwrap()).collect()
    }
}

#[test]
fn consts_match() {
    let g = golden("line_state");
    let c = &g["consts"];
    assert_eq!(NEXT_POPUP_SECS, num(&c["NEXT_POPUP_SECS"]));
    assert_eq!(MIN_GAP, num(&c["MIN_GAP"]));
    assert_eq!(PRE_HIDDEN_DOT_MS, num(&c["PRE_HIDDEN_DOT_MS"]));
    assert_eq!(INTERLUDE_PADDING, num(&c["INTERLUDE_PADDING"]));
    assert_eq!(LYRIC_LEAD, num(&c["LYRIC_LEAD"]));
    assert_eq!(BLUR_STEP, num(&c["BLUR_STEP"]));
    assert_eq!(BLUR_MAX, num(&c["BLUR_MAX"]));
}

#[test]
fn pipeline_matches() {
    let g = golden("line_state");
    let sets = g["pipeline"].as_array().unwrap();
    assert!(sets.len() > 40);
    let mut samples = 0;
    for set in sets {
        let name = set["name"].as_str().unwrap();
        let input = lines_of(&set["in"]);
        let unsynced = finish_lines(&input, Some(false), false);
        assert_eq!(unsynced == input, set["unsyncedSame"].as_bool().unwrap(), "{name}: synced=false");
        // synced undefined counts as synced
        assert_eq!(finish_lines(&input, None, true), finish_lines(&input, Some(true), true), "{name}");

        for (key, estimated) in [("false", false), ("true", true)] {
            let v = &set["variants"][key];
            let what = format!("{name} estimated={estimated}");

            // sealEnds
            let sealed = seal_ends(&input, estimated);
            let ends: Vec<f64> = v["sealedEnds"].as_array().unwrap().iter().map(|e| s_num(e.as_str().unwrap())).collect();
            assert_eq!(sealed.len(), ends.len(), "{what}: sealEnds length");
            for (i, (l, e)) in sealed.iter().zip(&ends).enumerate() {
                assert!(same_f64(l.end.unwrap(), *e), "{what}: sealEnds[{i}]");
                // everything but `end` is the input line
                let mut back = l.clone();
                back.end = input[i].end;
                assert_eq!(back, input[i], "{what}: sealEnds[{i}] changed more than end");
            }

            // withGaps
            check("withGaps", &what, &serde_json::to_value(with_gaps(&input, estimated)).unwrap(), &decode(&v["gapped"], &input));

            // finishLines
            let finished = finish_lines(&input, Some(true), estimated);
            check("finishLines", &what, &serde_json::to_value(&finished).unwrap(), &decode(&v["finished"], &sealed));

            // lineEndOf
            for (i, e) in v["ends"].as_array().unwrap().iter().enumerate() {
                let mut it = e.as_str().unwrap().split(' ');
                let (a, b) = (s_num(it.next().unwrap()), s_num(it.next().unwrap()));
                let x = line_end_of(&input[i], &input[i + 1], estimated);
                let y = line_end_of(&input[i], &input[i + 1], !estimated);
                assert!(same_f64(x, a) && same_f64(y, b), "{what}: lineEndOf[{i}]");
            }

            // resolveActive + isGapHiding on the finished lines
            for e in v["resolved"].as_array().unwrap() {
                let p: Vec<&str> = e.as_str().unwrap().split(' ').collect();
                let t = s_num(p[0]);
                let r = resolve_active(&finished, t);
                assert_eq!(r.active, ids(p[1]), "{what}: resolveActive({}) active", p[0]);
                assert_eq!(r.anchor, p[2].parse::<isize>().unwrap(), "{what}: resolveActive({}) anchor", p[0]);
                let hiding = is_gap_hiding(usize::try_from(r.anchor).ok().map(|a| &finished[a]), t);
                assert_eq!(hiding, p[3] == "1", "{what}: isGapHiding({})", p[0]);
                samples += 1;
            }
            // resolveActive on the lines that have ends but no gaps
            for e in v["resolvedSealed"].as_array().unwrap() {
                let p: Vec<&str> = e.as_str().unwrap().split(' ').collect();
                let r = resolve_active(&sealed, s_num(p[0]));
                assert_eq!(r.active, ids(p[1]), "{what}: sealed resolveActive({}) active", p[0]);
                assert_eq!(r.anchor, p[2].parse::<isize>().unwrap(), "{what}: sealed resolveActive({}) anchor", p[0]);
                samples += 1;
            }

            // firstLyricIdx, leadOf
            assert_eq!(first_lyric_index(&finished), v["firstIdx"].as_i64().unwrap() as isize, "{what}: firstLyricIndex");
            let lead_of = lead_indexes(&finished);
            let joined: Vec<String> = lead_of.iter().map(|x| x.to_string()).collect();
            assert_eq!(joined.join(","), v["leadOf"].as_str().unwrap(), "{what}: leadIndexes");

            // lineFlags + lineBlur
            let first = first_lyric_index(&finished);
            for f in v["flags"].as_array().unwrap() {
                let t = num(&f["t"]);
                let r = resolve_active(&finished, t);
                assert_eq!(r.anchor as i64, f["anchor"].as_i64().unwrap(), "{what}: flags anchor");
                let set: HashSet<usize> = r.active.iter().copied().collect();
                let bits = |fl: LineFlags| format!("{}{}{}", fl.is_current as u8, fl.is_past as u8, fl.is_intro_preview as u8);
                for (i, row) in f["rows"].as_array().unwrap().iter().enumerate() {
                    let fl = line_flags(&finished, i, true, &set, t, first);
                    let blur = line_blur(true, true, r.anchor, fl.is_intro_preview, &lead_of, i);
                    let (wb, wblur) = row.as_str().unwrap().split_once(' ').unwrap();
                    assert_eq!((bits(fl), blur), (wb.to_string(), s_num(wblur)), "{what}: row {i} at {t}");
                }
                for (i, row) in f["off"].as_array().unwrap().iter().enumerate() {
                    let fl = line_flags(&finished, i, false, &set, t, first);
                    let a = line_blur(false, true, r.anchor, fl.is_intro_preview, &lead_of, i);
                    let b = line_blur(true, false, r.anchor, false, &lead_of, i);
                    let p: Vec<&str> = row.as_str().unwrap().split(' ').collect();
                    assert_eq!((bits(fl), a, b), (p[0].to_string(), s_num(p[1]), s_num(p[2])), "{what}: off row {i} at {t}");
                }
            }
        }
    }
    println!("resolveActive samples: {samples}");
    assert!(samples > 3000, "{samples}");
}

#[test]
fn make_gap_matches() {
    let g = golden("line_state");
    let cs = g["makeGap"].as_array().unwrap();
    assert!(cs.len() > 100);
    for c in cs {
        let (a, b) = (num(&c["in"][0]), num(&c["in"][1]));
        check("makeGap", &c["in"].to_string(), &serde_json::to_value(make_gap(a, b)).unwrap(), &c["out"]);
    }
}

#[test]
fn is_gap_hiding_matches() {
    let g = golden("line_state");
    let cs = g["isGapHiding"].as_array().unwrap();
    assert!(cs.len() > 80);
    for c in cs {
        let line: Option<LyricLine> = if c["line"].is_null() { None } else { Some(serde_json::from_value(c["line"].clone()).unwrap()) };
        let t = num(&c["t"]);
        assert_eq!(is_gap_hiding(line.as_ref(), t), c["out"].as_bool().unwrap(), "{} at {}", c["line"], c["t"]);
    }
}

#[test]
fn fmt_matches() {
    let g = golden("line_state");
    let cs = g["fmt"].as_array().unwrap();
    assert!(cs.len() > 100);
    for c in cs {
        let args = c["in"].as_array().unwrap();
        let remaining = args.get(1).is_some_and(|r| r.as_bool().unwrap());
        assert_eq!(fmt(num(&args[0]), remaining), c["out"].as_str().unwrap(), "fmt({})", c["in"]);
    }
}

#[test]
fn split_for_wave_matches() {
    let g = golden("line_state");
    let cs = g["splitForWave"].as_array().unwrap();
    assert!(cs.len() > 100);
    let mut bad = Vec::new();
    for c in cs {
        let text = c["in"].as_str().unwrap();
        let actual = match split_for_wave(text) {
            Some(v) => json!(v),
            None => Value::Null,
        };
        if actual != c["out"] {
            let cps: Vec<String> = text.chars().map(|c| format!("{:04X}", c as u32)).collect();
            bad.push(format!("{text:?} U+{}: rust {actual} / js {}", cps.join(" U+"), c["out"]));
        }
    }
    assert!(bad.is_empty(), "grapheme differences:\n{}", bad.join("\n"));
}

#[test]
fn clock_helpers_match() {
    let g = golden("line_state");
    for c in g["lyricTimeOf"].as_array().unwrap() {
        let (p, o) = (num(&c["in"][0]), num(&c["in"][1]));
        let (a, e) = (lyric_time_of(p, o), num(&c["out"]));
        assert!(same_f64(a, e), "lyricTimeOf({p}, {o}): {a} != {e}");
    }
    let cs = g["isNearEnd"].as_array().unwrap();
    assert!(cs.len() > 100);
    for c in cs {
        let (d, n) = (num(&c["in"][0]), num(&c["in"][1]));
        assert_eq!(is_near_end(d, n), c["out"].as_bool().unwrap(), "isNearEnd({d}, {n})");
    }
}
