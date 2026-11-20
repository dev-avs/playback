//! Playback, the Tauri build. Replaces electron/ piece by piece; the page is
//! the same React app, which reaches this side through src/bridge/tauri.ts.
//!
//! - `media`    what Windows is playing, its buttons and the app's volume
//! - `lyrics`   synced lyrics from LRCMux, NetEase and LrcLib
//! - `art`      the cover: catalogues, YouTube, the session's thumbnail
//! - `spotify`  a Spotify token: sign-in, pasted, kept on disk
//! - `spicy`    Spicy Lyrics: its API, the lyrics cache, the user's key
//! - `bridge`   what the Spicetify extension inside Spotify sends
//! - `spicetify` where the Spotify bridge stands (Setup)
//! - `updates`  releases on GitHub (version control)
//! - `ui`       the widget, the card windows, fonts, What's new
//! - `support`  HTTP client, bounded cache, text and JSON helpers
//! - `commands` what the page calls, handed straight to the above

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod bridge;
mod commands;
mod lyrics;
mod media;
mod spicetify;
mod spicy;
mod spotify;
mod support;
mod ui;
mod updates;

use tauri::{Emitter, Manager};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ui::widget::Widget::default())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(media::Media::start(move |msg| {
                let _ = handle.emit_to("widget", "smtc:event", msg);
            }));
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(true);
            }
            bridge::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::player_command,
            commands::smtc_last,
            commands::lyrics_fetch,
            commands::lyrics_romanized,
            commands::lyrics_readings,
            commands::art_fetch,
            commands::spotify_status,
            commands::spotify_set_token,
            commands::spotify_login,
            commands::spotify_sign_out,
            commands::spicy_lyrics,
            commands::bridge_next,
            commands::spicy_key_status,
            commands::spicy_key_connect,
            commands::spicy_key_set,
            commands::spicy_key_clear,
            commands::settings_toggle,
            commands::settings_open,
            commands::settings_close,
            commands::changelog_open,
            commands::changelog_open_past,
            commands::changelog_is_open,
            commands::changelog_check,
            commands::versions_open,
            commands::card_fit,
            commands::card_close,
            commands::app_version,
            commands::fonts_list,
            commands::setup_state,
            commands::update_state,
            commands::update_releases,
            commands::copy_text,
            commands::open_link,
            commands::win_set_hidden,
            commands::win_resize,
            commands::win_set_width,
            commands::win_drag_start,
            commands::win_drag_end,
            commands::win_set_always_on_top,
            commands::win_minimize,
            commands::win_close,
        ])
        .build(tauri::generate_context!())
        .expect("Playback failed to start")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                bridge::shut_down();
            }
        });
}
