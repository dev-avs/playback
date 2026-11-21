//! src/lyrics/spicy.ts
//!
//! The payload is a `serde_json::Value`, read the way the TypeScript reads its
//! loosely typed interfaces: a field that is missing or `null` is `undefined`
//! (`prop`), and `??` falls through exactly on those. A payload whose fields
//! have the wrong JSON type (a number for `Text`, say) would make the
//! TypeScript throw or coerce; here such a field counts as missing. Not
//! ported for that reason: a `null` `EndTime` stays `null` in the TypeScript's
//! output where here it is absent, and `PROVIDER_NAMES[source]` finds
//! `Object.prototype` members for a source named `constructor` or `toString`.
//!
//! `loadSpicyLyrics` is async over `window.player`; here `SpicySource` is
//! that bridge and the function is synchronous, with the decisions unchanged.

use crate::lyrics::{tier, LyricLine, LyricWord, Tier};
use crate::objpack::SLObjPack;
use crate::romanize::has_non_latin_script;
use serde::Serialize;
use serde_json::Value;

// src/lyrics/spicy.ts:59
/// Who to credit for these lyrics, as the API's terms require on screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpicyAttribution {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<SpicyPerson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maker: Option<SpicyPerson>,
}

/// `{ name: string; url?: string }` (spicy.ts:61)
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpicyPerson {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// src/lyrics/spicy.ts:65
const PROVIDER_NAMES: [(&str, &str); 3] = [
    ("spicy_lyrics", "Spicy Lyrics"),
    ("apple_music", "Apple Music"),
    ("spotify", "Spotify"),
];

/// A property as `data.Key` reads it, with `null` and a missing key both `undefined`.
fn prop<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v.get(key) {
        Some(Value::Null) | None => None,
        other => other,
    }
}

fn prop_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    prop(v, key).and_then(Value::as_str)
}

fn prop_f64(v: &Value, key: &str) -> Option<f64> {
    prop(v, key).and_then(Value::as_f64)
}

/// `x?.Key === true`
fn prop_is_true(v: &Value, key: &str) -> bool {
    matches!(prop(v, key), Some(Value::Bool(true)))
}

/// `Array.isArray(x) ? x : []` (the elements of `x ?? []` when it is an array)
fn prop_array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    match prop(v, key) {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}

/// JavaScript truthiness of a defined value.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => {
            let x = n.as_f64().unwrap();
            x != 0.0 && !x.is_nan()
        }
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `String.prototype.trim`: WhiteSpace and LineTerminator of ECMAScript, which
/// is not Rust's `char::is_whitespace` (U+FEFF trims, U+0085 does not).
fn is_js_trim_char(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | '\u{20}' | '\u{A0}' | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_trim_char)
}

// src/lyrics/spicy.ts:71
fn attribution_of(data: &Value) -> Option<SpicyAttribution> {
    let source = prop(data, "source");
    if !js_truthy(source) {
        return None;
    }
    let source = source.and_then(Value::as_str);
    let person = |c: Option<&Value>| -> Option<SpicyPerson> {
        let c = c?;
        if js_truthy(prop(c, "username")) {
            Some(SpicyPerson {
                name: prop_str(c, "username").unwrap_or_default().to_string(),
                url: prop_str(c, "url").map(str::to_string),
            })
        } else {
            None
        }
    };
    let credit = if source == Some("spicy_lyrics") { prop(data, "UploadAttribution") } else { None };
    Some(SpicyAttribution {
        provider: PROVIDER_NAMES
            .iter()
            .find(|(k, _)| Some(*k) == source)
            .map(|(_, v)| *v)
            .unwrap_or("Spicy Lyrics")
            .to_string(),
        uploader: person(credit.and_then(|c| prop(c, "Uploader"))),
        maker: person(credit.and_then(|c| prop(c, "Maker"))),
    })
}

// src/lyrics/spicy.ts:82
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpicyResult {
    pub lines: Vec<LyricLine>,
    /// transliterated variant, present only when the payload carries one
    pub romanized: Option<Vec<LyricLine>>,
    /// display-ready credit lines, rendered apart from the lyrics
    pub credits: Vec<String>,
    pub synced: bool,
    /// present for answers from the public API, which must be credited on screen
    pub attribution: Option<SpicyAttribution>,
    /// Finest timing this payload carries.
    pub tier: Tier,
}

// src/lyrics/spicy.ts:100 `const packer = new SLObjPack();`
fn packer() -> SLObjPack {
    SLObjPack::default()
}

