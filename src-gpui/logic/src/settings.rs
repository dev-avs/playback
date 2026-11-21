//! src/settings.ts, minus the React hooks and localStorage access (port.md 2.2, 2.3).
//!
//! `generated` is the `Settings` struct, its defaults and `KEYS`, and `items`
//! is the settings window's rows; both are written by
//! `node scripts/gen-gpui-settings.mts`, never by hand.
//!
//! Function for function, in the order of settings.ts, with the line of the
//! original above each. JavaScript's own behaviour that the originals lean on
//! (number to text, `toFixed`, `Math.round`, `String.prototype.trim`,
//! `JSON.stringify`) is at the end of the file.

use serde_json::Value;

#[path = "settings_gen.rs"]
pub mod generated;
#[path = "settings_items_gen.rs"]
pub mod items;

pub use generated::{Settings, KEYS};

// src/settings.ts:4
/// Playback's logo as an image: what shows where a cover would, when there is none.
pub fn logo_art() -> String {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0b0b0d"/><g stroke-linecap="round" stroke="#fff"><line x1="17" y1="18" x2="40" y2="18" stroke-opacity=".28" stroke-width="5"/><line x1="13" y1="32" x2="51" y2="32" stroke-opacity=".28" stroke-width="9"/><line x1="13" y1="32" x2="37" y2="32" stroke-width="9"/><line x1="17" y1="46" x2="34" y2="46" stroke-opacity=".28" stroke-width="5"/></g></svg>"##;
    format!("data:image/svg+xml;utf8,{}", encode_uri_component(svg))
}

// src/settings.ts:21
/// Lyric fonts on offer: name and the --latin-stack it sets (None leaves the stylesheet's own).
pub const LYRIC_FONTS: &[(&str, Option<&str>)] = &[
    ("Default", None),
    ("Segoe UI", Some("\"Segoe UI Variable Display\", \"Segoe UI\"")),
    ("Bahnschrift", Some("\"Bahnschrift\"")),
    ("Georgia", Some("\"Georgia\"")),
    ("Mono", Some("\"Cascadia Code\", \"Consolas\"")),
];

// src/settings.ts:33
/// The --latin-stack value for a saved font: a preset's stack, or any installed
/// family picked from the full list. None leaves the stylesheet's default.
///
/// (`name in LYRIC_FONTS` is also true for the names on Object.prototype in
/// JavaScript, `toString` and the like; those are not special-cased here.)
pub fn font_stack(name: &str) -> Option<String> {
    if let Some((_, stack)) = LYRIC_FONTS.iter().find(|(n, _)| *n == name) {
        return stack.map(str::to_string);
    }
    if !name.is_empty() {
        Some(format!("\"{}\"", name.replace(['"', '\\'], "")))
    } else {
        None
    }
}

// src/settings.ts:42
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomaMode {
    Off,
    Latin,
    Dual,
}
impl RomaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RomaMode::Off => "off",
            RomaMode::Latin => "latin",
            RomaMode::Dual => "dual",
        }
    }
}
// src/settings.ts:43
pub const ROMA_MODES: [RomaMode; 3] = [RomaMode::Off, RomaMode::Latin, RomaMode::Dual];
// src/settings.ts:44
pub const ROMA_LABELS: [(&str, &str); 3] = [("off", "Off"), ("latin", "Latin only"), ("dual", "Both")];
// src/settings.ts:45
pub fn roma_mode(v: &Value) -> RomaMode {
    if *v == Value::Bool(true) {
        return RomaMode::Latin;
    }
    match v.as_str() {
        Some(s) => ROMA_MODES.iter().copied().find(|m| m.as_str() == s).unwrap_or(RomaMode::Off),
        None => RomaMode::Off,
    }
}

// src/settings.ts:57
/// The block that carries an "off" from the old showNext toggle over to nextSong.
/// Takes what localStorage holds for `set.nextSong` and `set.showNext`; returns
/// the text to store under `set.nextSong`, if any.
pub fn carry_over_next_song(next_song: Option<&str>, show_next: Option<&str>) -> Option<String> {
    if next_song.is_none() && show_next == Some("false") {
        return Some(stringify(&Value::String("off".to_string()), 0));
    }
    None
}

