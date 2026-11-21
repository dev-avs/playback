mod common;

use playback_logic::uroman::{has_uroman_script, prepare, uromanize, uromanize_parts, Lattice};
use serde_json::{json, Value};

fn lines() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for rel in ["edge/scripts.txt", "uroman/lines.txt"] {
        v.extend(common::fixture(rel).split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).filter(|l| !l.is_empty()));
    }
    v.push(String::new());
    v.push(" ".to_string());
    v
}

#[test]
fn uroman_matches_golden() {
    let golden = common::golden("uroman");
    let lines = lines();
    let mut parts: Vec<Vec<String>> = lines.iter().filter(|s| s.contains(' ')).map(|s| s.split(' ').map(String::from).collect()).collect();
    let extra: Vec<Vec<String>> = serde_json::from_str(&common::fixture("uroman/parts.json")).unwrap();
    parts.extend(extra);

    let actual = json!({
        "uromanize": lines.iter().map(|s| json!({"in": s, "has": has_uroman_script(s), "out": uromanize(s)})).collect::<Vec<_>>(),
        "prepare": lines.iter().map(|s| json!({"in": s, "out": prepare(s)})).collect::<Vec<_>>(),
        "edges": lines.iter().map(|s| json!({"in": s, "out": serde_json::to_value(Lattice::new(&prepare(s)).romanize()).unwrap()})).collect::<Vec<_>>(),
        "uromanizeParts": parts.iter().map(|p| json!({"in": p, "out": uromanize_parts(p)})).collect::<Vec<_>>(),
    });
    let _: &Value = &actual;
    common::assert_same(&actual, &golden);
}
