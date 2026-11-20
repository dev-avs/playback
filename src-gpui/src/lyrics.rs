use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct LyricsResult {
    pub source: String,
    pub synced: bool,
    pub lines: Vec<LyricLine>,
}

/// Parses standard LRC timestamps (`[mm:ss.xx]` / `[mm:ss.xxx]`), supporting
/// multiple timestamps per line and sorting chronologically.
pub fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let mut rest = line.trim();
        let mut stamps = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(secs) = parse_stamp(tag) {
                stamps.push(secs);
            }
            rest = rest[end + 1..].trim_start();
        }
        if stamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for time in stamps {
            out.push(LyricLine {
                time,
                text: text.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

fn parse_stamp(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let mins: f64 = m.trim().parse().ok()?;
    let secs: f64 = s.trim().parse().ok()?;
    Some(mins * 60.0 + secs)
}

pub fn active_line_index(lines: &[LyricLine], position_s: f64) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let mut best = None;
    for (i, line) in lines.iter().enumerate() {
        if line.time <= position_s + 0.05 {
            best = Some(i);
        } else {
            break;
        }
    }
    best
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibHit {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

/// Fetches synced (or plain) lyrics from LRCLIB for the current track.
pub async fn fetch_lyrics(title: &str, artist: &str, duration_s: f64) -> Option<LyricsResult> {
    if title.trim().is_empty() {
        return None;
    }
    let client = reqwest::Client::builder()
        .user_agent("Playback/1.5 (GPUI)")
        .build()
        .ok()?;
    let mut params = vec![("track_name", title.to_string())];
    if !artist.trim().is_empty() {
        params.push(("artist_name", artist.to_string()));
    }
    if duration_s > 1.0 {
        params.push(("duration", (duration_s.round() as u64).to_string()));
    }
    let res = client
        .get("https://lrclib.net/api/get")
        .query(&params)
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    let hit: LrclibHit = res.json().await.ok()?;
    if let Some(synced) = hit.synced_lyrics.filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(&synced);
        if !lines.is_empty() {
            return Some(LyricsResult {
                source: "lrclib".into(),
                synced: true,
                lines,
            });
        }
    }
    if let Some(plain) = hit.plain_lyrics.filter(|s| !s.trim().is_empty()) {
        let lines = plain
            .lines()
            .map(|l| LyricLine {
                time: 0.0,
                text: l.trim().to_string(),
            })
            .filter(|l| !l.text.is_empty())
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            return Some(LyricsResult {
                source: "lrclib".into(),
                synced: false,
                lines,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{active_line_index, parse_lrc};

    #[test]
    fn parses_multi_timestamp_lrc() {
        let lines = parse_lrc("[00:10.50][01:02.00]Hello world\n[00:15.00]Next");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].time, 10.5);
        assert_eq!(lines[1].text, "Next");
        assert_eq!(lines[2].time, 62.0);
    }

    #[test]
    fn finds_active_line() {
        let lines = parse_lrc("[00:05.00]A\n[00:10.00]B\n[00:15.00]C");
        assert_eq!(active_line_index(&lines, 2.0), None);
        assert_eq!(active_line_index(&lines, 10.2), Some(1));
        assert_eq!(active_line_index(&lines, 99.0), Some(2));
    }
}
