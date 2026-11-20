//! Spotify: a user token for the Spicy Lyrics API. Only the token comes from
//! here; the track id comes from the Spicetify bridge.
//!
//! A token gets here two ways: handed over (pasted, or later pushed by the
//! bridge), which wins since it comes from a real Spotify session, or
//! through a sign-in with the user's own Spotify app (`login`).

mod login;
mod token;

pub use login::login;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    signed_in: bool,
    redirect_uri: &'static str,
    /// when a handed-over token expires, epoch ms
    token_expires_at: Option<f64>,
}

pub fn status() -> Status {
    let manual = token::manual();
    let manual_valid = manual.as_ref().is_some_and(|m| m.expires_at.is_none_or(|e| token::now_ms() < e));
    Status {
        configured: token::client_id().is_some(),
        signed_in: manual_valid || token::signed_in().is_some_and(|t| t.refresh_token.is_some()),
        redirect_uri: login::REDIRECT_URI,
        token_expires_at: manual.and_then(|m| m.expires_at),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<f64>,
}

/// A pasted token.
pub fn set_token(raw: &str) -> Saved {
    match token::save_manual(raw, None) {
        Ok(at) => Saved { ok: true, error: None, expires_at: Some(at) },
        Err(e) => Saved { ok: false, error: Some(e), expires_at: None },
    }
}

pub fn sign_out() {
    token::clear();
}
