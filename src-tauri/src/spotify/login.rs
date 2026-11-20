//! Signing in to Spotify: Authorization Code with PKCE, in the user's own
//! browser (never an embedded window, which would train people to type
//! their password into an app), back through a one-off loopback listener.

use super::token::{client_id, now_ms, save_signed_in, SignedIn};
use crate::support::http::client;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PORT: u16 = 8888;
pub const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
/// Enough for a user-identified token, which is all Spicy Lyrics needs.
const SCOPES: &str = "user-read-private";
/// The listener does not stay open forever.
const GIVE_UP: Duration = Duration::from_secs(5 * 60);

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn random_b64(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("the system has no random source");
    B64.encode(bytes)
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<f64>,
}

/// One call to Spotify's token endpoint. A refresh's answer may leave the
/// refresh token out, in which case the one asked with stays.
async fn exchange(params: &[(&str, &str)], kept_refresh: Option<&str>) -> Result<SignedIn, String> {
    let res = client()
        .post("https://accounts.spotify.com/api/token")
        .form(params)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("token endpoint {}: {}", status.as_u16(), body.chars().take(200).collect::<String>()));
    }
    let g: Granted = res.json().await.map_err(|e| e.to_string())?;
    Ok(SignedIn {
        access_token: Some(g.access_token),
        refresh_token: g.refresh_token.or(kept_refresh.map(String::from)),
        expires_at: Some(now_ms() + g.expires_in.unwrap_or(3600.0) * 1000.0 - 60_000.0),
    })
}

/// A new access token for a spent one.
pub async fn refresh(refresh_token: &str, id: &str) -> Result<SignedIn, String> {
    exchange(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", id)], Some(refresh_token)).await
}

/// The path and query of one HTTP request.
async fn request_target(stream: &mut TcpStream) -> Option<Url> {
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await.ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    let target = head.lines().next()?.split_whitespace().nth(1)?;
    Url::parse(REDIRECT_URI).ok()?.join(target).ok()
}

async fn reply(stream: &mut TcpStream, status: &str, body: &str) {
    let page = format!(r#"<!doctype html><meta charset="utf-8"><body style="font:15px system-ui;padding:40px">{body}</body>"#);
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        page.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(page.as_bytes()).await;
}

/// Waits for Spotify's redirect and turns its code into tokens.
async fn wait_for_redirect(listener: TcpListener, state: &str, verifier: &str, id: &str) -> Result<(), String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let Some(url) = request_target(&mut stream).await else { continue };
        if url.path() != "/callback" {
            reply(&mut stream, "404 Not Found", "").await;
            continue;
        }
        let param = |k: &str| url.query_pairs().find(|(name, _)| name == k).map(|(_, v)| v.into_owned());
        let result = async {
            if param("state").as_deref() != Some(state) {
                return Err("state mismatch".to_string());
            }
            if let Some(error) = param("error") {
                return Err(error);
            }
            let code = param("code").ok_or("no code in redirect")?;
            let granted = exchange(
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", REDIRECT_URI),
                    ("client_id", id),
                    ("code_verifier", verifier),
                ],
                None,
            )
            .await?;
            save_signed_in(&granted)
        }
        .await;
        match &result {
            Ok(()) => reply(&mut stream, "200 OK", "Signed in. You can close this tab and go back to Playback.").await,
            Err(e) => reply(&mut stream, "200 OK", &format!("Sign-in failed: {e}")).await,
        }
        return result;
    }
}

/// Runs the sign-in; `open` shows the consent page in the browser.
pub async fn login(open: impl FnOnce(&str)) -> Result<(), String> {
    let id = client_id().ok_or(format!(
        "No Spotify client id. Create an app at developer.spotify.com, add {REDIRECT_URI} as a redirect URI, \
         then set SPOTIFY_CLIENT_ID or write {{\"clientId\":\"...\"}} to spotify.json in Playback's data folder."
    ))?;
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return Err("a sign-in is already open in the browser".into());
    }
    let result = async {
        let listener = TcpListener::bind(("127.0.0.1", PORT)).await.map_err(|e| e.to_string())?;
        let verifier = random_b64(64);
        let challenge = B64.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_b64(16);
        let mut auth = Url::parse("https://accounts.spotify.com/authorize").expect("a valid URL");
        auth.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &id)
            .append_pair("scope", SCOPES)
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &challenge);
        open(auth.as_str());
        tokio::time::timeout(GIVE_UP, wait_for_redirect(listener, &state, &verifier, &id))
            .await
            .map_err(|_| "sign-in timed out".to_string())?
    }
    .await;
    IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}
