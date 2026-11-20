//! Everything the page can call. Each command only hands over to the module
//! that does the work; nothing here decides anything.

use crate::media::Media;
use crate::ui::widget::Widget;
use crate::{art, lyrics};
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow};

// ── media ──────────────────────────────────────────────────────────

/// play | pause | playpause | next | prev | seek <sec> | shuffle | repeat | volume <0..1>
#[tauri::command]
pub fn player_command(cmd: String, media: State<Media>) -> bool {
    media.send(&cmd)
}

#[tauri::command]
pub fn smtc_last(media: State<Media>) -> Option<Value> {
    media.latest()
}

// ── lyrics and art ─────────────────────────────────────────────────

#[tauri::command]
pub async fn lyrics_fetch(req: lyrics::Request) -> Option<Value> {
    lyrics::fetch(req).await
}

#[derive(Deserialize)]
pub struct Song {
    artist: String,
    title: String,
}

#[tauri::command]
pub async fn lyrics_romanized(req: Song) -> Option<String> {
    lyrics::romanized(&req.artist, &req.title).await
}

#[tauri::command]
pub async fn art_fetch(req: art::Request) -> Option<String> {
    art::fetch(req).await
}

// ── the widget window ──────────────────────────────────────────────

#[tauri::command]
pub fn win_resize(height: f64, hovered: Option<bool>, win: WebviewWindow, w: State<Widget>) {
    w.resize(&win, height, hovered.unwrap_or(false));
}

#[tauri::command]
pub fn win_set_width(width: f64, win: WebviewWindow, w: State<Widget>) {
    w.set_width(&win, width);
}

#[tauri::command]
pub fn win_drag_start(win: WebviewWindow, w: State<Widget>) {
    w.drag_start(&win);
}

#[tauri::command]
pub fn win_drag_end(win: WebviewWindow, w: State<Widget>) {
    w.drag_end(&win);
}

#[tauri::command]
pub fn win_set_always_on_top(on: bool, win: WebviewWindow) {
    let _ = win.set_always_on_top(on);
}

#[tauri::command]
pub fn win_minimize(win: WebviewWindow) {
    let _ = win.minimize();
}

#[tauri::command]
pub fn win_close(app: AppHandle) {
    app.exit(0);
}