// src/lyrics/spicy.ts:112
/// One LyricWord per syllable, carrying its real StartTime/EndTime.
fn syllables_to_words(syllables: &[Value], transliterate: bool) -> Vec<LyricWord> {
    let mut words: Vec<LyricWord> = Vec::new();
    for s in syllables {
        let text = js_trim(
            (if transliterate {
                prop_str(s, "TransliteratedText").or(prop_str(s, "Text"))
            } else {
                prop_str(s, "Text")
            })
            .unwrap_or(""),
        );
        if text.is_empty() {
            continue;
        }
        words.push(LyricWord {
            // a syllable without StartTime is outside the payload's shape (undefined in TypeScript)
            t: prop_f64(s, "StartTime").unwrap_or(0.0),
            end: prop_f64(s, "EndTime"),
            text: text.to_string(),
            // Romanized CJK syllables stand apart (一二三 → "ichi ni san"); glued
            // they ran together into one unreadable word.
            join_next: Some(
                prop_is_true(s, "IsPartOfWord")
                    && !(transliterate && has_non_latin_script(prop_str(s, "Text").unwrap_or(""))),
            ),
            read_join: None,
        });
    }
    words
}

// src/lyrics/spicy.ts:139
/// Rebuild the display text so joinNext runs of syllables print as one token.
fn join_syllable_text(words: &[LyricWord]) -> String {
    let mut text = String::new();
    for i in 0..words.len() {
        text.push_str(&words[i].text);
        if i + 1 < words.len() && words[i].join_next != Some(true) {
            text.push(' ');
        }
    }
    text
}

/// `Partial<LyricLine>` as vocalGroupToLine's callers pass it (`opposite`, `bg`).
struct Extra {
    opposite: Option<bool>,
    bg: Option<bool>,
}

// src/lyrics/spicy.ts:148
fn vocal_group_to_line(group: &Value, transliterate: bool, extra: Extra) -> Option<LyricLine> {
    let syllables = prop_array(group, "Syllables");
    if syllables.is_empty() {
        return None;
    }
    let words = syllables_to_words(syllables, transliterate);
    if words.is_empty() {
        return None;
    }
    Some(LyricLine {
        t: prop_f64(group, "StartTime").unwrap_or(words[0].t),
        // Use the real EndTime from the spicy-lyrics payload (seconds).
        end: prop_f64(group, "EndTime").or(words[words.len() - 1].end),
        text: join_syllable_text(&words),
        words: Some(words),
        opposite: extra.opposite,
        bg: extra.bg,
        ..Default::default()
    })
}

// src/lyrics/spicy.ts:167
fn convert(data: &Value, transliterate: bool) -> Vec<LyricLine> {
    let content = match data.get("Content") {
        Some(Value::Array(a)) => a.as_slice(),
        _ => &[],
    };
    let mut lines: Vec<LyricLine> = Vec::new();

    if prop_str(data, "Type") == Some("Syllable") {
        for raw in content {
            let opposite = prop_is_true(raw, "OppositeAligned");
            let lead = if js_truthy(prop(raw, "Lead")) {
                vocal_group_to_line(
                    prop(raw, "Lead").unwrap(),
                    transliterate,
                    Extra { opposite: Some(opposite), bg: None },
                )
            } else {
                None
            };
            let Some(lead) = lead else { continue };
            lines.push(lead);

            // Backup vocals become their own lines, inserted directly after the lead
            // they belong to.
            for bg in prop_array(raw, "Background") {
                let bg_line = vocal_group_to_line(bg, transliterate, Extra { opposite: Some(opposite), bg: Some(true) });
                if let Some(bg_line) = bg_line {
                    lines.push(bg_line);
                }
            }
        }
        return lines;
    }

    if prop_str(data, "Type") == Some("Line") {
        for raw in content {
            let text = (if transliterate {
                prop_str(raw, "TransliteratedText").or(prop_str(raw, "Text"))
            } else {
                prop_str(raw, "Text")
            })
            .unwrap_or("");
            if js_trim(text).is_empty() {
                continue;
            }
            // Use the real EndTime from the spicy-lyrics payload (seconds).
            lines.push(LyricLine {
                t: prop_f64(raw, "StartTime").unwrap_or(0.0),
                end: prop_f64(raw, "EndTime"),
                text: js_trim(text).to_string(),
                opposite: Some(prop_is_true(raw, "OppositeAligned")),
                ..Default::default()
            });
        }
        return lines;
    }

    // "Static" — unsynced; no timings at all, so leave them at 0 and let the
    // caller decide (we simply never auto-scroll these).
    for raw in content {
        let text = (if transliterate {
            prop_str(raw, "TransliteratedText").or(prop_str(raw, "Text"))
        } else {
            prop_str(raw, "Text")
        })
        .unwrap_or("");
        if !js_trim(text).is_empty() {
            lines.push(LyricLine { t: 0.0, end: None, text: js_trim(text).to_string(), ..Default::default() });
        }
    }
    lines
}

