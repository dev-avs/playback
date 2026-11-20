//! Lyrics from LRCMux, NetEase and LrcLib, best timing first. Spicy Lyrics is
//! asked before all of these, by the page.
//!
//! The answer has the shape src/lyrics.ts reads (LyricsResponse in
//! src/player.d.ts): `wordLyrics` (yrc), `syncedLyrics` (LRC), `plainLyrics`,
//! `source`, plus `tier`.

mod lrclib;
mod lrcmux;
mod netease;
mod script;

pub use netease::romanized;

use crate::support::json::str_of;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::sync::LazyLock;

/// One provider's answer: Some lyrics, None when it has none.
type Found = Result<Option<Value>, reqwest::Error>;

fn with_source(mut v: Value, source: &str) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.insert("source".into(), source.into());
    }
    v
}

#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    artist: String,
    #[serde(default)]
    title: String,
    album: Option<String>,
    duration: Option<f64>,
    /// Ask just this provider: the user picked it for this song.
    only: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Provider {
    LrcMux,
    NetEase,
    LrcLib,
}

impl Provider {
    /// Both LRCMux and NetEase carry syllable timing, so asking them first is
    /// what lets the search stop early. At equal timing, earlier wins.
    const ORDER: [Provider; 3] = [Provider::LrcMux, Provider::NetEase, Provider::LrcLib];

    fn name(self) -> &'static str {
        match self {
            Provider::LrcMux => "lrcmux",
            Provider::NetEase => "netease",
            Provider::LrcLib => "lrclib",
        }
    }

    async fn find(self, artist: &str, title: &str, album: Option<&str>, duration: Option<f64>) -> Found {
        match self {
            Provider::LrcMux => lrcmux::find(artist, title, album, duration).await,
            Provider::NetEase => netease::find(artist, title, duration).await,
            Provider::LrcLib => lrclib::find(artist, title, album, duration).await,
        }
    }
}

/// How finely lyrics know the timing; mirrored in src/lyrics.ts.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Tier {
    Untimed = 0,
    Line = 1,
    Word = 2,
    Syllable = 3,
}

fn tier_of(r: &Value) -> Tier {
    static WORD_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<\d+:\d+").unwrap());
    let synced = str_of(r, "syncedLyrics");
    if !str_of(r, "wordLyrics").is_empty() {
        Tier::Syllable
    } else if synced.is_empty() {
        Tier::Untimed
    } else if WORD_TAG.is_match(synced) {
        Tier::Word
    } else {
        Tier::Line
    }
}

static TITLE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*[-–]\s*(Remaster(ed)?|Radio Edit|Single Version|.*Version|.*Mix|Live).*$").unwrap()
});
static TITLE_FEAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s*[(\[](feat\.?|ft\.?|with)[^)\]]*[)\]]").unwrap());
static ARTIST_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i),|&|\sfeat\.?\s|\sft\.?\s").unwrap());

/// Spotify decorates titles in ways lyrics databases do not carry.
fn clean_title(title: &str) -> String {
    TITLE_FEAT.replace_all(&TITLE_SUFFIX.replace(title, ""), "").trim().to_string()
}

/// The best-timed lyrics any provider has, asked as reported and then as the
/// first artist with a cleaned-up title. Timing outranks provider order;
/// nothing beats syllable timing, so the first such answer ends the search.
pub async fn fetch(req: Request) -> Option<Value> {
    let first_artist = ARTIST_SPLIT.split(&req.artist).next().unwrap_or("").trim().to_string();
    let asks = [(req.artist.clone(), req.title.clone()), (first_artist, clean_title(&req.title))];
    let album = req.album.as_deref().filter(|a| !a.is_empty());
    let duration = req.duration.filter(|d| *d > 0.0);

    let mut best: Option<(Value, Tier)> = None;
    for provider in Provider::ORDER {
        if req.only.as_deref().is_some_and(|only| only != provider.name()) {
            continue;
        }
        for (artist, title) in asks.iter().filter(|(a, t)| !a.is_empty() && !t.is_empty()) {
            let found = match provider.find(artist, title, album, duration).await {
                Ok(Some(found)) => found,
                Ok(None) => continue,
                Err(e) => {
                    eprintln!("[lyrics:{}] {e}", provider.name());
                    continue;
                }
            };
            let tier = tier_of(&found);
            if tier == Tier::Syllable {
                return Some(found);
            }
            if tier > best.as_ref().map_or(Tier::Untimed, |b| b.1) {
                best = Some((found, tier));
            }
        }
    }
    // Untimed lyrics cannot be followed, so the track counts as having none.
    best.map(|(mut found, tier)| {
        found["tier"] = (tier as i32).into();
        found
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cleans_spotify_titles() {
        assert_eq!(clean_title("Song - Remastered 2011"), "Song");
        assert_eq!(clean_title("Song (feat. Someone)"), "Song");
        assert_eq!(clean_title("Song - Radio Edit"), "Song");
        assert_eq!(clean_title("Plain"), "Plain");
    }

    #[test]
    fn ranks_timing() {
        assert!(tier_of(&json!({ "wordLyrics": "x" })) == Tier::Syllable);
        assert!(tier_of(&json!({ "syncedLyrics": "[00:01.00]<00:01.00>a" })) == Tier::Word);
        assert!(tier_of(&json!({ "syncedLyrics": "[00:01.00]a" })) == Tier::Line);
        assert!(tier_of(&json!({ "plainLyrics": "a" })) == Tier::Untimed);
    }
}
