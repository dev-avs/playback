//! The cover for what is playing.
//!
//! For a music app: a catalogue's big cover first, the session's own thumbnail
//! as a fallback. For anything else (a video in the browser): the session's
//! thumbnail first, since that is the picture the app shows, swapped for
//! YouTube's full-size image when it is a YouTube video.

mod catalog;
mod thumbnail;
mod youtube;

use crate::media::is_music_app;
use crate::support::cache::Cache;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    artist: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    album: String,
    #[serde(default, rename = "appId")]
    app_id: String,
}

static FOUND: LazyLock<Cache<String>> = LazyLock::new(|| Cache::new(200));
/// covers already turned into data: URLs, by their URL
static INLINED: LazyLock<Cache<String>> = LazyLock::new(|| Cache::new(16));

/// The cover at `url` as a data: URL, so a window that opens later (Settings,
/// the update prompt) shows it at once instead of downloading it again: each
/// WebView2 window fetches an http cover by itself, which took seconds. Falls
/// back to `url` when the download fails.
pub async fn inline(url: String) -> String {
    if !url.starts_with("http") {
        return url;
    }
    if let Some(data) = INLINED.get(&url) {
        return data;
    }
    let got = async {
        let res = crate::support::http::client()
            .get(&url)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let kind = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .filter(|k| k.starts_with("image/"))
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = res.bytes().await.ok()?;
        use base64::Engine;
        Some(format!(
            "data:{kind};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ))
    }
    .await;
    match got {
        Some(data) => {
            INLINED.put(url, data.clone());
            data
        }
        None => url,
    }
}

async fn video(app_id: &str, artist: &str, title: &str) -> Option<String> {
    let thumb = thumbnail::find(app_id, title).await?;
    match youtube::find(title, artist).await {
        Ok(Some(url)) => Some(url),
        Ok(None) => Some(thumb),
        Err(e) => {
            eprintln!("[art:youtube] {e}");
            Some(thumb)
        }
    }
}

/// A URL (or data: URL) for the cover, or None. A miss is not remembered: it
/// may only have been offline, and the widget asks again.
pub async fn fetch(req: Request) -> Option<String> {
    let Request {
        artist,
        title,
        album,
        app_id,
    } = &req;
    if artist.is_empty() && title.is_empty() {
        return None;
    }
    // Spotify's own cover, from the bridge, beats any search
    if let Some(url) = crate::bridge::art(title, app_id).await {
        return Some(url);
    }
    let key = format!("{app_id}|{artist}|{album}|{title}");
    if let Some(url) = FOUND.get(&key) {
        return Some(url);
    }

    let url = if is_music_app(app_id) {
        match catalog::find(artist, title, album).await {
            Some(url) => Some(url),
            None => thumbnail::find(app_id, title).await,
        }
    } else {
        match video(app_id, artist, title).await {
            Some(url) => Some(url),
            None => catalog::find(artist, title, album).await,
        }
    };

    match &url {
        Some(u) => FOUND.put(key, u.clone()),
        None => eprintln!("[art] no artwork found for {key}"),
    }
    url
}
