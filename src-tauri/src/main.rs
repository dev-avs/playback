//! Playback, the Tauri build. Replaces electron/ piece by piece; the page is
//! the same React app, which reaches this side through src/bridge/tauri.ts.
//!
//! - `media`    what Windows is playing, its buttons and the app's volume
//! - `lyrics`   synced lyrics from LRCMux, NetEase and LrcLib
//! - `art`      the cover: catalogues, YouTube, the session's thumbnail
//! - `ui`       the windows (so far the widget)
//! - `support`  HTTP client, bounded cache, text and JSON helpers
//! - `commands` what the page calls, handed straight to the above

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod commands;
mod lyrics;
mod media;
mod support;
mod ui;

use tauri::{Emitter, Manager};

fn main() {
    tauri::Builder::default()
        .manage(ui::widget::Widget::default())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(media::Media::start(move |msg| {
                let _ = handle.emit_to("widget", "smtc:event", msg);
            }));
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::player_command,
            commands::smtc_last,
            commands::lyrics_fetch,
            commands::lyrics_romanized,
            commands::art_fetch,
            commands::win_resize,
            commands::win_set_width,
            commands::win_drag_start,
            commands::win_drag_end,
            commands::win_set_always_on_top,
            commands::win_minimize,
            commands::win_close,
        ])
        .run(tauri::generate_context!())
        .expect("Playback failed to start");
}
