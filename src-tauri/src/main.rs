//! Playback, the Tauri build. Replaces electron/ piece by piece; the page is
//! the same React app, which reaches this side through src/bridge/tauri.ts.
//! The backend modules below live in crates/playback-core, shared with the
//! GPUI build; `host` is how they reach this app.
//!
//! - `media`    what Windows is playing, its buttons and the app's volume
//! - `lyrics`   synced lyrics from LRCMux, NetEase and LrcLib
//! - `art`      the cover: catalogues, YouTube, the session's thumbnail
//! - `spotify`  a Spotify token: sign-in, pasted, kept on disk
//! - `spicy`    Spicy Lyrics: its API, the lyrics cache, the user's key
//! - `bridge`   what the Spicetify extension inside Spotify sends
//! - `spicetify` the Spotify bridge in Spicetify (Setup)
//! - `updates`  updates and version control, from the GitHub releases
//! - `install`  the installed app: detached installers, uninstalling
//! - `migrate`  what the Electron build left: old data folder, settings
//! - `launch`   Setup, then the widget, then What's new and the update prompt
//! - `ui`       the widget, the card windows, fonts, What's new
//! - `support`  HTTP client, bounded cache, text and JSON helpers
//! - `commands` what the page calls, handed straight to the above

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod host;
mod install;
mod launch;
mod migrate;
mod spicy_connect;
mod ui;

// the shared backend (crates/playback-core), under the names it had here
use playback_core::{art, bridge, lyrics, media, spicetify, spicy, spotify, support, updates};

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Fullscreen on and off, from anywhere: the widget toggles it.
fn fullscreen_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    const KEYS: &str = "CommandOrControl+Alt+F";
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit_to(ui::WIDGET, "fullscreen:toggle", ());
            }
        })
        .build();
    app.plugin(plugin)?;
    // taken by another app: fullscreen still has its button
    if let Err(e) = app.global_shortcut().register(KEYS) {
        eprintln!("[shortcut] {KEYS}: {e}");
    }
    Ok(())
}

fn main() {
    playback_core::rt::init(tauri::async_runtime::handle().inner().clone());
    migrate::adopt_legacy_folder();
    tauri::Builder::default()
        // first, so a second launch leaves before anything has started
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            launch::second_instance(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ui::widget::Widget::default())
        .manage(updates::Updater::default())
        .setup(|app| {
            fullscreen_shortcut(app.handle())?;
            let handle = app.handle().clone();
            app.manage(media::Media::start(move |msg| {
                let _ = handle.emit_to("widget", "smtc:event", msg);
            }));
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(true);
            }
            bridge::start(Arc::new(host::TauriHost(app.handle().clone())));
            launch::start(app.handle())?;
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
            commands::setup_recheck,
            commands::setup_apply,
            commands::update_state,
            commands::update_check,
            commands::update_install,
            commands::update_bump,
            commands::update_skip,
            commands::update_releases,
            commands::app_uninstall,
            commands::copy_text,
            commands::open_link,
            commands::win_set_hidden,
            commands::win_resize,
            commands::win_set_width,
            commands::win_drag_start,
            commands::win_drag_end,
            commands::win_fullscreen,
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
