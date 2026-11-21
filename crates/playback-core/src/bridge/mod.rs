//! The Spicetify bridge: what the extension inside Spotify
//! (spicetify/player-bridge.js) tells the app, over a loopback server
//! (server). Ported from electron/tokenBridge.cjs and the handlers in
//! electron/main.cjs.
//!
//! It pushes the client's own Spotify token (with its real expiry), the
//! track it plays and what is queued next (track), lyrics Spicy Lyrics
//! already fetched, and the playback position. For a song another app
//! plays, it searches Spotify on the app's behalf (lookup), in a Spotify
//! started hidden for the purpose (quick_open).

mod lookup;
mod quick_open;
mod server;
mod track;

use crate::support::http::client;
use crate::support::paths::data_file;
use crate::host::Host;
use crate::{spicy, spotify};
use lookup::{Candidate, IdCache, Wanted};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use track::Track;

pub use server::start;

/// A token this close to running out is renewed now rather than mid-song.
const TOKEN_MARGIN_MS: f64 = 5.0 * 60_000.0;

#[derive(Default)]
struct State {
    track: Option<Track>,
    /// the one song the extension is asked to find
    pending: Option<Wanted>,
    /// searched this run: a failed search waits for a restart
    looked_up: HashSet<String>,
    /// queued songs whose lyrics were already fetched ahead
    preloaded: HashSet<String>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);
static IDS: LazyLock<IdCache> = LazyLock::new(|| IdCache::open(data_file("spotify-ids.json")));

fn now() -> f64 {
    crate::spotify::now_ms()
}

/// The song the page asks about, as src/player.d.ts sends it.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    pub duration: Option<f64>,
}

fn spotify_playing(app: &dyn Host) -> bool {
    app.media_latest()
        .as_ref()
        .and_then(|s| s["appId"].as_str())
        .is_some_and(|id| id.to_lowercase().contains("spotify"))
}

fn token_fresh() -> bool {
    spotify::signed_in() && spotify::token_expiry().is_none_or(|e| e - now() > TOKEN_MARGIN_MS)
}

fn bridged_id(title: &str) -> Option<String> {
    STATE
        .lock()
        .unwrap()
        .track
        .as_ref()?
        .id_for(title, now())
        .map(String::from)
}

/// The song's Spotify id: from the bridge, else from an earlier search.
fn spotify_id(song: &Song) -> Option<String> {
    bridged_id(&song.title).or_else(|| IDS.get(&song.artist, &song.title, now()).flatten())
}

/// Have a hidden Spotify supply what this song lacks: its id, a fresh
/// token, or both. Spotify playing it itself needs nothing.
fn request_from_spotify(app: &dyn Host, song: &Song) {
    if spotify_playing(app) || song.title.is_empty() {
        return;
    }
    let known = spotify_id(song).is_some();
    let lookup_key = format!("{}|{}", song.artist, song.title);
    let need_id = {
        let mut s = STATE.lock().unwrap();
        let need = !known
            && IDS.get(&song.artist, &song.title, now()).is_none()
            && !s.looked_up.contains(&lookup_key);
        if need {
            s.looked_up.insert(lookup_key);
            s.pending = Some(Wanted {
                title: song.title.clone(),
                artist: song.artist.clone(),
                duration: song.duration,
            });
        }
        need
    };
    if need_id || (known && !token_fresh()) {
        quick_open::open();
    }
}

/// A Spotify opened for us closes once it handed over all it was asked for.
fn close_quick_open_when_done(app: &dyn Host) {
    if !quick_open::is_open() {
        return;
    }
    if spotify_playing(app) {
        return quick_open::release(); // the user took it over
    }
    if STATE.lock().unwrap().pending.is_none() && token_fresh() {
        quick_open::close();
    }
}

/// fetchSpicyLyrics in src/player.d.ts.
pub async fn spicy_lyrics(app: &dyn Host, song: Song) -> Value {
    let id = spotify_id(&song);
    if id.is_none() || !token_fresh() {
        request_from_spotify(app, &song);
    }
    spicy::resolve(id.as_deref()).await
}

/// getNext in src/player.d.ts: what Spotify plays after this song.
pub fn next(title: &str) -> Option<Value> {
    STATE.lock().unwrap().track.as_ref()?.next_for(title, now())
}

