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
    let Request { artist, title, album, app_id } = &req;
    if artist.is_empty() && title.is_empty() {
        return None;
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
