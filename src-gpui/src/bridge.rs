//! Loopback HTTP server on 127.0.0.1:8899 for the Spicetify extension
//! (`spicetify/player-bridge.js`), matching `electron/tokenBridge.cjs` and
//! `src-tauri/src/bridge/server.rs`.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PORT: u16 = 8899;
const SPOTIFY_ORIGIN: &str = "https://xpui.app.spotify.com";
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_HEAD: usize = 16 * 1024;

#[derive(Clone, Debug, Default)]
pub struct NextTrack {
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct BridgeState {
    pub connected: bool,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub position: f64,
    pub playing: bool,
    pub sampled_at: Instant,
    pub next: Option<NextTrack>,
    pub shuffle: bool,
    pub repeat: u8, // 0 = off, 1 = context, 2 = track
    pub token: Option<String>,
    pub spicy_payloads: HashMap<String, Value>,
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            connected: false,
            track_id: String::new(),
            title: String::new(),
            artist: String::new(),
            duration: 0.0,
            position: 0.0,
            playing: false,
            sampled_at: Instant::now(),
            next: None,
            shuffle: false,
            repeat: 0,
            token: None,
            spicy_payloads: HashMap::new(),
        }
    }
}

struct Request {
    method: String,
    route: String,
    origin: Option<String>,
    length: usize,
    body: Vec<u8>,
}

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
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => origin = Some(value.trim().to_string()),
            "content-length" => length = value.trim().parse().ok()?,
            _ => {}
        }
    }
    Some(Request {
        method,
        route,
        origin,
        length,
        body: buf[end + 4..].to_vec(),
    })
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

fn handle_post(state: &Arc<RwLock<BridgeState>>, route: &str, payload: Value) {
    let Ok(mut st) = state.write() else { return };
    st.connected = true;
    match route {
        "/token" => {
            if let Some(tok) = payload["token"].as_str().filter(|s| !s.is_empty()) {
                st.token = Some(tok.to_string());
            }
            if let Some(track) = payload.get("track").and_then(|v| v.as_object()) {
                if let Some(id) = track.get("id").and_then(|v| v.as_str()) {
                    st.track_id = id.to_string();
                }
                if let Some(t) = track.get("title").and_then(|v| v.as_str()) {
                    st.title = t.to_string();
                }
                if let Some(a) = track.get("artist").and_then(|v| v.as_str()) {
                    st.artist = a.to_string();
                }
                if let Some(d) = track.get("duration").and_then(|v| v.as_f64()) {
                    st.duration = if d > 10_000.0 { d / 1000.0 } else { d };
                }
            }
            if let Some(next) = payload.get("next").and_then(|v| v.as_object()) {
                let title = next
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                st.next = if !title.is_empty() {
                    Some(NextTrack { title })
                } else {
                    None
                };
            }
            if let Some(shuf) = payload.get("shuffle").and_then(|v| v.as_bool()) {
                st.shuffle = shuf;
            }
            if let Some(rep) = payload.get("repeat").and_then(|v| v.as_u64()) {
                st.repeat = rep as u8;
            }
        }
        "/progress" => {
            if let Some(pos) = payload["position"].as_f64() {
                st.position = if pos > 10_000.0 { pos / 1000.0 } else { pos };
                st.sampled_at = Instant::now();
            }
            if let Some(dur) = payload["duration"].as_f64() {
                st.duration = if dur > 10_000.0 { dur / 1000.0 } else { dur };
            }
            if let Some(playing) = payload["playing"].as_bool() {
                st.playing = playing;
            }
            if let Some(id) = payload["id"].as_str().filter(|s| !s.is_empty()) {
                st.track_id = id.to_string();
            }
        }
        "/spicy" => {
            let id = payload["id"]
                .as_str()
                .or_else(|| payload["trackId"].as_str())
                .unwrap_or(&st.track_id)
                .to_string();
            if !id.is_empty() {
                let body = payload.get("lyrics").cloned().unwrap_or(payload);
                st.spicy_payloads.insert(id, body);
            }
        }
        _ => {}
    }
}

async fn serve(state: Arc<RwLock<BridgeState>>, mut stream: TcpStream) {
    let Some(mut req) = read_head(&mut stream).await else {
        return;
    };
    if req.method == "OPTIONS" {
        return reply(&mut stream, "204 No Content", "*", None).await;
    }
    if req.method == "GET" && req.route == "/lookup" {
        return reply(&mut stream, "200 OK", SPOTIFY_ORIGIN, Some("null")).await;
    }
    if req.method != "POST"
        || !["/token", "/spicy", "/resolved", "/progress"].contains(&req.route.as_str())
    {
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
    if let Ok(payload) =
        serde_json::from_slice::<Value>(&req.body[..req.length.min(req.body.len())])
    {
        handle_post(&state, &req.route, payload);
        reply(&mut stream, "200 OK", SPOTIFY_ORIGIN, Some("{\"ok\":true}")).await;
    } else {
        reply(&mut stream, "400 Bad Request", SPOTIFY_ORIGIN, None).await;
    }
}

pub async fn run_server(state: Arc<RwLock<BridgeState>>) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", PORT)).await else {
        return;
    };
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let st = state.clone();
            tokio::spawn(async move {
                serve(st, stream).await;
            });
        }
    }
}
