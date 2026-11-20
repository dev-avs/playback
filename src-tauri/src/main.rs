// Playback, the Tauri build. Replaces electron/ piece by piece; the page is
// the same React app, which reaches this side through src/bridge/tauri.ts.
//
// So far: the widget window and Windows' media controls.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod smtc;
mod widget;

use serde_json::Value;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State, WebviewWindow};

struct Smtc {
    commands: Mutex<Sender<smtc::Command>>,
    latest: Arc<Mutex<Option<Value>>>,
}

/// play | pause | playpause | next | prev | seek <sec> | shuffle | repeat
#[tauri::command]
fn player_command(cmd: String, smtc: State<Smtc>) -> bool {
    match smtc::Command::parse(&cmd) {
        Some(c) => smtc.commands.lock().unwrap().send(c).is_ok(),
        None => false,
    }
}

/// The newest state, for a page that loaded after it was sent.
#[tauri::command]
fn smtc_last(smtc: State<Smtc>) -> Option<Value> {
    smtc.latest.lock().unwrap().clone()
}

#[tauri::command]
fn win_resize(height: f64, hovered: Option<bool>, win: WebviewWindow, w: State<widget::Widget>) {
    w.resize(&win, height, hovered.unwrap_or(false));
}

#[tauri::command]
fn win_set_width(width: f64, win: WebviewWindow, w: State<widget::Widget>) {
    w.set_width(&win, width);
}

#[tauri::command]
fn win_drag_start(win: WebviewWindow, w: State<widget::Widget>) {
    w.drag_start(&win);
}

#[tauri::command]
fn win_drag_end(win: WebviewWindow, w: State<widget::Widget>) {
    w.drag_end(&win);
}

#[tauri::command]
fn win_set_always_on_top(on: bool, win: WebviewWindow) {
    let _ = win.set_always_on_top(on);
}

#[tauri::command]
fn win_minimize(win: WebviewWindow) {
    let _ = win.minimize();
}

#[tauri::command]
fn win_close(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let (tx, rx) = channel();
    let latest = Arc::new(Mutex::new(None));
    tauri::Builder::default()
        .manage(Smtc { commands: Mutex::new(tx), latest: latest.clone() })
        .manage(widget::Widget::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            smtc::spawn(rx, latest, move |msg| {
                let _ = handle.emit_to("widget", "smtc:event", msg);
            });
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            player_command,
            smtc_last,
            win_resize,
            win_set_width,
            win_drag_start,
            win_drag_end,
            win_set_always_on_top,
            win_minimize,
            win_close,
        ])
        .run(tauri::generate_context!())
        .expect("Playback failed to start");
}
