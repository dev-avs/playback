//! A YouTube video's full-size image, in place of the browser's tiny session
//! thumbnail. Only an exact title from the same channel counts (the session
//! reports the channel as the artist), so a video from another site is never
//! matched to a lookalike.

use crate::support::http::client;
use crate::support::text::{loose_match, same_text};
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""title":\{"runs":\[\{"text":"((?:[^"\\]|\\.)*)""#).unwrap());
static OWNER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""ownerText":\{"runs":\[\{"text":"((?:[^"\\]|\\.)*)""#).unwrap());

/// A JSON string body as text ("&" and the like), or as it was.
fn unescape(s: &str) -> String {
    serde_json::from_str(&format!("\"{s}\"")).unwrap_or_else(|_| s.to_string())
}

fn capture(re: &Regex, chunk: &str) -> Option<String> {
    re.captures(chunk).map(|c| unescape(&c[1]))
}

pub async fn find(title: &str, channel: &str) -> Result<Option<String>, reqwest::Error> {
    let res = client()
        .get("https://www.youtube.com/results")
        .query(&[("search_query", format!("{title} {channel}"))])
        .header("Accept-Language", "en")
        .header("Cookie", "CONSENT=YES+")
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let html = res.text().await?;
    for chunk in html.split(r#""videoRenderer":{"videoId":""#).skip(1).take(7) {
        let Some(id) = chunk.get(..11) else { continue };
        if !capture(&TITLE, chunk).is_some_and(|t| same_text(&t, title)) {
            continue;
        }
        if !channel.is_empty() && !loose_match(&capture(&OWNER, chunk).unwrap_or_default(), channel) {
            continue;
        }
        // 1280 px when the uploader made one; 320 px (twice the thumbnail,
        // without the black bars of the 480 px size) otherwise
        let max = format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg");
        let has_max = client()
            .head(&max)
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success());
        return Ok(Some(if has_max { max } else { format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg") }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unescapes_json_text() {
        assert_eq!(super::unescape(r"Rock & Roll"), "Rock & Roll");
        assert_eq!(super::unescape(r"bad \x"), r"bad \x");
    }
}
