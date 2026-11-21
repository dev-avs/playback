//! LrcLib (lrclib.net): mostly line-timed, but the widest catalogue.

use super::script::{is_derived, script_matches, scripts_of};
use super::{with_source, Found};
use crate::support::http::client;
use crate::support::json::{num_of, str_of};
use serde_json::Value;

const SOURCE: &str = "lrclib";

fn has_lyrics(entry: &Value) -> bool {
    !str_of(entry, "syncedLyrics").is_empty() || !str_of(entry, "plainLyrics").is_empty()
}

fn text_of(entry: &Value) -> String {
    format!(
        "{}\n{}",
        str_of(entry, "syncedLyrics"),
        str_of(entry, "plainLyrics")
    )
}

/// The exact match first; then the fuzzy search, which tolerates the
/// "Song (feat. X) - Remastered" shapes Spotify reports.
pub async fn find(artist: &str, track: &str, album: Option<&str>, duration: Option<f64>) -> Found {
    let expected = scripts_of(&format!("{track} {artist} {}", album.unwrap_or("")));

    let mut query = vec![
        ("artist_name", artist.to_string()),
        ("track_name", track.to_string()),
    ];
    if let Some(a) = album {
        query.push(("album_name", a.into()));
    }
    if let Some(d) = duration {
        query.push(("duration", d.round().to_string()));
    }
    let res = client()
        .get("https://lrclib.net/api/get")
        .query(&query)
        .send()
        .await?;
    if res.status().is_success() {
        let d: Value = res.json().await?;
        if has_lyrics(&d) && !is_derived(&d) && script_matches(expected, &text_of(&d)) {
            return Ok(Some(with_source(d, SOURCE)));
        }
    }

    let res = client()
        .get("https://lrclib.net/api/search")
        .query(&[("artist_name", artist), ("track_name", track)])
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let list: Value = res.json().await?;
    let candidates = list.as_array().map(Vec::as_slice).unwrap_or(&[]);

    let score = |x: &Value| {
        let mut s = 0;
        if script_matches(expected, &text_of(x)) {
            s += 100;
        }
        if is_derived(x) {
            s -= 80;
        }
        if !str_of(x, "syncedLyrics").is_empty() {
            s += 20;
        }
        if let (Some(d), Some(t)) = (duration, num_of(x, "duration")) {
            if (t - d).abs() <= 3.0 {
                s += 15;
            }
        }
        if str_of(x, "trackName").to_lowercase() == track.to_lowercase() {
            s += 10;
        }
        s
    };

    // The first of the best; a negative best means everything on offer was in
    // the wrong language or a translation, and none beats the wrong words.
    let mut best: Option<(&Value, i32)> = None;
    for x in candidates.iter().filter(|x| has_lyrics(x)) {
        let s = score(x);
        if best.is_none_or(|(_, b)| s > b) {
            best = Some((x, s));
        }
    }
    Ok(best
        .filter(|(_, s)| *s >= 0)
        .map(|(x, _)| with_source(x.clone(), SOURCE)))
}
