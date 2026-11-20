//! The Spotify tokens on disk, in electron/spotify.cjs' files and format:
//!
//! - `spotify-manual.json`: a token handed over by hand (later also by the
//!   Spicetify bridge), `{ access_token, expires_at }`;
//! - `spotify-tokens.json`: the sign-in's tokens, `{ access_token,
//!   refresh_token, expires_at }`;
//! - `spotify.json`: the user's own Spotify app, `{ clientId }`.

use crate::support::paths::data_file;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::SystemTime;

const MANUAL: &str = "spotify-manual.json";
const SIGNED_IN: &str = "spotify-tokens.json";

/// Spotify's web tokens last an hour. One whose expiry cannot be read is
/// trusted for a little less, counted from when it was saved, so a spent
/// token lapses by itself and the page offers to take a new one.
const OPAQUE_TOKEN_TTL_MS: f64 = 55.0 * 60_000.0;

pub fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

#[derive(Serialize, Deserialize)]
pub struct Manual {
    pub access_token: String,
    pub expires_at: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct SignedIn {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<f64>,
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(data_file(name)).ok()?).ok()
}

/// The handed-over token. Records from before tokens had an expiry are aged
/// from the file's own time.
pub fn manual() -> Option<Manual> {
    let mut m: Manual = read(MANUAL)?;
    if m.expires_at.is_none() {
        let saved = fs::metadata(data_file(MANUAL))
            .and_then(|f| f.modified())
            .ok()?;
        let saved = saved
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs_f64()
            * 1000.0;
        m.expires_at = Some(saved + OPAQUE_TOKEN_TTL_MS);
    }
    Some(m)
}

pub fn signed_in() -> Option<SignedIn> {
    read(SIGNED_IN)
}

pub fn save_signed_in(tokens: &SignedIn) -> Result<(), String> {
    let json = serde_json::to_string(tokens).map_err(|e| e.to_string())?;
    fs::write(data_file(SIGNED_IN), json).map_err(|e| e.to_string())
}

pub fn client_id() -> Option<String> {
    #[derive(Deserialize)]
    struct App {
        #[serde(rename = "clientId")]
        client_id: Option<String>,
    }
    let id = std::env::var("SPOTIFY_CLIENT_ID")
        .ok()
        .or_else(|| read::<App>("spotify.json")?.client_id)?;
    Some(id.trim().to_string()).filter(|id| !id.is_empty())
}

/// Spotify's tokens are JWT-shaped: the expiry is read off the payload. The
/// signature is not checked; the expiry only drives a hint and drops a dead
/// token early.
fn jwt_expiry(token: &str) -> Option<f64> {
    let payload = token.split('.').nth(1)?.trim_end_matches('=');
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims["exp"].as_f64().map(|s| s * 1000.0)
}

/// An expiry as epoch ms. Some Spotify builds report seconds, which read as
/// ms would be 1970 and look long expired.
fn as_epoch_ms(reported: f64) -> Option<f64> {
    (reported.is_finite() && reported > 0.0).then_some(if reported < 1e12 {
        reported * 1000.0
    } else {
        reported
    })
}

/// Checks a handed-over token: the token itself, and when it dies if known.
/// `known_expiry` is the expiry the caller was told along with it.
pub fn accept(
    raw: &str,
    known_expiry: Option<f64>,
    now: f64,
) -> Result<(String, Option<f64>), String> {
    let token = raw.trim();
    let token = match token.get(..7) {
        Some(p) if p.eq_ignore_ascii_case("bearer ") => token[7..].trim_start(),
        _ => token,
    };
    if token.is_empty() {
        return Err("empty token".into());
    }
    // catch paste mistakes here rather than as a confusing 401 later
    if token.len() < 40 || token.chars().any(char::is_whitespace) {
        return Err("that does not look like an access token".into());
    }
    let jwt = jwt_expiry(token);
    let reported = known_expiry.and_then(as_epoch_ms).filter(|e| *e > now);
    if jwt.is_some_and(|e| e < now) || (known_expiry.is_some() && reported.is_none()) {
        return Err("that token has already expired".into());
    }
    Ok((token.to_string(), reported.or(jwt)))
}

/// Saves a handed-over token and returns when it expires.
pub fn save_manual(raw: &str, known_expiry: Option<f64>) -> Result<f64, String> {
    let now = now_ms();
    let (token, expiry) = accept(raw, known_expiry, now)?;
    // the bridge re-sends the same token every few seconds
    if let Some(current) = manual() {
        let until = current.expires_at.unwrap_or(0.0);
        if current.access_token == token && until > now + 5.0 * 60_000.0 {
            return Ok(until);
        }
    }
    let expires_at = expiry.unwrap_or(now + OPAQUE_TOKEN_TTL_MS);
    let record = Manual {
        access_token: token.clone(),
        expires_at: Some(expires_at),
    };
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    fs::write(data_file(MANUAL), json).map_err(|e| format!("could not save token: {e}"))?;
    // read back: a silent write failure looks fine until the next start
    match read::<Manual>(MANUAL) {
        Some(back) if back.access_token == token => Ok(expires_at),
        _ => Err("could not save token: verify-after-write mismatch".into()),
    }
}

/// Signs out: both the handed-over token and the sign-in's.
pub fn clear_manual() {
    let _ = fs::remove_file(data_file(MANUAL));
}

pub fn clear() {
    clear_manual();
    let _ = fs::remove_file(data_file(SIGNED_IN));
}

#[cfg(test)]
mod tests {
    use super::accept;
    use base64::Engine;

    const NOW: f64 = 1_800_000_000_000.0;

    fn jwt(exp_s: f64) -> String {
        let claims =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp_s}}}"));
        format!("eyJhbGciOiJIUzI1NiJ9.{claims}.signature-signature-signature")
    }

    #[test]
    fn refuses_paste_mistakes() {
        assert_eq!(accept("  ", None, NOW).unwrap_err(), "empty token");
        assert_eq!(
            accept("short", None, NOW).unwrap_err(),
            "that does not look like an access token"
        );
        assert!(accept(&format!("{} x", "a".repeat(50)), None, NOW).is_err());
    }

    #[test]
    fn reads_the_expiry_off_the_token() {
        let token = jwt(NOW / 1000.0 + 600.0);
        assert_eq!(
            accept(&format!("Bearer {token}"), None, NOW).unwrap(),
            (token, Some(NOW + 600_000.0))
        );
        assert_eq!(
            accept(&jwt(NOW / 1000.0 - 1.0), None, NOW).unwrap_err(),
            "that token has already expired"
        );
        // an opaque token's expiry is not known
        assert_eq!(accept(&"a".repeat(50), None, NOW).unwrap().1, None);
    }

    #[test]
    fn takes_a_reported_expiry_in_seconds_or_ms() {
        let token = "a".repeat(50);
        assert_eq!(
            accept(&token, Some(NOW + 1000.0), NOW).unwrap().1,
            Some(NOW + 1000.0)
        );
        assert_eq!(
            accept(&token, Some(NOW / 1000.0 + 1.0), NOW).unwrap().1,
            Some(NOW + 1000.0)
        );
        assert!(accept(&token, Some(NOW - 1.0), NOW).is_err());
    }
}
