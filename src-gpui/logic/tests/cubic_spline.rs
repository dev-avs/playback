
mod common;

use playback_logic::cubic_spline::NaturalSpline;
use serde_json::Value;

/// Golden numbers are strings (String(n) in JS) so they round-trip exactly.
fn num(v: &Value) -> f64 {
    v.as_str().unwrap().parse::<f64>().unwrap()
}

fn same(a: f64, e: f64) -> bool {
    (a.is_nan() && e.is_nan()) || a.to_bits() == e.to_bits()
}

#[test]
fn natural_spline_matches_golden() {
    let golden = common::golden("cubic_spline");
    let mut checked = 0;
    for case in golden.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let xs: Vec<f64> = case["xs"].as_array().unwrap().iter().map(num).collect();
        let ys: Vec<f64> = case["ys"].as_array().unwrap().iter().map(num).collect();
        let sp = NaturalSpline::new(xs, ys);

        let ks: Vec<f64> = case["ks"].as_array().unwrap().iter().map(num).collect();
        assert_eq!(sp.ks.len(), ks.len(), "{name}: ks length");
        for (i, (a, e)) in sp.ks.iter().zip(&ks).enumerate() {
            assert!(same(*a, *e), "{name}: ks[{i}] = {a:?}, expected {e:?}");
        }

        for pair in case["at"].as_array().unwrap() {
            let x = num(&pair[0]);
            let e = num(&pair[1]);
            let a = sp.at(x);
            assert!(same(a, e), "{name}: at({x:?}) = {a:?}, expected {e:?}");
            checked += 1;
        }
    }
    assert!(checked > 1000);
}
