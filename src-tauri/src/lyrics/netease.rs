//! NetEase Cloud Music: syllable timing (yrc) and a romanized track.
//!
//! Its catalogue for non-Chinese pop is mostly covers, remixes and karaoke
//! uploads, and lyrics from another recording cannot line up. So a search
//! result has to earn its place (`score_song`); when none does, there is no
//! NetEase hit and LrcLib's timing for the right recording wins instead.

use super::script::{script_matches, scripts_of};
use super::Found;
use crate::support::cache::Cache;
use crate::support::http::client;
use crate::support::json::{list_of, num_of, str_of};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;

const SOURCE: &str = "netease";
const REFERER: &str = "https://music.163.com";

static KARAOKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)karaoke|instrumental|伴奏|inst\.|off vocal").unwrap());
static REWORKED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)remix|edit|sped ?up|slowed|nightcore|mashup|version").unwrap());
static COVER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)cover|翻自|翻唱").unwrap());

/// Below this, no NetEase hit beats one.
const MIN_SCORE: i32 = 40;

/// How surely a search result is the recording being played; None when it is
/// another song altogether. `duration` in seconds; NetEase reports ms.
fn score_song(song: &Value, artist: &str, track: &str, duration: Option<f64>) -> Option<i32> {
    let name = str_of(song, "name").to_lowercase();
    let wanted = track.to_lowercase();
    let wanted_artist = artist.to_lowercase();
    if name.is_empty() {
        return None;
    }
    let mut score = if name == wanted {
        40
    } else if name.contains(&wanted) || wanted.contains(&name) {
        20
    } else {
        return None;
    };

    // Re-uploads impersonate the act with punctuation ("周杰伦-"), which is a
    // substring match every time: only an exact credit counts in full.
    let artists: Vec<String> = list_of(song, "artists").iter().map(|a| str_of(a, "name").to_lowercase()).collect();
    if artists.contains(&wanted_artist) {
        score += 30;
    } else if artists.iter().any(|a| a.contains(&wanted_artist) || wanted_artist.contains(a.as_str())) {
        score += 10;
    }

    // Length ranks rather than vetoes: album, single and radio edits differ,
    // and a correctly credited title must survive that.
    if let (Some(d), Some(theirs)) = (duration, num_of(song, "duration").filter(|t| *t > 0.0)) {
        score += match (theirs / 1000.0 - d).abs() {
            x if x <= 2.0 => 30,
            x if x <= 5.0 => 15,
            x if x <= 15.0 => 0,
            x if x <= 30.0 => -15,
            _ => -25,
        };
    }

    if KARAOKE.is_match(&name) {
        score -= 60; // no lead vocal
    } else if REWORKED.is_match(&name) {
        score -= 30; // same words, every timestamp moved
    } else if COVER.is_match(&name) {
        score -= 10; // same words, usable when the length agrees
    }
    Some(score)
}

fn pick_song<'a>(songs: &'a [Value], artist: &str, track: &str, duration: Option<f64>) -> Option<&'a Value> {
    let mut best: Option<(&Value, i32)> = None;
    for song in songs {
        if let Some(s) = score_song(song, artist, track, duration) {
            if best.is_none_or(|(_, b)| s > b) {
                best = Some((song, s));
            }
        }
    }
    best.filter(|(_, s)| *s >= MIN_SCORE).map(|(song, _)| song)
}

async fn search(artist: &str, track: &str, duration: Option<f64>) -> Result<Option<i64>, reqwest::Error> {
    // 10, not 5: the right recording is often buried under re-uploads.
    let res = client()
        .get("https://music.163.com/api/search/get")
        .query(&[("s", format!("{track} {artist}").as_str()), ("type", "1"), ("limit", "10")])
        .header("Referer", REFERER)
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let body: Value = res.json().await?;
    let songs = body.pointer("/result/songs").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    Ok(pick_song(songs, artist, track, duration).and_then(|s| s["id"].as_i64()))
}