// src/settings.ts:233
pub struct HighlightColour {
    pub name: &'static str,
    pub h: f64,
    pub s: f64,
    pub l: f64,
}
pub const HIGHLIGHTS: &[(&str, HighlightColour)] = &[
    ("white", HighlightColour { name: "White", h: 0.0, s: 0.0, l: 100.0 }),
    ("pink", HighlightColour { name: "Pink", h: 330.0, s: 90.0, l: 78.0 }),
    ("gold", HighlightColour { name: "Gold", h: 45.0, s: 95.0, l: 65.0 }),
    ("mint", HighlightColour { name: "Mint", h: 155.0, s: 70.0, l: 70.0 }),
    ("sky", HighlightColour { name: "Sky", h: 200.0, s: 90.0, l: 72.0 }),
    ("rainbow", HighlightColour { name: "Rainbow", h: 0.0, s: 90.0, l: 72.0 }),
];

// src/settings.ts:244
/// CSS variables for a highlight colour, consumed as --lit-h/s/l.
pub fn highlight_vars(h: &str) -> Vec<(&'static str, Value)> {
    let c = &HIGHLIGHTS.iter().find(|(k, _)| *k == h).unwrap_or(&HIGHLIGHTS[0]).1;
    vec![
        ("--lit-h", Value::from(c.h)),
        ("--lit-s", Value::String(format!("{}%", js_num(c.s)))),
        ("--lit-l", Value::String(format!("{}%", js_num(c.l)))),
    ]
}

// src/settings.ts:251
/// The fill's travel: along the line as it is sung, or top to bottom (spicy's).
pub const SWEEP_ANGLES: [(&str, &str); 2] = [("across", "90deg"), ("down", "180deg")];
// src/settings.ts:253
/// Right-to-left lines (Arabic, Hebrew) sweep across the other way.
pub const SWEEP_ANGLES_RTL: [(&str, &str); 2] = [("across", "270deg"), ("down", "180deg")];

// src/settings.ts:256
/// Glass colours: name and the RGB of the widget's tint (alpha is Background).
pub const GLASS_TINTS: &[(&str, &str, &str)] = &[
    ("dark", "Dark", "20, 20, 22"),
    ("black", "Black", "0, 0, 0"),
    ("warm", "Warm", "56, 34, 22"),
    ("cool", "Cool", "18, 30, 56"),
];

// src/settings.ts:264
pub const LYRIC_WEIGHTS: [(&str, f64); 3] = [("regular", 400.0), ("semibold", 600.0), ("bold", 700.0)];

// src/settings.ts:268
/// Corner radius of the small cover next to the title.
pub const COVER_SHAPES: [(&str, &str); 3] = [("rounded", "6px"), ("square", "2px"), ("circle", "50%")];

// src/settings.ts:276
pub struct CjkFont {
    pub name: &'static str,
    pub stack: &'static str,
    pub options: &'static [&'static str],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CjkLang {
    Ja,
    Ko,
    Zh,
}
/// Lyric faces per language; `stack` is the built-in choice.
pub const CJK_FONTS: [(CjkLang, &str, CjkFont); 3] = [
    (
        CjkLang::Ja,
        "ja",
        CjkFont {
            name: "Japanese font",
            stack: "\"Yu Gothic UI\", \"Meiryo\"",
            options: &["Yu Gothic UI", "Meiryo", "MS Gothic", "BIZ UDPGothic", "Noto Sans JP"],
        },
    ),
    (
        CjkLang::Ko,
        "ko",
        CjkFont {
            name: "Korean font",
            stack: "\"Noto Sans KR\", \"Malgun Gothic\"",
            options: &["Noto Sans KR", "Malgun Gothic", "Gulim", "Batang"],
        },
    ),
    (
        CjkLang::Zh,
        "zh",
        CjkFont {
            name: "Chinese font",
            stack: "\"Microsoft YaHei UI\", \"Microsoft JhengHei UI\", \"Microsoft YaHei\"",
            options: &["Microsoft YaHei UI", "Microsoft JhengHei UI", "SimHei", "DengXian", "Noto Sans SC"],
        },
    ),
];

// src/settings.ts:296
/// The font stack for a language, with the picked font (if any) first.
pub fn cjk_stack(lang: CjkLang, font: &str) -> String {
    let base = CJK_FONTS.iter().find(|(l, _, _)| *l == lang).unwrap().2.stack;
    if font == "default" {
        base.to_string()
    } else {
        format!("\"{}\", {}", font.replace(['"', '\\'], ""), base)
    }
}

// src/settings.ts:302
/// Scale on the letters' lift and growth: calm holds them still.
pub const MOTIONS: [(&str, f64); 3] = [("calm", 0.0), ("normal", 1.0), ("bouncy", 1.8)];
// src/settings.ts:305
/// The providers that can be picked, in cycle order.
pub const PROVIDERS: [&str; 4] = ["spicy", "lrcmux", "netease", "lrclib"];
// src/settings.ts:306
pub const PROVIDER_NAMES: [(&str, &str); 5] = [
    ("auto", "Auto"),
    ("spicy", "Spicy"),
    ("lrcmux", "LRCMux"),
    ("netease", "NetEase"),
    ("lrclib", "LrcLib"),
];

