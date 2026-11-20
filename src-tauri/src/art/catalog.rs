//! Covers from music catalogues: iTunes, then Deezer. Both give 600 px or
//! more with no API key.

use crate::support::http::client;
use crate::support::json::{list_of, str_of};
use crate::support::text::loose_match;
use serde_json::Value;

/// iTunes' first result is often another artist's album of the same name, and
/// a wrong cover is worse than none, so the artist has to match.
async fn itunes(artist: &str, title: &str, album: &str) -> Result<Option<String>, reqwest::Error> {
    let term = format!("{artist} {}", if album.is_empty() { title } else { album });
    let entity = if album.is_empty() { "song" } else { "album" };
    let res = client()
        .get("https://itunes.apple.com/search")
        .query(&[("term", term.trim()), ("entity", entity), ("limit", "5")])
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let body: Value = res.json().await?;
    let hit = list_of(&body, "results").iter().find(|r| loose_match(str_of(r, "artistName"), artist));
    let art = hit.map(|h| match str_of(h, "artworkUrl100") {
        "" => str_of(h, "artworkUrl60"),
        url => url,
    });
    // the size is just a path segment: ask for one worth blurring
    Ok(art.filter(|a| !a.is_empty()).map(|a| resize_itunes(a, 600)))
}

fn resize_itunes(url: &str, px: u32) -> String {
    static SIZE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"/\d+x\d+bb\.").unwrap());
    SIZE.replace(url, format!("/{px}x{px}bb.")).into_owned()
}

async fn deezer(artist: &str, title: &str) -> Result<Option<String>, reqwest::Error> {
    let q = format!("artist:\"{artist}\" track:\"{title}\"");
    let res = client().get("https://api.deezer.com/search").query(&[("q", q.as_str()), ("limit", "1")]).send().await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let body: Value = res.json().await?;
    let album = &body["data"][0]["album"];
    let url = [str_of(album, "cover_xl"), str_of(album, "cover_big")].into_iter().find(|u| !u.is_empty());
    Ok(url.map(String::from))
}

pub async fn find(artist: &str, title: &str, album: &str) -> Option<String> {
    match itunes(artist, title, album).await {
        Ok(Some(url)) => return Some(url),
        Ok(None) => {}
        Err(e) => eprintln!("[art:itunes] {e}"),
    }
    deezer(artist, title).await.unwrap_or_else(|e| {
        eprintln!("[art:deezer] {e}");
        None
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn asks_itunes_for_a_bigger_size() {
        assert_eq!(
            super::resize_itunes("https://is1.mzstatic.com/image/a/100x100bb.jpg", 600),
            "https://is1.mzstatic.com/image/a/600x600bb.jpg"
        );
    }
}
