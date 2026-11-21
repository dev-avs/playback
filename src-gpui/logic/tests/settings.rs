//! settings.rs against golden/settings.json (scripts/golden/cases/settings.mts).

mod common;

use common::{assert_same, fixture, golden};
use playback_logic::settings::items::{Control, Fmt, CATEGORIES, ITEMS};
use playback_logic::settings::*;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn obj<'a>(pairs: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<Map<_, _>>())
}
fn opt(o: Option<&str>) -> Value {
    o.map_or(Value::Null, |s| json!(s))
}

fn entries_object(settings: &Settings) -> Value {
    obj(settings.entries())
}

#[test]
fn defaults_and_keys() {
    let g = golden("settings");
    assert_same(&entries_object(&Settings::default()), &g["defaults"]);
    // serde agrees with entries()
    assert_same(&serde_json::to_value(Settings::default()).unwrap(), &g["defaults"]);
    let order: Vec<&str> = Settings::default().entries().iter().map(|(k, _)| *k).collect();
    let want: Vec<&str> = g["defaultKeyOrder"].as_array().unwrap().iter().map(s).collect();
    assert_eq!(order, want);
    assert_eq!(want.len(), 83);

    let keys: Vec<(&str, &str)> = KEYS.to_vec();
    let want_keys: Vec<(String, String)> =
        g["keys"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), s(v).to_string())).collect();
    assert_eq!(keys.len(), want_keys.len());
    for (a, b) in keys.iter().zip(&want_keys) {
        assert_eq!((a.0, a.1), (b.0.as_str(), b.1.as_str()));
    }
    let key_order: Vec<&str> = g["keyOrder"].as_array().unwrap().iter().map(s).collect();
    assert_eq!(keys.iter().map(|k| k.0).collect::<Vec<_>>(), key_order);
}

#[test]
fn snapshot_text_matches_the_typescript_function() {
    let g = golden("settings");
    let scenarios = g["snapshots"].as_array().unwrap();
    for (i, sc) in scenarios.iter().enumerate() {
        let storage: HashMap<String, String> =
            sc["storage"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), s(v).to_string())).collect();
        let out = snapshot_settings_json(|k| storage.get(k).cloned());
        assert_eq!(out, s(&sc["out"]), "scenario {i}");
    }
}

#[test]
fn default_settings_write_the_same_text() {
    let g = golden("settings");
    let text = stringify_entries(&Settings::default().entries(), 2);
    assert_eq!(text, s(&g["snapshots"][0]["out"]));
}

