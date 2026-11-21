
mod common;

use playback_logic::lyric_animator::*;
use serde_json::Value;

/// Golden numbers are strings (String(n) in JS) so they round-trip exactly.
fn num(v: &Value) -> f64 {
    v.as_str().unwrap().parse::<f64>().unwrap()
}

fn same(a: f64, e: f64) -> bool {
    (a.is_nan() && e.is_nan()) || a.to_bits() == e.to_bits()
}

fn style_of(name: &str) -> AnimationStyle {
    match name {
        "playback" => AnimationStyle::Playback,
        "spicy" => AnimationStyle::Spicy,
        _ => panic!("style {name}"),
    }
}

// ── the frame token format of scripts/golden/cases/lyric_animator.mts ──
enum Tok {
    N(f64),
    S(Option<String>),
}

fn opt(s: &Option<String>) -> Tok {
    Tok::S(s.clone())
}

fn spring_tokens(sp: &Spring, out: &mut Vec<Tok>) {
    out.push(Tok::N(sp.x));
    out.push(Tok::N(sp.v));
    out.push(Tok::N(sp.g));
}

fn unit_tokens(el: &El, springs: [&Spring; 3], opacity: Option<&Spring>, w: &Written, extra: bool, out: &mut Vec<Tok>) {
    for sp in springs {
        spring_tokens(sp, out);
    }
    if let Some(o) = opacity {
        spring_tokens(o, out);
    }
    for n in [w.s, w.y, w.gb, w.go, w.gp, w.o] {
        out.push(Tok::N(n));
    }
    for p in [&el.style.s, &el.style.y, &el.style.gb, &el.style.go, &el.style.gp, &el.style.o] {
        out.push(opt(p));
    }
    if extra {
        out.push(opt(&el.style.will_change));
        out.push(opt(&el.style.backface_visibility));
        out.push(Tok::S(if el.gpu { Some("1".to_string()) } else { None }));
    }
}

fn frame_tokens(words: &[WordUnit], extra: bool) -> Vec<Tok> {
    let mut out = Vec::new();
    for w in words {
        unit_tokens(&w.el, [&w.scale, &w.y, &w.glow], w.opacity.as_ref(), &w.w, extra, &mut out);
        for l in &w.letters {
            unit_tokens(&l.el, [&l.scale, &l.y, &l.glow], None, &l.w, extra, &mut out);
        }
    }
    out
}

/// Compares our tokens to the golden frame string; Err describes the first difference.
fn compare(actual: &[Tok], expected: &str) -> Result<(), String> {
    let exp: Vec<&str> = expected.split('|').collect();
    if exp.len() != actual.len() {
        return Err(format!("{} tokens, expected {}", actual.len(), exp.len()));
    }
    for (i, (a, e)) in actual.iter().zip(&exp).enumerate() {
        match a {
            Tok::N(x) => {
                let y: f64 = e.parse().unwrap_or_else(|_| panic!("token {i} {e:?} is not a number"));
                if !same(*x, y) {
                    return Err(format!("token {i}: {x:?} != expected {y:?}"));
                }
            }
            Tok::S(s) => {
                let got = s.as_deref().unwrap_or("-");
                if got != *e {
                    return Err(format!("token {i}: {got:?} != expected {e:?}"));
                }
            }
        }
    }
    Ok(())
}

