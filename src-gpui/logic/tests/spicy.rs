//! src/lyrics/spicy.ts against golden/spicy.json (`node scripts/golden/run.mts spicy`).

mod common;

use playback_logic::objpack::SLObjPack;
use playback_logic::spicy::{load_spicy_lyrics, SpicyFetch, SpicyRequest, SpicySource};
use serde_json::{json, Value};

struct Fake {
    has_fetch: bool,
    res: Option<SpicyFetch>,
}

impl SpicySource for Fake {
    fn has_fetch_spicy_lyrics(&self) -> bool {
        self.has_fetch
    }
    fn fetch_spicy_lyrics(&self, _req: &SpicyRequest) -> Option<SpicyFetch> {
        self.res.clone()
    }
}

fn ok(lyrics: Option<Value>, packed: Option<Value>) -> Option<SpicyFetch> {
    Some(SpicyFetch { status: "ok".to_string(), packed, lyrics })
}

fn request() -> SpicyRequest {
    SpicyRequest { artist: "a".to_string(), title: "t".to_string(), ..Default::default() }
}

fn run(res: Option<SpicyFetch>) -> Value {
    let out = load_spicy_lyrics(&Fake { has_fetch: true, res }, &request());
    serde_json::to_value(out).unwrap()
}

fn payload(file: &str) -> Value {
    serde_json::from_str(&common::fixture(file)).unwrap()
}

#[test]
fn real_and_edge_payloads() {
    let golden = common::golden("spicy");
    let entries = golden["payloads"].as_array().unwrap();
    assert!(entries.len() >= 16, "{} payloads", entries.len());
    for entry in entries {
        let file = entry["file"].as_str().unwrap();
        let data = payload(file);
        // from the Spicetify bridge cache, already unpacked
        let actual = run(ok(Some(data.clone()), None));
        common::assert_same(&actual, &entry["lyrics"]);
        // from the API, packed (packed here with the ported pack, unpacked with the ported unpack)
        let packed = SLObjPack::default().pack(&data).unwrap();
        let actual = run(ok(None, Some(packed)));
        common::assert_same(&actual, &entry["packed"]);
    }
    eprintln!("{} payloads", entries.len());
}

#[test]
fn bridge_answers() {
    let golden = common::golden("spicy");
    let first = payload(golden["payloads"][0]["file"].as_str().unwrap());
    let status = |s: &str, lyrics: Option<Value>| Some(SpicyFetch { status: s.to_string(), packed: None, lyrics });
    let cases: Vec<(&str, Option<SpicyFetch>)> = vec![
        ("null", None),
        ("no-auth", status("no-auth", None)),
        ("error", status("error", Some(first.clone()))),
        ("ok-empty", ok(None, None)),
        ("lyrics-wins-over-packed", ok(Some(first.clone()), Some(json!("junk")))),
        ("lyrics-null-packed", ok(None, Some(SLObjPack::default().pack(&first).unwrap()))),
        ("packed-garbage", ok(None, Some(json!("junk")))),
        ("packed-bad-stream", ok(None, Some(json!([["a"], [9]])))),
        ("lyrics-empty-object", ok(Some(json!({})), None)),
        ("lyrics-empty-content", ok(Some(json!({"Type": "Syllable", "Content": []})), None)),
        ("lyrics-content-not-array", ok(Some(json!({"Type": "Line", "Content": "x"})), None)),
        ("lyrics-zero", ok(Some(json!(0)), None)),
        ("lyrics-all-blank", ok(Some(json!({"Type": "Line", "Content": [{"Text": " "}, {}]})), None)),
    ];
    let expected = golden["sources"].as_array().unwrap();
    assert_eq!(cases.len(), expected.len());
    for (name, res) in cases {
        let want = expected.iter().find(|e| e["name"] == name).unwrap_or_else(|| panic!("no golden case {name}"));
        let actual = run(res);
        common::assert_same(&actual, &want["result"]);
    }
}

#[test]
fn bridge_missing() {
    let golden = common::golden("spicy");
    let none = Fake { has_fetch: false, res: ok(Some(json!({"Type": "Line", "Content": [{"Text": "x"}]})), None) };
    let actual = serde_json::to_value(load_spicy_lyrics(&none, &request())).unwrap();
    common::assert_same(&actual, &golden["noBridge"]["noWindow"]);
    common::assert_same(&actual, &golden["noBridge"]["noFn"]);
}
