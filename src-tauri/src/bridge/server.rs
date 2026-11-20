//! The loopback server the extension posts to, on 127.0.0.1:8899, as
//! electron/tokenBridge.cjs ran it.
//!
//! It takes credentials and never hands one back out. Only Spotify's own
//! page may post (a browser names the page a request comes from, and page
//! code cannot change that); a request with no Origin is not from a web
//! page at all (scripts/test-bridge.mjs). The one GET, /lookup, names a
//! song another app plays, so it is readable by Spotify's page only.

use serde_json::Value;
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PORT: u16 = 8899;
const SPOTIFY_ORIGIN: &str = "https://xpui.app.spotify.com";
/// A token push is tiny, a captured lyrics payload is not.
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_HEAD: usize = 16 * 1024;

struct Request {
    method: String,
    route: String,
    origin: Option<String>,
    length: usize,
    /// whatever of the body came in with the head
    body: Vec<u8>,
}

/// Reads up to the end of the head; None for anything that is not HTTP.
async fn read_head(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let (method, target) = (first.next()?.to_string(), first.next()?);
    let route = target.split('?').next().unwrap_or("").to_string();
    let mut origin = None;
    let mut length = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => origin = Some(value.trim().to_string()),
            "content-length" => length = value.trim().parse().ok()?,
            _ => {}
        }
    }
    Some(Request { method, route, origin, length, body: buf[end + 4..].to_vec() })
}

async fn reply(stream: &mut TcpStream, status: &str, allow_origin: &str, body: Option<&str>) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nAccess-Control-Allow-Origin: {allow_origin}\r\nAccess-Control-Allow-Headers: Content-Type\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.map_or(0, str::len)
    );
    if body.is_some() {
        head.push_str("Content-Type: application/json\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes()).await;
    if let Some(body) = body {
        let _ = stream.write_all(body.as_bytes()).await;
    }
    let _ = stream.shutdown().await;
}

async fn serve(app: AppHandle, mut stream: TcpStream) {
    let Some(mut req) = read_head(&mut stream).await else { return };
    if req.method == "OPTIONS" {
        return reply(&mut stream, "204 No Content", "*", None).await;
    }
    if req.method == "GET" && req.route == "/lookup" {
        let body = super::pending().to_string();
        return reply(&mut stream, "200 OK", SPOTIFY_ORIGIN, Some(&body)).await;
    }
    if req.method != "POST" || !["/token", "/spicy", "/resolved", "/progress"].contains(&req.route.as_str()) {
        return reply(&mut stream, "404 Not Found", "*", None).await;
    }
    if req.origin.as_deref().is_some_and(|o| o != SPOTIFY_ORIGIN) {
        return reply(&mut stream, "403 Forbidden", "*", None).await;
    }
    if req.length > MAX_BODY {
        return reply(&mut stream, "413 Payload Too Large", "*", None).await;
    }
    while req.body.len() < req.length {
        let mut chunk = vec![0u8; (req.length - req.body.len()).min(64 * 1024)];
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => req.body.extend_from_slice(&chunk[..n]),
        }
    }
    let payload: Value = match serde_json::from_slice(&req.body[..req.length.min(req.body.len())]) {
        Ok(v) => v,
        Err(e) => {
            let body = serde_json::json!({ "ok": false, "error": e.to_string() }).to_string();
            return reply(&mut stream, "400 Bad Request", "*", Some(&body)).await;
        }
    };
    let ok = r#"{"ok":true}"#.to_string();
    let (status, body) = match req.route.as_str() {
        "/resolved" => {
            super::on_resolved(&app, &payload);
            ("200 OK", ok)
        }
        "/progress" => {
            super::on_progress(&app, payload);
            ("200 OK", ok)
        }
        "/spicy" => ("200 OK", super::on_spicy(&app, &payload).to_string()),
        _ => {
            let saved = super::on_token(&app, &payload);
            (if saved.ok { "200 OK" } else { "400 Bad Request" }, serde_json::to_string(&saved).unwrap_or(ok))
        }
    };
    reply(&mut stream, status, "*", Some(&body)).await;
}

/// Listens for the extension. A second copy of the app finds the port
/// taken; the first one owns the bridge, and both read the same token file.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(l) => l,
            Err(e) => return eprintln!("[bridge] could not listen on {PORT}: {e}"),
        };
        eprintln!("[bridge] listening for the Spicetify extension on 127.0.0.1:{PORT}");
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let app = app.clone();
            // a stalled connection cannot hold anything up
            tauri::async_runtime::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(30), serve(app, stream)).await;
            });
        }
    });
}