// src/settings.ts:315
/// "netease/yrc" → "netease"
pub fn provider_of(source: Option<&str>) -> String {
    source.unwrap_or("").split('/').next().unwrap().to_string()
}

// src/settings.ts:320
/// ●●● syllable, ●●○ word, ●○○ line. `None` where JavaScript's `repeat` throws a RangeError.
pub fn tier_dots(tier: f64) -> Option<String> {
    let dots = repeat("●", tier)?;
    // Math.max(0, 3 - tier): NaN stays NaN (repeat takes it as 0)
    let rest = 3.0 - tier;
    let rest = if rest.is_nan() || rest > 0.0 { rest } else { 0.0 };
    Some(dots + &repeat("○", rest)?)
}

// src/settings.ts:325
/// Identifies a song across players and albums for its own offset.
pub fn song_key(artist: &str, title: &str) -> String {
    format!("{}|{}", js_trim(artist).to_lowercase(), js_trim(title).to_lowercase())
}

// src/settings.ts:426
/// `read` without the localStorage lookup: what is stored (None = nothing),
/// parsed as JSON, or `initial` when nothing is stored or it does not parse.
pub fn read(raw: Option<&str>, initial: Value) -> Value {
    match raw {
        None => initial,
        Some(raw) => serde_json::from_str(raw).unwrap_or(initial),
    }
}

// src/settings.ts:435
/// `write` without the localStorage call: the text it stores.
pub fn write(value: &Value) -> String {
    stringify(value, 0)
}

// src/windows/Versions.tsx:26
/// The JSON `snapshotSettingsJson()` writes to gpui-settings.json when someone
/// switches flavor: every setting, from what localStorage holds (`stored` looks a
/// storage key up) or its default.
pub fn snapshot_settings_json(stored: impl Fn(&str) -> Option<String>) -> String {
    let defaults = Settings::default().entries();
    let mut out: Vec<(&str, Value)> = Vec::new();
    for (k, def) in defaults {
        let storage_key = KEYS.iter().find(|(name, _)| *name == k).map(|(_, v)| *v).filter(|s| !s.is_empty());
        let raw = storage_key.and_then(&stored);
        let v = match raw {
            Some(raw) => serde_json::from_str(&raw).unwrap_or(def),
            None => def,
        };
        out.push((k, v));
    }
    stringify_entries(&out, 2)
}

// src/windows/Settings.tsx:306-308 and the `fmt` of each slider row (the variants of `items::Fmt`)
/// A slider row's hint: its value as the settings window prints it.
pub fn fmt_slider(fmt: items::Fmt, v: f64) -> String {
    use items::Fmt;
    match fmt {
        // :306 (v === 0 ? "in sync" : `${v > 0 ? "+" : ""}${v.toFixed(1)}s`)
        Fmt::FmtOffset => {
            if v == 0.0 {
                "in sync".to_string()
            } else {
                format!("{}{}s", if v > 0.0 { "+" } else { "" }, to_fixed(v, 1))
            }
        }
        // :836 (v ? `after ${v}s` : "off")
        Fmt::AutoSync => {
            if truthy(v) {
                format!("after {}s", js_num(v))
            } else {
                "off".to_string()
            }
        }
        // :839 `${v} ms`
        Fmt::FollowSpeed => format!("{} ms", js_num(v)),
        // :842 `${v}×`
        Fmt::ScrollSpeed => format!("{}×", js_num(v)),
        // :844 `${v}px`
        Fmt::PanelHeight => format!("{}px", js_num(v)),
        // :965 v.toFixed(2)
        Fmt::LineHeight => to_fixed(v, 2),
        // :968 (v ? `${v > 0 ? "+" : ""}${v.toFixed(2)}em` : "normal")
        Fmt::LetterSpacing => {
            if truthy(v) {
                format!("{}{}em", if v > 0.0 { "+" } else { "" }, to_fixed(v, 2))
            } else {
                "normal".to_string()
            }
        }
        // :990 `${v}s`
        Fmt::RainbowSpeed => format!("{}s", js_num(v)),
        // :308 `${Math.round(v * 100)}%`
        Fmt::Times => format!("{}%", js_num(js_round(v * 100.0))),
        // :307 `${Math.round(v)}%`
        Fmt::Pct => format!("{}%", js_num(js_round(v))),
        // :1025 `${v}%${v === 0 ? " · clear" : v === 100 ? " · solid" : v === DEFAULTS.bgAlpha ? " · default" : ""}`
        Fmt::BgAlpha => {
            let tail = if v == 0.0 {
                " · clear"
            } else if v == 100.0 {
                " · solid"
            } else if v == Settings::default().bg_alpha {
                " · default"
            } else {
                ""
            };
            format!("{}%{}", js_num(v), tail)
        }
        // :1046 `${v} px/s`
        Fmt::PanSpeed => format!("{} px/s", js_num(v)),
        // :1069 `${v + 2}px`
        Fmt::BarThickness => format!("{}px", js_num(v + 2.0)),
    }
}