#[test]
fn real_gpui_settings_round_trip() {
    let text = fixture("settings/gpui-settings.json");
    let settings: Settings = serde_json::from_str(&text).unwrap();
    assert_eq!(settings.glass_tint, "cool");
    assert_eq!(stringify_entries(&settings.entries(), 2), text);
    // the same as the Value it parsed from
    let want: Value = serde_json::from_str(&text).unwrap();
    assert_same(&entries_object(&settings), &want);
    // a missing key takes its default
    let partial: Settings = serde_json::from_str(r#"{"lyricSize": 20}"#).unwrap();
    assert_eq!(partial.lyric_size, 20.0);
    assert_eq!(partial.glass_tint, "dark");
    assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
}

#[test]
fn snapshot_outputs_load_back() {
    let g = golden("settings");
    for (i, sc) in g["snapshots"].as_array().unwrap().iter().enumerate() {
        let text = s(&sc["out"]);
        let settings: Settings = serde_json::from_str(text).unwrap_or_else(|e| panic!("scenario {i}: {e}"));
        let raw: Value = serde_json::from_str(text).unwrap();
        if raw["romanizeDefault"] == Value::Bool(true) {
            // stored as a boolean up to 1.5.2: loads as romaMode() reads it
            assert_eq!(settings.romanize_default, "latin", "scenario {i}");
        } else {
            assert_eq!(stringify_entries(&settings.entries(), 2), text, "scenario {i}");
        }
    }
}

#[test]
fn read_and_write() {
    let g = golden("settings");
    for r in g["reads"].as_array().unwrap() {
        let raw = r["raw"].as_str();
        assert_same(&read(raw, json!("INIT")), &r["out"]);
    }
    for w in g["writes"].as_array().unwrap() {
        let v: Value = serde_json::from_str(s(&w["in"])).unwrap();
        assert_eq!(write(&v), s(&w["out"]), "write {}", w["in"]);
    }
    for c in g["carry"].as_array().unwrap() {
        assert_eq!(carry_over_next_song(c["nextSong"].as_str(), c["showNext"].as_str()).as_deref(), c["out"].as_str(), "{c}");
    }
}

#[test]
fn helpers() {
    let h = &golden("settings")["helpers"];
    for c in h["fontStack"].as_array().unwrap() {
        assert_eq!(font_stack(s(&c["in"])).as_deref(), c["out"].as_str(), "{c}");
    }
    for c in h["romaMode"].as_array().unwrap() {
        assert_eq!(roma_mode(&c["in"]).as_str(), s(&c["out"]), "{c}");
    }
    for c in h["highlightVars"].as_array().unwrap() {
        assert_same(&obj(highlight_vars(s(&c["in"]))), &c["out"]);
    }
    for c in h["cjkStack"].as_array().unwrap() {
        let lang = match s(&c["lang"]) {
            "ja" => CjkLang::Ja,
            "ko" => CjkLang::Ko,
            _ => CjkLang::Zh,
        };
        assert_eq!(cjk_stack(lang, s(&c["font"])), s(&c["out"]), "{c}");
    }
    for c in h["tierDots"].as_array().unwrap() {
        let tier: f64 = s(&c["in"]).parse().unwrap();
        assert_eq!(tier_dots(tier).as_deref(), c["out"].as_str(), "{c}");
    }
    for c in h["songKey"].as_array().unwrap() {
        assert_eq!(song_key(s(&c["artist"]), s(&c["title"])), s(&c["out"]), "{c}");
    }
    for c in h["providerOf"].as_array().unwrap() {
        assert_eq!(provider_of(c["in"].as_str()), s(&c["out"]), "{c}");
    }
    assert_eq!(logo_art(), s(&golden("settings")["logoArt"]));
}

#[test]
fn tables() {
    let t = &golden("settings")["tables"];
    let pairs = |p: &[(&str, &str)]| obj(p.iter().map(|(k, v)| (*k, json!(v))));
    assert_same(&obj(LYRIC_FONTS.iter().map(|(k, v)| (*k, opt(*v)))), &t["LYRIC_FONTS"]);
    assert_same(&json!(ROMA_MODES.iter().map(|m| m.as_str()).collect::<Vec<_>>()), &t["ROMA_MODES"]);
    assert_same(&pairs(&ROMA_LABELS), &t["ROMA_LABELS"]);
    assert_same(
        &obj(HIGHLIGHTS.iter().map(|(k, c)| (*k, json!({"name": c.name, "h": c.h, "s": c.s, "l": c.l})))),
        &t["HIGHLIGHTS"],
    );
    assert_same(&pairs(&SWEEP_ANGLES), &t["SWEEP_ANGLES"]);
    assert_same(&pairs(&SWEEP_ANGLES_RTL), &t["SWEEP_ANGLES_RTL"]);
    assert_same(&obj(GLASS_TINTS.iter().map(|(k, n, rgb)| (*k, json!({"name": n, "rgb": rgb})))), &t["GLASS_TINTS"]);
    assert_same(&obj(LYRIC_WEIGHTS.iter().map(|(k, v)| (*k, json!(v)))), &t["LYRIC_WEIGHTS"]);
    assert_same(&pairs(&COVER_SHAPES), &t["COVER_SHAPES"]);
    assert_same(
        &obj(CJK_FONTS.iter().map(|(_, k, f)| (*k, json!({"name": f.name, "stack": f.stack, "options": f.options})))),
        &t["CJK_FONTS"],
    );
    assert_same(&obj(MOTIONS.iter().map(|(k, v)| (*k, json!(v)))), &t["MOTIONS"]);
    assert_same(&json!(PROVIDERS), &t["PROVIDERS"]);
    assert_same(&pairs(&PROVIDER_NAMES), &t["PROVIDER_NAMES"]);
}

fn control_json(c: &Control) -> Value {
    match c {
        Control::Toggle { preview } => json!({"kind": "toggle", "preview": preview}),
        Control::Slider { min, max, step, fmt, on_change } => json!({
            "kind": "slider", "min": min, "max": max, "step": step,
            "fmt": fmt.map(|f| f.name()), "onChange": on_change,
        }),
        Control::Chips { options, preview, peek, on_change, note } => json!({
            "kind": "chips",
            "options": options.iter().map(|(v, l)| json!({"value": v, "label": l})).collect::<Vec<_>>(),
            "preview": preview, "peek": peek, "onChange": on_change, "note": note,
        }),
        Control::Custom { note } => json!({"kind": "custom", "note": note}),
    }
}

#[test]
fn items_match_the_extracted_rows() {
    let g = golden("settings");
    let items = &g["items"];
    let cats: Vec<Value> = CATEGORIES.iter().map(|c| json!({"id": c.id, "name": c.name})).collect();
    assert_same(&json!(cats), &items["categories"]);
    let rows: Vec<Value> = ITEMS
        .iter()
        .map(|i| {
            json!({
                "id": i.id, "cat": i.cat, "key": i.key, "label": i.label, "desc": i.desc,
                "hint": i.hint, "hintExpr": i.hint_expr, "control": control_json(&i.control),
            })
        })
        .collect();
    assert_eq!(rows.len(), 90);
    assert_same(&json!(rows), &items["items"]);
    // every keyed row is a real setting, and every setting has a row
    let defaults = entries_object(&Settings::default());
    for i in ITEMS {
        if let Some(k) = i.key {
            assert!(defaults.get(k).is_some(), "{k}");
        }
    }
    // the formatters: same names and sources
    let fmts = items["fmts"].as_object().unwrap();
    assert_eq!(Fmt::ALL.len(), fmts.len());
    for f in Fmt::ALL {
        assert_eq!(f.source(), s(&fmts[f.name()]["ts"]));
    }
}

#[test]
fn slider_formats() {
    let g = golden("settings");
    let mut n = 0;
    for (name, list) in g["fmt"].as_object().unwrap() {
        let fmt = *Fmt::ALL.iter().find(|f| f.name() == name).unwrap();
        for c in list.as_array().unwrap() {
            let v = c["v"].as_f64().unwrap();
            assert_eq!(fmt_slider(fmt, v), s(&c["out"]), "{name}({v})");
            n += 1;
        }
    }
    assert!(n > 1000, "{n}");
}

#[test]
fn javascript_numbers() {
    let g = golden("settings");
    for c in g["jsNum"].as_array().unwrap() {
        let v = c["v"].as_f64().unwrap();
        assert_eq!(js_num(v), s(&c["str"]), "String({v:e})");
        assert_eq!(js_num(js_round(v)), js_num(c["round"].as_f64().unwrap()), "Math.round({v:e})");
        for (d, want) in c["fixed"].as_array().unwrap().iter().enumerate() {
            assert_eq!(to_fixed(v, d), s(want), "({v:e}).toFixed({d})");
        }
    }
}

#[test]
fn wrong_types_take_defaults() {
    let s: Settings = serde_json::from_str(r#"{"lyricSize": "big", "romanizeDefault": false, "fsSwap": 3}"#).unwrap();
    let d = Settings::default();
    assert_eq!(s.lyric_size, d.lyric_size);
    assert_eq!(s.fs_swap, d.fs_swap);
    assert_eq!(s.romanize_default, "off");
}