// src/lyrics/spicy.ts:215
fn has_transliteration(data: &Value) -> bool {
    let content = match data.get("Content") {
        Some(Value::Array(a)) => a.as_slice(),
        _ => &[],
    };
    content.iter().any(|line| {
        if js_truthy(prop(line, "TransliteratedText")) {
            return true;
        }
        let lead_syllables = match prop(line, "Lead") {
            Some(lead) => prop_array(lead, "Syllables"),
            None => &[],
        };
        if lead_syllables.iter().any(|s| js_truthy(prop(s, "TransliteratedText"))) {
            return true;
        }
        prop_array(line, "Background")
            .iter()
            .any(|bg| prop_array(bg, "Syllables").iter().any(|s| js_truthy(prop(s, "TransliteratedText"))))
    })
}

/// `req` of `loadSpicyLyrics` (spicy.ts:232)
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SpicyRequest {
    pub artist: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

/// What `window.player.fetchSpicyLyrics` resolves to (src/types/player.d.ts:123),
/// without the fields spicy.ts never reads (`trackId`, `httpStatus`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpicyFetch {
    /// "ok" | "no-auth" | "no-track" | "not-found" | "queued" | "error" | "disabled"
    pub status: String,
    /// still binary-packed (`undefined` or `null` is `None`)
    pub packed: Option<Value>,
    /// already unpacked, from the Spicy Lyrics client's own cache (`undefined` or `null` is `None`)
    pub lyrics: Option<Value>,
}

/// The `window.player` calls `loadSpicyLyrics` makes.
pub trait SpicySource {
    /// `window.player?.fetchSpicyLyrics` is present (`const bridge = window.player; if (!bridge?.fetchSpicyLyrics) return null;`)
    fn has_fetch_spicy_lyrics(&self) -> bool;
    /// `await bridge.fetchSpicyLyrics(req)`; `None` is a resolved `null`.
    fn fetch_spicy_lyrics(&self, req: &SpicyRequest) -> Option<SpicyFetch>;
}

// src/lyrics/spicy.ts:232
/// Fetch + unpack lyrics for the current track via Spicy Lyrics.
/// Returns None whenever the source can't serve this track, so the caller can
/// fall back to the existing LrcLib/NetEase chain.
pub fn load_spicy_lyrics(bridge: &dyn SpicySource, req: &SpicyRequest) -> Option<SpicyResult> {
    if !bridge.has_fetch_spicy_lyrics() {
        return None;
    }

    let res = bridge.fetch_spicy_lyrics(req);
    let res = res.filter(|r| r.status == "ok")?;

    let data: Value;
    if let Some(lyrics) = res.lyrics {
        // Came from the Spicy Lyrics client's own cache via the Spicetify bridge.
        // It stores what it already unpacked, so there is nothing to decode here.
        data = lyrics;
    } else if let Some(packed) = res.packed {
        match packer().unpack(&packed) {
            Ok(unpacked) => data = unpacked,
            Err(e) => {
                eprintln!("[spicy] unpack failed {e}");
                return None;
            }
        }
    } else {
        return None;
    }
    // `!data || !Array.isArray(data.Content) || !data.Content.length`
    if !js_truthy(Some(&data)) {
        return None;
    }
    match data.get("Content") {
        Some(Value::Array(a)) if !a.is_empty() => {}
        _ => return None,
    }

    let lines = convert(&data, false);
    if lines.is_empty() {
        return None;
    }

    let romanized = if has_transliteration(&data) { Some(convert(&data, true)) } else { None };

    // Matches ApplyLyricsCredits.ts: one "Written by: …" line built from the
    // structured array.
    let writers: Vec<&str> = prop_array(&data, "SongWriters")
        .iter()
        .filter_map(Value::as_str)
        .filter(|w| !js_trim(w).is_empty())
        .collect();

    let kind = prop_str(&data, "Type");
    let synced = kind == Some("Syllable") || kind == Some("Line");
    Some(SpicyResult {
        lines,
        romanized: romanized.filter(|r| !r.is_empty()),
        credits: if writers.is_empty() { Vec::new() } else { vec![format!("Written by: {}", writers.join(", "))] },
        synced,
        attribution: attribution_of(&data),
        tier: if kind == Some("Syllable") {
            tier::SYLLABLE
        } else if kind == Some("Line") {
            tier::LINE
        } else {
            tier::NONE
        },
    })
}
