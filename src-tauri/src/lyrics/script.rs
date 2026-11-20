//! Which writing systems a text uses. Lyrics in the wrong one are a
//! translation or a transliteration: a Korean song must not quietly get
//! English words.

use crate::support::json::str_of;
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

#[derive(Default, Clone, Copy)]
pub struct Scripts {
    hangul: bool,
    kana: bool,
    han: bool,
    cyrillic: bool,
}

pub fn scripts_of(text: &str) -> Scripts {
    let mut s = Scripts::default();
    for c in text.chars() {
        match c {
            '\u{AC00}'..='\u{D7A3}' => s.hangul = true,
            '\u{3040}'..='\u{30FF}' => s.kana = true,
            '\u{4E00}'..='\u{9FFF}' => s.han = true,
            '\u{0400}'..='\u{04FF}' => s.cyrillic = true,
            _ => {}
        }
    }
    s
}

/// Whether `lyrics` are in the script the track's metadata promises. Only
/// judged when that metadata is non-Latin: a Latin title says nothing about
/// the language of the words.
pub fn script_matches(expected: Scripts, lyrics: &str) -> bool {
    let got = scripts_of(lyrics);
    if expected.hangul {
        got.hangul
    } else if expected.kana {
        got.kana || got.han
    } else if expected.han {
        got.han
    } else if expected.cyrillic {
        got.cyrillic
    } else {
        true
    }
}

static DERIVED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(english|eng\s*(sub|trans)|translation|translated|romani[sz]ed|romaji|romanization|pinyin|vietsub)\b|번역|翻訳|译文|譯文",
    )
    .unwrap()
});

/// An LrcLib upload that says it is a translation or a transliteration.
pub fn is_derived(entry: &Value) -> bool {
    DERIVED.is_match(&format!(
        "{} {}",
        str_of(entry, "trackName"),
        str_of(entry, "artistName")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_lyrics_in_another_script() {
        let korean = scripts_of("사랑 노래");
        assert!(!script_matches(korean, "love song lyrics"));
        assert!(script_matches(korean, "사랑해"));
        assert!(script_matches(scripts_of("Love Song"), "anything"));
        assert!(script_matches(scripts_of("さくら"), "桜"));
    }

    #[test]
    fn spots_derived_uploads() {
        assert!(is_derived(
            &json!({ "trackName": "Song (English Translation)" })
        ));
        assert!(is_derived(&json!({ "trackName": "노래 번역" })));
        assert!(!is_derived(
            &json!({ "trackName": "Englishman in New York" })
        ));
    }
}
