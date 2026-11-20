//! Spicy Lyrics: syllable-timed lyrics by Spotify track id. Ported from
//! electron/spicyResolver.cjs.
//!
//! The finest copy on hand wins and needs no request. Otherwise the user's
//! own key is tried first (api, public route), then the Spotify token
//! (token route); either way a coarser answer never replaces a finer copy,
//! and a failure falls back to whatever copy there is.
//!
//! Copies come from the disk cache (cache) and from Spotify itself: the
//! Spicetify bridge hands over what Spicy Lyrics already fetched in the
//! client (`remember`).

mod api;
mod cache;
pub mod connect;
pub mod key;

use crate::support::cache::Cache;
use api::timing_tier;
use cache::DiskCache;
pub use cache::Entry;
use serde_json::{json, Value};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

/// Stored as an API answer's client version, so it is not asked for again
/// on every play.
const API_VERSION: &str = "v1";

static DISK: LazyLock<DiskCache> = LazyLock::new(DiskCache::standard);
/// What the bridge handed over this run, in front of the disk copy.
static FROM_CLIENT: LazyLock<Cache<Entry>> = LazyLock::new(|| Cache::new(40));
/// Set from a 429's Retry-After; until then the key route is skipped.
static API_BLOCKED_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// Lyrics the bridge captured inside Spotify: the packed array Spicy's API
/// returns, or the unpacked object Spicy Lyrics caches.
pub fn remember(track_id: &str, lyrics: Value) {
    let kind = if lyrics.is_array() {
        "packed"
    } else {
        "lyrics"
    };
    let entry = Entry {
        kind: kind.into(),
        data: lyrics,
        client_version: None,
    };
    DISK.put(track_id, &entry);
    FROM_CLIENT.put(track_id.to_string(), entry);
}

/// Whether lyrics for this track are already here, without asking anyone.
pub fn on_hand(track_id: &str) -> bool {
    FROM_CLIENT.get(track_id).is_some() || DISK.get(track_id).is_some()
}

fn answer(entry: &Entry, track_id: &str) -> Value {
    json!({ "status": "ok", entry.kind.as_str(): entry.data, "trackId": track_id })
}

/// The response in src/player.d.ts (fetchSpicyLyrics).
pub async fn resolve(track_id: Option<&str>) -> Value {
    let Some(id) = track_id else {
        return json!({ "status": "no-track" });
    };
    let stored = DISK.get(id);
    let bridged = FROM_CLIENT.get(id);
    // the disk copy on a tie: it may already be checked with today's version
    let tier = |e: &Option<Entry>| e.as_ref().map_or(0, |e| timing_tier(&e.data));
    let cached = if tier(&bridged) > tier(&stored) {
        bridged
    } else {
        stored.or(bridged)
    };
    let cached_tier = tier(&cached);
    let fallback = |response: Value| cached.as_ref().map_or(response, |c| answer(c, id));

    if cached_tier == 2 {
        return answer(cached.as_ref().unwrap(), id);
    }

    let blocked = API_BLOCKED_UNTIL
        .lock()
        .unwrap()
        .is_some_and(|until| Instant::now() < until);
    if let Some(key) = key::get().filter(|_| !blocked) {
        if let Some(c) = cached
            .as_ref()
            .filter(|c| c.client_version.as_deref() == Some(API_VERSION))
        {
            return answer(c, id);
        }
        match api::get_lyrics(id, &key).await {
            Ok(r) if r.http_status == 200 && r.data.is_some() => {
                let data = r.data.unwrap();
                let tier = timing_tier(&data);
                if cached.is_some() && tier < cached_tier {
                    return fallback(Value::Null);
                }
                let entry = Entry {
                    kind: "lyrics".into(),
                    data,
                    client_version: Some(API_VERSION.into()),
                };
                if tier > 0 {
                    DISK.put(id, &entry);
                }
                return answer(&entry, id);
            }
            // the token route asks the same catalogue, so it has nothing either
            Ok(r) if r.http_status == 404 => {
                return fallback(json!({ "status": "not-found", "httpStatus": 404 }))
            }
            Ok(r) => {
                if let Some(s) = r.retry_after {
                    *API_BLOCKED_UNTIL.lock().unwrap() =
                        Some(Instant::now() + std::time::Duration::from_secs(s));
                }
                let why = r.error.map(|e| format!(" ({e})")).unwrap_or_default();
                eprintln!(
                    "[spicy] API answered {}{why}; trying the bridge",
                    r.http_status
                );
            }
            Err(e) => eprintln!("[spicy] API unreachable; trying the bridge: {e}"),
        }
    }

    let Some(token) = crate::spotify::access_token().await else {
        return fallback(json!({ "status": "no-auth" }));
    };
    let version = api::client_version(&token, false).await;
    if let Some(c) = cached
        .as_ref()
        .filter(|c| c.client_version.as_deref() == Some(version.as_str()))
    {
        return answer(c, id);
    }
    match api::query_lyrics(id, &token).await {
        Ok(Some(r)) if r.http_status == 200 => {
            let tier = timing_tier(&r.data);
            if cached.is_some() && tier < cached_tier {
                return fallback(Value::Null);
            }
            let entry = Entry {
                kind: "packed".into(),
                data: r.data,
                client_version: Some(r.client_version),
            };
            if tier > 0 {
                DISK.put(id, &entry);
            }
            answer(&entry, id)
        }
        Ok(Some(r)) => {
            let status = match r.http_status {
                404 => "not-found",
                503 => "queued",
                _ => "error",
            };
            fallback(json!({ "status": status, "httpStatus": r.http_status }))
        }
        Ok(None) => fallback(json!({ "status": "error" })),
        Err(e) => {
            eprintln!("[spicy] {e}");
            fallback(json!({ "status": "error", "error": e }))
        }
    }
}