/// yv=1 asks for yrc (word timing), rv=-1 for romalrc (romanized).
async fn lyric(id: i64) -> Result<Option<Value>, reqwest::Error> {
    let res = client()
        .get(format!("https://music.163.com/api/song/lyric?id={id}&lv=1&kv=1&tv=-1&rv=-1&yv=1"))
        .header("Referer", REFERER)
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    Ok(Some(res.json().await?))
}

fn track_text<'a>(data: &'a Value, track: &str) -> &'a str {
    data.pointer(&format!("/{track}/lyric")).and_then(Value::as_str).unwrap_or("")
}

/// The words as sung: yrc when there is one, LRC otherwise. The `tlyric`
/// translation is ignored: it only ever translates into Chinese.
pub async fn find(artist: &str, track: &str, duration: Option<f64>) -> Found {
    let Some(id) = search(artist, track, duration).await? else { return Ok(None) };
    let Some(data) = lyric(id).await? else { return Ok(None) };
    let lrc = track_text(&data, "lrc");
    let yrc = track_text(&data, "yrc");
    let body = if yrc.is_empty() { lrc } else { yrc };
    if body.is_empty() || !script_matches(scripts_of(&format!("{track} {artist}")), body) {
        return Ok(None);
    }
    let mut found = json!({ "source": SOURCE });
    if !yrc.is_empty() {
        found["wordLyrics"] = yrc.into();
    }
    if !lrc.is_empty() {
        found["syncedLyrics"] = lrc.into();
    }
    Ok(Some(found))
}

static ROMANIZED: LazyLock<Cache<Option<String>>> = LazyLock::new(|| Cache::new(100));

/// NetEase's romanized track (the only source that romanizes kanji). A failed
/// lookup is not remembered, so it is asked again next time.
pub async fn romanized(artist: &str, title: &str) -> Option<String> {
    let key = format!("{artist}|{title}");
    if let Some(hit) = ROMANIZED.get(&key) {
        return hit;
    }
    let lookup = async {
        let Some(id) = search(artist, title, None).await? else { return Ok(None) };
        let data = lyric(id).await?;
        let roma = data.as_ref().map(|d| track_text(d, "romalrc")).unwrap_or("");
        Ok::<_, reqwest::Error>(Some(roma.to_string()).filter(|r| !r.trim().is_empty()))
    };
    match lookup.await {
        Ok(lrc) => {
            ROMANIZED.put(key, lrc.clone());
            lrc
        }
        Err(e) => {
            eprintln!("[lyrics:netease] romanized: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(name: &str, artist: &str, ms: f64) -> Value {
        json!({ "id": 1, "name": name, "artists": [{ "name": artist }], "duration": ms })
    }

    #[test]
    fn picks_the_recording_being_played() {
        let songs = [
            song("Shape of You (Remix)", "Ed Sheeran", 200_000.0),
            song("Shape of You", "Someone Else", 183_000.0),
            song("Shape of You", "Ed Sheeran", 233_700.0),
        ];
        let hit = pick_song(&songs, "Ed Sheeran", "Shape of You", Some(233.7)).unwrap();
        assert_eq!(hit["artists"][0]["name"], "Ed Sheeran");
        assert_eq!(hit["duration"], 233_700.0);
    }

    #[test]
    fn passes_over_karaoke_and_other_songs() {
        let songs = [song("Shape of You (Karaoke)", "Ed Sheeran", 233_700.0), song("Other Song", "Ed Sheeran", 233_700.0)];
        assert!(pick_song(&songs, "Ed Sheeran", "Shape of You", Some(233.7)).is_none());
    }

    #[test]
    fn a_correct_credit_survives_a_length_disagreement() {
        let songs = [song("Shape of You", "Ed Sheeran", 264_000.0)];
        assert!(pick_song(&songs, "Ed Sheeran", "Shape of You", Some(233.7)).is_some());
    }
}
