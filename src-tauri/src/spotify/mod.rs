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
pub use token::now_ms;

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
    Status {
        configured: token::client_id().is_some(),
        signed_in: signed_in(),
        redirect_uri: login::REDIRECT_URI,
        token_expires_at: token_expiry(),
    }
}

/// A live handed-over token, or a sign-in that can refresh itself.
pub fn signed_in() -> bool {
    let live = token::manual().is_some_and(|m| m.expires_at.is_none_or(|e| token::now_ms() < e));
    live || token::signed_in().is_some_and(|t| t.refresh_token.is_some())
}

/// When the handed-over token runs out, epoch ms.
pub fn token_expiry() -> Option<f64> {
    token::manual().and_then(|m| m.expires_at)
}

/// The token to call Spicy Lyrics with: a handed-over one first, since it
/// comes from a real Spotify session, else the sign-in's, refreshed once
/// spent. A spent handed-over token is dropped on the way.
pub async fn access_token() -> Option<String> {
    let now = token::now_ms();
    if let Some(m) = token::manual() {
        if m.expires_at.is_none_or(|e| now < e) {
            return Some(m.access_token);
        }
        token::clear_manual();
    }
    let t = token::signed_in()?;
    if let Some(live) = t.access_token.filter(|_| now < t.expires_at.unwrap_or(0.0)) {
        return Some(live);
    }
    let id = token::client_id()?;
    match login::refresh(t.refresh_token.as_deref()?, &id).await {
        Ok(next) => {
            let _ = token::save_signed_in(&next);
            next.access_token
        }
        Err(e) => {
            eprintln!("[spotify] refresh failed: {e}");
            None
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<f64>,
}

/// A pasted token.
pub fn set_token(raw: &str) -> Saved {
    hand_over(raw, None)
}

/// A token from the Spicetify bridge, which knows when it runs out.
pub fn hand_over(raw: &str, expires_at: Option<f64>) -> Saved {
    match token::save_manual(raw, expires_at) {
        Ok(at) => Saved {
            ok: true,
            error: None,
            expires_at: Some(at),
        },
        Err(e) => Saved {
            ok: false,
            error: Some(e),
            expires_at: None,
        },
    }
}

pub fn sign_out() {
    token::clear();
}