/// The cover Spotify itself shows, which beats any search. The media
/// session usually names the song a moment before the push lands, so for
/// Spotify's own songs the push gets two seconds to catch up.
pub async fn art(title: &str, app_id: &str) -> Option<String> {
    let tries = if app_id.to_lowercase().contains("spotify") {
        10
    } else {
        1
    };
    for _ in 0..tries {
        let found = {
            let s = STATE.lock().unwrap();
            let t = s.track.as_ref().filter(|t| t.fresh(now()))?;
            if track::loose_match(t.name.as_deref(), title) {
                Some(t.image.clone().ok_or_else(|| t.id.clone()))
            } else {
                // the queued song just started: its cover is already known
                t.next
                    .as_ref()
                    .filter(|n| track::loose_match(n.name.as_deref(), title))
                    .and_then(|n| n.image.clone())
                    .map(Ok)
            }
        };
        match found {
            Some(Ok(image)) => return Some(image),
            Some(Err(id)) => return oembed_art(&id).await,
            None => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
    None
}

/// Spotify's cover for a track id, from its public oEmbed endpoint.
async fn oembed_art(id: &str) -> Option<String> {
    let url = format!("https://open.spotify.com/oembed?url=spotify:track:{id}");
    let res = client()
        .get(url)
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .ok()?;
    let body: Value = res.error_for_status().ok()?.json().await.ok()?;
    // oEmbed hands out 300px; the 640px one differs only in this code
    Some(
        body["thumbnail_url"]
            .as_str()?
            .replace("ab67616d00001e02", "ab67616d0000b273"),
    )
}

/// Fetches the queued song's lyrics into the cache ahead, once per run.
fn preload(next: Option<&track::Next>) {
    let Some(n) = next else { return };
    if !STATE.lock().unwrap().preloaded.insert(n.id.clone()) {
        return;
    }
    let (id, name) = (n.id.clone(), n.name.clone().unwrap_or_default());
    crate::rt::spawn(async move {
        let r = spicy::resolve(Some(&id)).await;
        eprintln!("[preload] {name}: {}", r["status"].as_str().unwrap_or("?"));
    });
}

// ── what the extension sends (server routes) ───────────────────────

/// POST /token: `{ token, expiresAt, track }`.
fn on_token(app: &dyn Host, payload: &Value) -> spotify::Saved {
    if let Ok(mut t) = serde_json::from_value::<Track>(payload["track"].clone()) {
        t.at = now();
        on_track(app, t);
    }
    let before = spotify::signed_in();
    let saved = spotify::hand_over(
        payload["token"].as_str().unwrap_or(""),
        payload["expiresAt"].as_f64(),
    );
    if saved.ok && !before {
        eprintln!("[bridge] token accepted, Spicy Lyrics is reachable again");
        let _ = app.emit("spotify:tokenArrived", Value::Null);
    } else if !saved.ok {
        eprintln!(
            "[bridge] token rejected: {}",
            saved.error.as_deref().unwrap_or("?")
        );
    }
    close_quick_open_when_done(app);
    saved
}

fn on_track(app: &dyn Host, t: Track) {
    let (changed, next_changed) = {
        let mut s = STATE.lock().unwrap();
        let before = s.track.as_ref();
        let changed = before.is_none_or(|b| b.id != t.id);
        let next_changed = before.map(|b| &b.next) != Some(&t.next);
        s.track = Some(t.clone());
        (changed, next_changed)
    };
    if changed || next_changed {
        preload(t.next.as_ref());
        // the queue changes on its own too, and the "Next" hint follows it
        let _ = app.emit("bridge:track", Value::Null);
    }
    if changed {
        eprintln!(
            "[bridge] now playing {} ({})",
            t.name.as_deref().unwrap_or("?"),
            t.id
        );
        // diagnostics only: the current id, readable without a debugger
        let _ = std::fs::write(
            data_file("bridged-track.json"),
            serde_json::to_string(&t).unwrap_or_default(),
        );
        // The page asked on the session's change, before this id arrived; if
        // lyrics are on hand now, it looks again.
        if spicy::on_hand(&t.id) {
            let _ = app.emit("spicy:lyricsArrived", Value::Null);
        }
    }
}

/// POST /spicy: `{ track, lyrics }`, lyrics Spicy Lyrics had in Spotify.
fn on_spicy(app: &dyn Host, payload: &Value) -> Value {
    let id = payload["track"]["id"].as_str().filter(|id| !id.is_empty());
    let lyrics = &payload["lyrics"];
    let Some(id) = id.filter(|_| !lyrics.is_null()) else {
        return json!({ "ok": false, "error": "no track id or lyrics" });
    };
    eprintln!(
        "[bridge] {} lyrics for {} ({id})",
        lyrics["Type"].as_str().unwrap_or("?"),
        payload["track"]["name"].as_str().unwrap_or("?")
    );
    spicy::remember(id, lyrics.clone());
    // the page may show this very song on a coarser source
    let _ = app.emit("spicy:lyricsArrived", Value::Null);
    json!({ "ok": true })
}

/// GET /lookup: the song the extension should find, if any.
fn pending() -> Value {
    let s = STATE.lock().unwrap();
    json!({ "lookups": s.pending.iter().collect::<Vec<_>>() })
}

/// POST /resolved: `{ title, artist, candidates, error? }`.
fn on_resolved(app: &dyn Host, payload: &Value) {
    let wanted = {
        let mut s = STATE.lock().unwrap();
        let matches = s.pending.as_ref().is_some_and(|w| {
            payload["title"] == w.title.as_str() && payload["artist"] == w.artist.as_str()
        });
        if !matches {
            return;
        }
        s.pending.take().expect("checked above")
    };
    if let Some(error) = payload["error"].as_str() {
        eprintln!("[lookup] search failed for {}: {error}", wanted.title);
    } else {
        let candidates: Vec<Candidate> =
            serde_json::from_value(payload["candidates"].clone()).unwrap_or_default();
        let id = lookup::pick(&wanted, &candidates);
        IDS.set(&wanted.artist, &wanted.title, id.clone(), now());
        eprintln!(
            "[lookup] {} - {} -> {}",
            wanted.artist,
            wanted.title,
            id.as_deref().unwrap_or("not on Spotify")
        );
        if id.is_some() {
            let _ = app.emit("spicy:lyricsArrived", Value::Null);
        }
    }
    close_quick_open_when_done(app);
}

/// POST /progress: Spotify's own clock, about once a second, which beats
/// the media session's when that drifts mid-song.
fn on_progress(app: &dyn Host, payload: Value) {
    let _ = app.emit_widget("bridge:progress", payload);
}

/// Called on quit: a hidden Spotify never outlives the app.
pub fn shut_down() {
    quick_open::close();
}
