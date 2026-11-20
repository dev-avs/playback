//! LRCMux (lrcmux.dev): gathers KuGou, LrcLib, YouTube Music and more. What
//! it adds is KuGou's per-word timing. That comes back as JSON and is turned
//! into NetEase's yrc layout here, so it ranks and parses the same.

use super::Found;
use crate::support::http::client;
use crate::support::json::{list_of, num_of, str_of};
use serde_json::{json, Value};

const SOURCE: &str = "lrcmux";

/// A recording whose length differs by more than this is another edit, and
/// its timings would drift.
const MAX_MISMATCH_S: f64 = 8.0;

/// `mm:ss.xx`, as LRC writes it.
fn lrc_time(ms: f64) -> String {
    let t = ms.max(0.0) / 1000.0;
    let m = (t / 60.0).floor();
    format!("{:02}:{:05.2}", m as u64, t - m * 60.0)
}

/// `[lineStart,lineDur](wordStart,wordDur,0)word…` in ms, one line each; a
/// line with no word data becomes one word spanning the line.
fn to_yrc(lines: &[Value]) -> String {
    let line = |l: &Value| -> Option<String> {
        let mut words: Vec<(f64, f64, &str)> = list_of(l, "words")
            .iter()
            .filter_map(|w| Some((num_of(w, "start")?, num_of(w, "end")?, str_of(w, "text"))))
            .filter(|(_, _, text)| !text.is_empty())
            .collect();
        if words.is_empty() {
            let (start, end) = (num_of(l, "start")?, num_of(l, "end")?);
            if end <= start {
                return None;
            }
            words.push((start, end, str_of(l, "text")));
        }
        let start = words[0].0;
        let end = words.iter().map(|w| w.1).fold(f64::MIN, f64::max);
        let body: String = words
            .iter()
            .map(|(s, e, t)| format!("({s},{},0){t}", (e - s).max(1.0)))
            .collect();
        Some(format!("[{start},{}]{body}", end - start))
    };
    lines.iter().filter_map(line).collect::<Vec<_>>().join("\n")
}

fn to_lrc(lines: &[Value]) -> String {
    lines
        .iter()
        .filter_map(|l| Some((num_of(l, "start")?, str_of(l, "text"))))
        .filter(|(_, text)| !text.is_empty())
        .map(|(start, text)| format!("[{}]{text}", lrc_time(start)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn find(artist: &str, track: &str, album: Option<&str>, duration: Option<f64>) -> Found {
    let mut query = vec![
        ("artist", artist.to_string()),
        ("title", track.to_string()),
        ("level", "word".into()),
        ("format", "json".into()),
    ];
    if let Some(a) = album {
        query.push(("album", a.into()));
    }
    if let Some(d) = duration {
        query.push(("duration", d.round().to_string()));
    }
    let res = client()
        .get("https://api.lrcmux.dev/get")
        .query(&query)
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None); // 404: nothing found
    }
    let body: Value = res.json().await?;
    let lines = list_of(&body, "lines");
    if lines.is_empty()
        || body
            .pointer("/meta/instrumental")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Ok(None);
    }
    if let (Some(d), Some(theirs)) = (
        duration,
        body.pointer("/track/duration").and_then(Value::as_f64),
    ) {
        if (theirs - d).abs() > MAX_MISMATCH_S {
            eprintln!(
                "[lyrics:lrcmux] skipped: {theirs}s recording, playing {}s",
                d.round()
            );
            return Ok(None);
        }
    }

    let level = body
        .pointer("/meta/level")
        .and_then(Value::as_str)
        .unwrap_or("");
    if level == "word" {
        let yrc = to_yrc(lines);
        if !yrc.is_empty() {
            return Ok(Some(json!({ "wordLyrics": yrc, "source": SOURCE })));
        }
    }
    if level == "word" || level == "line" {
        let lrc = to_lrc(lines);
        if !lrc.is_empty() {
            return Ok(Some(json!({ "syncedLyrics": lrc, "source": SOURCE })));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_become_yrc() {
        let lines = [
            json!({ "start": 1000, "end": 2000, "text": "a b", "words": [
                { "text": "a ", "start": 1000, "end": 1400 },
                { "text": "b", "start": 1400, "end": 2000 }
            ]}),
            json!({ "start": 3000, "end": 4000, "text": "whole line", "words": [] }),
            json!({ "start": 5000, "end": 5000, "text": "no length" }),
        ];
        assert_eq!(
            to_yrc(&lines),
            "[1000,1000](1000,400,0)a (1400,600,0)b\n[3000,1000](3000,1000,0)whole line"
        );
    }

    #[test]
    fn lines_become_lrc() {
        let lines = [
            json!({ "start": 83_450, "text": "hi" }),
            json!({ "start": 90_000, "text": "" }),
        ];
        assert_eq!(to_lrc(&lines), "[01:23.45]hi");
        assert_eq!(lrc_time(0.0), "00:00.00");
    }
}