// ───────────────────────── JavaScript's own behaviour ─────────────────────────

/// JavaScript's truthiness of a number: not 0, -0 or NaN.
fn truthy(v: f64) -> bool {
    !(v == 0.0 || v.is_nan())
}

/// `String(v)` / a template literal's `${v}` for a number.
pub fn js_num(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == 0.0 {
        return "0".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    // shortest round-trip digits d.ddd e E  →  k digits, decimal point position n
    let sci = format!("{:e}", v.abs());
    let (mant, exp) = sci.split_once('e').unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap() + 1;
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let e = n - 1;
        let sign_e = if e < 0 { "-" } else { "+" };
        if k == 1 {
            format!("{}e{}{}", digits, sign_e, e.abs())
        } else {
            format!("{}.{}e{}{}", &digits[..1], &digits[1..], sign_e, e.abs())
        }
    };
    format!("{sign}{body}")
}

/// `Math.round`: ties go toward +Infinity.
pub fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `x.toFixed(digits)`: the exact decimal value of the double, rounded half up.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.abs() >= 1e21 {
        return js_num(x);
    }
    let neg = x < 0.0;
    // a double's exact decimal expansion has at most 1074 digits after the point
    let exact = format!("{:.1100}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap();
    let mut kept: Vec<u8> = int.bytes().chain(frac.bytes().take(digits)).collect();
    let round_up = frac.as_bytes()[digits] >= b'5';
    if round_up {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let int_len = kept.len() - digits;
    let int_part = String::from_utf8(kept[..int_len].to_vec()).unwrap();
    let frac_part = String::from_utf8(kept[int_len..].to_vec()).unwrap();
    let body = if digits == 0 { int_part } else { format!("{int_part}.{frac_part}") };
    format!("{}{}", if neg { "-" } else { "" }, body)
}

/// `s.repeat(count)`, or None for a count that makes it throw (negative, infinite, or
/// longer than V8's longest string, 2^29 - 24 UTF-16 units).
fn repeat(s: &str, count: f64) -> Option<String> {
    let n = if count.is_nan() { 0.0 } else { count.trunc() };
    if n < 0.0 || n * (s.encode_utf16().count() as f64) > 536_870_888.0 {
        return None;
    }
    Some(s.repeat(n as usize))
}

/// `String.prototype.trim`: JavaScript's WhiteSpace and LineTerminator set.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
                | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `JSON.stringify(value, null, indent)` (indent 0 = compact).
pub fn stringify(value: &Value, indent: usize) -> String {
    let mut out = String::new();
    stringify_into(value, indent, 0, &mut out);
    out
}

/// `JSON.stringify` of an object whose keys keep the order given.
pub fn stringify_entries(entries: &[(&str, Value)], indent: usize) -> String {
    let mut out = String::new();
    object_into(entries.iter().map(|(k, v)| (*k, v)), indent, 0, &mut out);
    out
}

fn stringify_into(value: &Value, indent: usize, depth: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let f = n.as_f64().unwrap();
            out.push_str(&if f.is_finite() { js_num(f) } else { "null".to_string() });
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(indent, depth + 1, out);
                stringify_into(v, indent, depth + 1, out);
            }
            newline(indent, depth, out);
            out.push(']');
        }
        Value::Object(m) => object_into(m.iter().map(|(k, v)| (k.as_str(), v)), indent, depth, out),
    }
}

fn object_into<'a>(entries: impl Iterator<Item = (&'a str, &'a Value)>, indent: usize, depth: usize, out: &mut String) {
    let mut any = false;
    out.push('{');
    for (k, v) in entries {
        if any {
            out.push(',');
        }
        any = true;
        newline(indent, depth + 1, out);
        out.push_str(&serde_json::to_string(k).unwrap());
        out.push(':');
        if indent > 0 {
            out.push(' ');
        }
        stringify_into(v, indent, depth + 1, out);
    }
    if any {
        newline(indent, depth, out);
    }
    out.push('}');
}

fn newline(indent: usize, depth: usize, out: &mut String) {
    if indent > 0 {
        out.push('\n');
        out.push_str(&" ".repeat(indent * depth));
    }
}