fn line_input(v: &Value) -> LineInput {
    v.as_array()
        .unwrap()
        .iter()
        .map(|w| WordInput {
            dot: w["dot"].as_bool().unwrap(),
            word_group: w["word_group"].as_bool().unwrap(),
            s: num(&w["s"]),
            e: num(&w["e"]),
            sweep: w["sweep"].as_bool().unwrap(),
            letters: w["letters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| LetterInput { s: num(&l[0]), e: num(&l[1]) })
                .collect(),
        })
        .collect()
}

#[test]
fn step_line_matches_golden() {
    let golden = common::golden("lyric_animator");
    let mut frames_checked = 0;
    let mut letter_units = 0;
    for run in golden["runs"].as_array().unwrap() {
        let name = format!(
            "{} [{} motion {} dt {} {}]",
            run["name"].as_str().unwrap(),
            run["style"].as_str().unwrap(),
            run["motion"].as_str().unwrap(),
            run["dt"].as_str().unwrap_or("seq"),
            run["frames"],
        );
        set_animation_style(style_of(run["style"].as_str().unwrap()));
        set_lyric_motion(num(&run["motion"]));
        let line = line_input(&run["line"]);
        let mut words = collect_line(&line);
        letter_units += words.iter().map(|w| w.letters.len()).sum::<usize>();

        compare(&frame_tokens(&words, true), run["init"].as_str().unwrap()).unwrap_or_else(|e| panic!("{name}: init: {e}"));

        let dt_seq: Option<Vec<f64>> = run["dt_seq"].as_array().map(|a| a.iter().map(num).collect());
        let dt = run["dt"].as_str().map(|_| num(&run["dt"]));
        let frames = run["frames"].as_u64().unwrap() as usize;
        let sample = run["sample"].as_u64().unwrap() as usize;
        let mut snaps = run["snaps"].as_array().unwrap().iter();
        let mut t = num(&run["t0"]);
        for i in 0..frames {
            let d = match &dt_seq {
                Some(seq) => seq[i % seq.len()],
                None => dt.unwrap(),
            };
            step_line(&mut words, t, d);
            if i % sample == 0 || i == frames - 1 {
                let snap = snaps.next().unwrap();
                assert_eq!(snap["i"].as_u64().unwrap() as usize, i, "{name}: snapshot order");
                assert!(same(t, num(&snap["t"])), "{name}: frame {i} t {t:?} != {}", snap["t"]);
                compare(&frame_tokens(&words, false), snap["s"].as_str().unwrap())
                    .unwrap_or_else(|e| panic!("{name}: frame {i} (t = {t:?}): {e}"));
                frames_checked += 1;
            }
            t += d;
        }
        assert!(snaps.next().is_none(), "{name}: extra snapshots");

        reset_line(&mut words);
        compare(&frame_tokens(&words, true), run["reset"].as_str().unwrap()).unwrap_or_else(|e| panic!("{name}: reset: {e}"));
    }
    assert!(frames_checked > 1000, "{frames_checked}");
    assert!(letter_units > 100, "{letter_units} letter units: the golden should cover letter waves");
}

#[test]
fn step_line_empty_is_noop() {
    step_line(&mut [], 1.0, 1.0 / 60.0);
}

#[test]
fn letter_windows_matches_golden() {
    let golden = common::golden("lyric_animator");
    let cases = golden["letter_windows"].as_array().unwrap();
    let mut some = 0;
    for c in cases {
        let (start, end) = (num(&c["start"]), num(&c["end"]));
        let count = c["count"].as_f64().unwrap();
        let an = style_of(c["animation"].as_str().unwrap());
        let got = letter_windows(start, end, count, an);
        let what = format!("letterWindows({start:?}, {end:?}, {count}, {an:?})");
        match (&c["out"], got) {
            (Value::Null, None) => {}
            (Value::Null, Some(g)) => panic!("{what}: got {g:?}, expected null"),
            (e, None) => panic!("{what}: got null, expected {e}"),
            (e, Some(g)) => {
                some += 1;
                assert!(same(g.group_end, num(&e["group_end"])), "{what}: group_end {:?} != {}", g.group_end, e["group_end"]);
                assert!(same(g.per, num(&e["per"])), "{what}: per {:?} != {}", g.per, e["per"]);
            }
        }
    }
    assert!(cases.len() > 500 && some > 100);
}

#[test]
fn state_of_matches_golden() {
    let golden = common::golden("lyric_animator");
    for c in golden["state_of"].as_array().unwrap() {
        let (t, s, e) = (num(&c["t"]), num(&c["s"]), num(&c["e"]));
        let got = match state_of(t, s, e) {
            UnitState::NotSung => "NotSung",
            UnitState::Active => "Active",
            UnitState::Sung => "Sung",
        };
        assert_eq!(got, c["out"].as_str().unwrap(), "stateOf({t:?}, {s:?}, {e:?})");
    }
}

/// toFixed values taken from Node: exact ties pick the larger magnitude.
#[test]
fn to_fixed_matches_js() {
    assert_eq!(to_fixed(0.03125, 4), "0.0313");
    assert_eq!(to_fixed(-0.03125, 4), "-0.0313");
    assert_eq!(to_fixed(2.5, 0), "3");
    assert_eq!(to_fixed(1.005, 2), "1.00");
    assert_eq!(to_fixed(-0.0, 4), "0.0000");
    assert_eq!(to_fixed(-0.00001, 4), "-0.0000");
    assert_eq!(to_fixed(0.0625, 3), "0.063");
    assert_eq!(to_fixed(100.0, 2), "100.00");
    assert_eq!(fmt_pct(-20.0), "-20.00%");
}
