//! src/vendor/objpack.ts against golden/objpack.json (`node scripts/golden/run.mts objpack`).

mod common;

use playback_logic::objpack::{ObjPackError, SLObjPack, SLObjPackOptions};
use serde_json::Value;

/// Compares object key order too (`assert_same` ignores it, and the key order
/// JavaScript gives is part of what pack emits and unpack returns).
fn same_key_order(a: &Value, e: &Value, at: &str) {
    match (a, e) {
        (Value::Object(x), Value::Object(y)) => {
            let kx: Vec<&String> = x.keys().collect();
            let ky: Vec<&String> = y.keys().collect();
            assert_eq!(kx, ky, "key order at {at}");
            for (k, v) in x {
                same_key_order(v, &y[k], &format!("{at}.{k}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same_key_order(p, q, &format!("{at}[{i}]"));
            }
        }
        _ => {}
    }
}

fn check(actual: Result<Value, ObjPackError>, entry: &Value, what: &str) {
    match (actual, entry.get("ok"), entry.get("error")) {
        (Ok(a), Some(e), None) => {
            common::assert_same(&a, e);
            same_key_order(&a, e, "$");
        }
        (Err(a), None, Some(e)) => assert_eq!(&a.0, e.as_str().unwrap(), "{what}"),
        (a, e, err) => panic!("{what}: got {a:?}, expected ok {e:?} / error {err:?}"),
    }
}

fn pack_with(options: &Value) -> SLObjPack {
    if options.is_null() {
        return SLObjPack::default();
    }
    SLObjPack::new(serde_json::from_value::<SLObjPackOptions>(options.clone()).unwrap())
}

#[test]
fn pack_values() {
    let golden = common::golden("objpack");
    let packer = SLObjPack::default();
    let entries = golden["pack"].as_array().unwrap();
    for e in entries {
        check(packer.pack(&e["in"]), e, e["name"].as_str().unwrap());
    }
    eprintln!("{} pack cases", entries.len());
}

#[test]
fn round_trip() {
    let golden = common::golden("objpack");
    let packer = SLObjPack::default();
    let entries = golden["roundTrip"].as_array().unwrap();
    for e in entries {
        check(packer.unpack(&e["packed"]), e, e["name"].as_str().unwrap());
    }
    // and the ported pack's output unpacks to the input
    for p in golden["pack"].as_array().unwrap() {
        if p.get("ok").is_some() {
            let input = &p["in"];
            let back = packer.unpack(&packer.pack(input).unwrap()).unwrap();
            common::assert_same(&back, input);
        }
    }
    eprintln!("{} round trips", entries.len());
}

#[test]
fn real_payloads() {
    let golden = common::golden("objpack");
    let packer = SLObjPack::default();
    let entries = golden["real"].as_array().unwrap();
    for e in entries {
        let file = e["file"].as_str().unwrap();
        let data: Value = serde_json::from_str(&common::fixture(file)).unwrap();
        let packed = packer.pack(&data).unwrap();
        common::assert_same(&packed, &e["packed"]);
        let unpacked = packer.unpack(&e["packed"]).unwrap();
        common::assert_same(&unpacked, &e["unpacked"]);
        same_key_order(&unpacked, &e["unpacked"], file);
        common::assert_same(&unpacked, &data);
    }
    eprintln!("{} real payloads", entries.len());
}

#[test]
fn limits() {
    let golden = common::golden("objpack");
    let entries = golden["limits"].as_array().unwrap();
    for e in entries {
        let p = pack_with(&e["options"]);
        let name = e["name"].as_str().unwrap();
        if let Some(input) = e.get("in") {
            check(p.pack(input), e, name);
        } else {
            check(p.unpack(&e["packed"]), e, name);
        }
    }
    eprintln!("{} limit cases", entries.len());
}

#[test]
fn unpack_malformed() {
    let golden = common::golden("objpack");
    let entries = golden["unpack"].as_array().unwrap();
    for e in entries {
        let p = pack_with(&e["options"]);
        check(p.unpack(&e["in"]), e, e["name"].as_str().unwrap());
    }
    eprintln!("{} unpack cases", entries.len());
}
