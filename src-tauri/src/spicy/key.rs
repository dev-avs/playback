//! The user's own Spicy Lyrics API key, ported from electron/spicyKey.cjs.
//!
//! Kept in `spicy-key.json` as `{ encrypted }`: sealed the way Electron's
//! safeStorage seals it (support/os_crypt), so the key survives the switch
//! between builds. Without Electron's profile key it is sealed with DPAPI
//! alone (`{ dpapi }`), which only this build reads. Never shown back: the
//! page only learns whether one is set and how it ends.

use super::api;
use crate::support::os_crypt;
use crate::support::paths::data_file;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};

const FILE: &str = "spicy-key.json";
/// A track every key can read, to check one with.
const PROBE_TRACK: &str = "1QV6tiMFM6fSOKOGLMHYYg";

static KEY: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(load()));

#[derive(Deserialize, Serialize, Default)]
struct Saved {
    #[serde(skip_serializing_if = "Option::is_none")]
    encrypted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dpapi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

fn load() -> Option<String> {
    let saved: Saved = serde_json::from_str(&std::fs::read_to_string(data_file(FILE)).ok()?).ok()?;
    let opened = |b64: &Option<String>, open: fn(&[u8]) -> Option<String>| open(&B64.decode(b64.as_deref()?).ok()?);
    opened(&saved.encrypted, os_crypt::open)
        .or_else(|| opened(&saved.dpapi, |b| String::from_utf8(os_crypt::dpapi_open(b)?).ok()))
        .or(saved.key)
}

fn store(next: Option<&str>) {
    *KEY.lock().unwrap() = next.map(String::from);
    let path = data_file(FILE);
    let written = match next {
        None => std::fs::remove_file(&path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
        Some(key) => {
            let saved = match os_crypt::seal(key) {
                Some(sealed) => Saved { encrypted: Some(B64.encode(sealed)), ..Saved::default() },
                None => Saved { dpapi: os_crypt::dpapi_seal(key.as_bytes()).map(|b| B64.encode(b)), ..Saved::default() },
            };
            std::fs::write(&path, serde_json::to_string(&saved).expect("plain JSON"))
        }
    };
    if let Err(e) = written {
        eprintln!("[spicy-key] could not save: {e}");
    }
}

pub fn get() -> Option<String> {
    KEY.lock().unwrap().clone()
}

/// Status in src/player.d.ts (spicyKeyStatus).
#[derive(Serialize)]
pub struct Status {
    set: bool,
    ending: Option<String>,
}

pub fn status() -> Status {
    let key = get();
    Status { set: key.is_some(), ending: key.map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()) }
}

/// `sl_pk_` or `sl_sk_` and at least eight more.
pub fn looks_like_key(s: &str) -> bool {
    let rest = s.strip_prefix("sl_pk_").or_else(|| s.strip_prefix("sl_sk_"));
    rest.is_some_and(|r| r.len() >= 8 && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

/// `{ ok }` or `{ ok: false, error }`, as the page expects.
#[derive(Serialize)]
pub struct Checked {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Checked {
    fn failed(error: String) -> Checked {
        Checked { ok: false, error: Some(error) }
    }
}

/// The API decides: a key it refuses answers 401 or 403. Not-found and
/// rate-limited answers still mean it took the key.
async fn verify(candidate: &str) -> Checked {
    if !looks_like_key(candidate) {
        return Checked::failed("That isn't a Spicy Lyrics key. They start with sl_sk_ or sl_pk_".into());
    }
    match api::get_lyrics(PROBE_TRACK, candidate).await {
        Ok(r) if r.http_status == 401 || r.http_status == 403 => Checked::failed(r.message.unwrap_or_else(|| {
            format!("Spicy Lyrics refused the key ({}).", r.error.unwrap_or(r.http_status.to_string()))
        })),
        Ok(_) => Checked { ok: true, error: None },
        Err(e) => Checked::failed(format!("Couldn't reach Spicy Lyrics: {e}")),
    }
}

/// Checks, then keeps, a key typed, pasted or copied in the key window.
pub async fn set(raw: &str) -> Checked {
    let candidate = raw.trim();
    let checked = verify(candidate).await;
    if checked.ok {
        store(Some(candidate));
    }
    checked
}

pub fn clear() {
    store(None);
}

#[cfg(test)]
mod tests {
    #[test]
    fn knows_a_key_by_its_shape() {
        assert!(super::looks_like_key("sl_sk_abcdEFGH12_-"));
        assert!(super::looks_like_key("sl_pk_12345678"));
        assert!(!super::looks_like_key("sl_sk_short"));
        assert!(!super::looks_like_key("sl_xk_abcdefgh"));
        assert!(!super::looks_like_key("sl_sk_abc defgh"));
    }
}
