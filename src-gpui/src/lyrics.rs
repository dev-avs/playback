use crate::romanize::romanize_line;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct LyricWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub time: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<LyricWord>,
    pub roma: Option<String>,
    pub is_gap: bool,
    pub is_backing: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LyricsResult {
    pub source: String,
    /// 3 = word/syllable, 2 = line-synced, 1 = plain
    pub tier: u8,
    pub synced: bool,
    pub lines: Vec<LyricLine>,
}

impl LyricsResult {
    pub fn tier_dots(&self) -> &'static str {
        match self.tier {
            3 => "●●●",
            2 => "●●",
            _ => "●",
        }
    }
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
        let is_gap = text.is_empty();
        let is_backing = text.starts_with('(') && text.ends_with(')') && text.len() > 2;
        let roma = romanize_line(&text);
        for time in stamps {
            out.push(LyricLine {
                time,
                end: time + 4.0,
                text: text.clone(),
                words: Vec::new(),
                roma: roma.clone(),
                is_gap,
                is_backing,
            });
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    for i in 0..out.len().saturating_sub(1) {
        out[i].end = out[i + 1].time;
    }
    out
}

fn parse_stamp(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    let mins: f64 = m.trim().parse().ok()?;
    let secs: f64 = s.trim().parse().ok()?;
    Some(mins * 60.0 + secs)
}

/// Parses a Spicy Lyrics JSON payload captured from the Spicetify bridge.
pub fn parse_spicy_payload(val: &Value) -> Option<LyricsResult> {
    let kind = val["Type"].as_str().unwrap_or("");
    let content = val["Content"].as_array()?;
    let mut lines = Vec::new();
    let mut has_words = false;

    for item in content {
        let item_type = item["Type"].as_str().unwrap_or("");
        if item_type == "Gap" {
            let start = item["StartTime"].as_f64().unwrap_or(0.0);
            let end = item["EndTime"].as_f64().unwrap_or(start + 2.0);
            lines.push(LyricLine {
                time: start,
                end,
                text: String::new(),
                words: Vec::new(),
                roma: None,
                is_gap: true,
                is_backing: false,
            });
            continue;
        }

        // Syllable-timed Vocal block
        if let Some(lead) = item.get("Lead").and_then(|v| v.as_object()) {
            let start = lead
                .get("StartTime")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let end = lead
                .get("EndTime")
                .and_then(|v| v.as_f64())
                .unwrap_or(start + 3.0);
            let mut words = Vec::new();
            let mut full = String::new();
            let mut roma_parts = Vec::new();
            if let Some(sylls) = lead.get("Syllables").and_then(|v| v.as_array()) {
                for s in sylls {
                    let w_text = s["Text"].as_str().unwrap_or("").to_string();
                    let w_start = s["StartTime"].as_f64().unwrap_or(start);
                    let w_end = s["EndTime"].as_f64().unwrap_or(end);
                    let part_of_word = s["IsPartOfWord"].as_bool().unwrap_or(false);
                    if !full.is_empty() && !part_of_word {
                        full.push(' ');
                    }
                    full.push_str(&w_text);
                    if let Some(tr) = s["TransliteratedText"].as_str().filter(|t| !t.is_empty()) {
                        roma_parts.push(tr.to_string());
                    }
                    words.push(LyricWord {
                        text: if part_of_word {
                            w_text
                        } else {
                            format!("{w_text} ")
                        },
                        start: w_start,
                        end: w_end,
                    });
                }
            }
            if !words.is_empty() {
                has_words = true;
            }
            let text = full.trim().to_string();
            let roma = if !roma_parts.is_empty() {
                let joined = roma_parts.join(" ");
                (joined.trim() != text).then_some(joined)
            } else {
                romanize_line(&text)
            };
            if !text.is_empty() {
                lines.push(LyricLine {
                    time: start,
                    end,
                    text,
                    words,
                    roma,
                    is_gap: false,
                    is_backing: false,
                });
            }
            continue;
        }

        // Line-timed item
        if let Some(text) = item["Text"].as_str() {
            let start = item["StartTime"].as_f64().unwrap_or(0.0);
            let end = item["EndTime"].as_f64().unwrap_or(start + 3.0);
            let trimmed = text.trim().to_string();
            let roma = item["TransliteratedText"]
                .as_str()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s != &trimmed)
                .or_else(|| romanize_line(&trimmed));
            if !trimmed.is_empty() {
                lines.push(LyricLine {
                    time: start,
                    end,
                    text: trimmed,
                    words: Vec::new(),
                    roma,
                    is_gap: false,
                    is_backing: false,
                });
            }
        }
    }

    if lines.is_empty() {
        return None;
    }
    let tier = if has_words || kind == "Syllable" {
        3
    } else if kind == "Line" {
        2
    } else {
        1
    };
    Some(LyricsResult {
        source: "spicy".into(),
        tier,
        synced: tier >= 2,
        lines,
    })
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

async fn fetch_lrclib(title: &str, artist: &str, duration_s: f64) -> Option<LyricsResult> {
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
                tier: 2,
                synced: true,
                lines,
            });
        }
    }
    if let Some(plain) = hit.plain_lyrics.filter(|s| !s.trim().is_empty()) {
        let lines = plain
            .lines()
            .map(|l| {
                let text = l.trim().to_string();
                let roma = romanize_line(&text);
                LyricLine {
                    time: 0.0,
                    end: 0.0,
                    text,
                    words: Vec::new(),
                    roma,
                    is_gap: false,
                    is_backing: false,
                }
            })
            .filter(|l| !l.text.is_empty())
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            return Some(LyricsResult {
                source: "lrclib".into(),
                tier: 1,
                synced: false,
                lines,
            });
        }
    }
    None
}

/// Fetches lyrics using the requested provider priority (`auto`, `spicy`, `lrclib`).
pub async fn fetch_lyrics(
    title: &str,
    artist: &str,
    duration_s: f64,
    provider: &str,
    spicy_payload: Option<Value>,
) -> Option<LyricsResult> {
    let spicy = spicy_payload.and_then(|v| parse_spicy_payload(&v));
    match provider {
        "spicy" => spicy.or(fetch_lrclib(title, artist, duration_s).await),
        "lrclib" => fetch_lrclib(title, artist, duration_s).await.or(spicy),
        _ => {
            if let Some(ref s) = spicy {
                if s.tier >= 3 {
                    return spicy;
                }
            }
            let lrc = fetch_lrclib(title, artist, duration_s).await;
            match (spicy, lrc) {
                (Some(s), Some(l)) => Some(if s.tier >= l.tier { s } else { l }),
                (Some(s), None) => Some(s),
                (None, Some(l)) => Some(l),
                (None, None) => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{active_line_index, parse_lrc, parse_spicy_payload};
    use serde_json::json;

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

    #[test]
    fn parses_spicy_syllables() {
        let payload = json!({
            "Type": "Syllable",
            "Content": [
                {
                    "Type": "Vocal",
                    "Lead": {
                        "StartTime": 1.0,
                        "EndTime": 3.0,
                        "Syllables": [
                            { "Text": "Hello", "StartTime": 1.0, "EndTime": 1.8, "IsPartOfWord": false },
                            { "Text": "world", "StartTime": 1.8, "EndTime": 3.0, "IsPartOfWord": false }
                        ]
                    }
                }
            ]
        });
        let res = parse_spicy_payload(&payload).unwrap();
        assert_eq!(res.source, "spicy");
        assert_eq!(res.tier, 3);
        assert_eq!(res.lines[0].text, "Hello world");
        assert_eq!(res.lines[0].words.len(), 2);
    }
}
