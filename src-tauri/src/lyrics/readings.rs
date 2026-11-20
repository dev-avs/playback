//! Japanese readings for kanji, for romanizing.
//!
//! Kanji has no algorithmic reading, so it needs a dictionary: IPADIC, the one
//! kuromoji used in electron/readings.cjs, splits each line into words and
//! gives every word its reading in context (女の子 → オンナノコ). The
//! dictionary is large once loaded, so it is loaded per request and dropped
//! after, and answers are remembered instead.

use crate::support::cache::Cache;
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use serde::Serialize;
use std::borrow::Cow;
use std::sync::LazyLock;

/// One word: surface, reading (katakana), part of speech, its first detail,
/// base form. The part of speech decides where the romaji gets spaces
/// (src/romanize.ts, attaches).
#[derive(Clone, Serialize)]
pub struct Token {
    s: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    r: Option<String>,
    p: String,
    d: String,
    b: String,
}

// IPADIC's fields: part of speech, three details, conjugation type and form,
// base form, reading, pronunciation. An unknown word has "*" for its reading,
// which kuromoji left out, so the romaji keeps the word as written.
fn token(surface: &str, details: &[&str]) -> Token {
    let field = |i: usize| details.get(i).copied().unwrap_or("*").to_string();
    Token {
        s: surface.to_string(),
        r: details
            .get(7)
            .filter(|r| !r.is_empty() && **r != "*")
            .map(|r| r.to_string()),
        p: field(0),
        d: field(1),
        b: field(6),
    }
}

fn tokenize(texts: &[String]) -> lindera::LinderaResult<Vec<Vec<Token>>> {
    let segmenter = Segmenter::new(Mode::Normal, load_dictionary("embedded://ipadic")?, None);
    texts
        .iter()
        .map(|text| {
            let mut words = segmenter.segment(Cow::Borrowed(text.as_str()))?;
            let mut line = Vec::with_capacity(words.len());
            let mut at = 0;
            for w in words.iter_mut() {
                if w.byte_start > at {
                    line.push(space(&text[at..w.byte_start]));
                }
                line.push(token(&w.surface.clone(), &w.details()));
                at = w.byte_end;
            }
            if at < text.len() {
                line.push(space(&text[at..]));
            }
            Ok(line)
        })
        .collect()
}

/// Lindera skips whitespace; kuromoji gave it a token of its own, which the
/// romaji keeps as the gap between words.
fn space(gap: &str) -> Token {
    token(gap, &["記号", "空白"])
}

static FOUND: LazyLock<Cache<Vec<Vec<Token>>>> = LazyLock::new(|| Cache::new(20));

/// One token list per text, in order, covering every character of it; None
/// on failure, and the lyrics then romanize without readings.
pub async fn fetch(texts: Vec<String>) -> Option<Vec<Vec<Token>>> {
    let key = texts.join("\n");
    if let Some(hit) = FOUND.get(&key) {
        return Some(hit);
    }
    let found = tauri::async_runtime::spawn_blocking(move || tokenize(&texts))
        .await
        .ok()?;
    match found {
        Ok(tokens) => {
            FOUND.put(key, tokens.clone());
            Some(tokens)
        }
        Err(e) => {
            eprintln!("[readings] {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_kanji_in_context() {
        let tokens = super::tokenize(&["女の子が走る".to_string()]).unwrap();
        let line = &tokens[0];
        assert_eq!(
            line.iter().map(|t| t.s.as_str()).collect::<String>(),
            "女の子が走る"
        );
        assert_eq!(line[0].r.as_deref(), Some("オンナノコ"));
        assert_eq!(line.last().unwrap().b, "走る");
    }

    #[test]
    fn leaves_unknown_words_unread() {
        let tokens = super::tokenize(&["Hello 世界".to_string()]).unwrap();
        assert_eq!(tokens[0][0].s, "Hello");
        assert_eq!(tokens[0][0].r, None);
        assert_eq!(tokens[0][0].b, "*");
        assert_eq!(
            (tokens[0][1].s.as_str(), tokens[0][1].d.as_str()),
            (" ", "空白")
        );
    }
}
