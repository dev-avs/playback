//! Everything the page can call. Each command only hands over to the module
//! that does the work; nothing here decides anything.

use crate::host::TauriHost;
use crate::media::Media;
use crate::spicetify::setup::{Setup, SetupState};
use crate::ui::cards::{self, Card};
use crate::ui::widget::Widget;
use crate::updates::{UpdateState, Updater};
use crate::{art, bridge, install, lyrics, spicy, spotify, ui};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

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
pub async fn lyrics_readings(texts: Vec<String>) -> Option<Vec<Vec<lyrics::readings::Token>>> {
    lyrics::readings::fetch(texts).await
}

#[tauri::command]
pub async fn art_fetch(req: art::Request) -> Option<String> {
    art::fetch(req).await
}

// ── Spotify ────────────────────────────────────────────────────────

/// Every window looks the lyrics up again with the new token.
fn token_changed(app: &AppHandle) {
    let _ = app.emit("spotify:tokenArrived", ());
}

#[tauri::command]
pub fn spotify_status() -> spotify::Status {
    spotify::status()
}

#[tauri::command]
pub fn spotify_set_token(token: String, app: AppHandle) -> spotify::Saved {
    let saved = spotify::set_token(&token);
    if saved.ok {
        token_changed(&app);
    }
    saved
}

#[tauri::command]
pub async fn spotify_login(app: AppHandle) -> Value {
    let opener = app.clone();
    let done = spotify::login(move |url| {
        let _ = opener.opener().open_url(url, None::<&str>);
    })
    .await;
    match done {
        Ok(()) => {
            token_changed(&app);
            json!({ "ok": true })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
pub fn spotify_sign_out(app: AppHandle) {
    spotify::sign_out();
    token_changed(&app);
}

// ── Spicy Lyrics ───────────────────────────────────────────────────

#[tauri::command]
pub async fn spicy_lyrics(req: bridge::Song, app: AppHandle) -> Value {
    bridge::spicy_lyrics(&TauriHost(app), req).await
}

#[derive(Deserialize)]
pub struct Playing {
    title: String,
}

#[tauri::command]
pub fn bridge_next(req: Playing) -> Option<Value> {
    bridge::next(&req.title)
}

/// Setup, Settings and the widget's lyrics follow every change of key.
fn spicy_key_changed(app: &AppHandle, checked: spicy::key::Checked) -> spicy::key::Checked {
    if checked.ok {
        let _ = app.emit("spicyKey:changed", ());
    }
    checked
}

#[tauri::command]
pub fn spicy_key_status() -> spicy::key::Status {
    spicy::key::status()
}

#[tauri::command]
pub async fn spicy_key_connect(app: AppHandle) -> spicy::key::Checked {
    let checked = crate::spicy_connect::connect(&app).await;
    spicy_key_changed(&app, checked)
}

#[tauri::command]
pub async fn spicy_key_set(raw: String, app: AppHandle) -> spicy::key::Checked {
    let checked = spicy::key::set(&raw).await;
    spicy_key_changed(&app, checked)
}

#[tauri::command]
pub fn spicy_key_clear(app: AppHandle) {
    spicy::key::clear();
    let _ = app.emit("spicyKey:changed", ());
}

// ── the card windows ───────────────────────────────────────────────
//
// Whatever can open a window is async: a window built from a sync command
// waits on the main thread the command is holding, and both stop.

#[tauri::command]
pub async fn settings_toggle(app: AppHandle) -> tauri::Result<()> {
    if cards::is_open(&app, Card::Settings) {
        cards::close(&app, Card::Settings);
        Ok(())
    } else {
        cards::open(&app, Card::Settings, None)
    }
}

#[tauri::command]
pub async fn settings_open(app: AppHandle) -> tauri::Result<()> {
    cards::open(&app, Card::Settings, None)
}

#[tauri::command]
pub fn settings_close(app: AppHandle) {
    cards::close(&app, Card::Settings);
}

#[tauri::command]
pub async fn changelog_open(app: AppHandle) -> tauri::Result<()> {
    cards::open(&app, Card::Changelog, None)
}

#[tauri::command]
pub async fn changelog_open_past(app: AppHandle) -> tauri::Result<()> {
    cards::open(&app, Card::PastChangelog, None)
}

#[tauri::command]
pub fn changelog_is_open(app: AppHandle) -> bool {
    cards::is_open(&app, Card::Changelog)
}

#[tauri::command]
pub async fn changelog_check(enabled: bool, app: AppHandle) -> tauri::Result<()> {
    ui::changelog::check_after_update(&app, enabled).await
}

#[tauri::command]
pub async fn versions_open(app: AppHandle) -> tauri::Result<()> {
    cards::open(&app, Card::Versions, None)
}

/// The calling card fits its window to its height.
#[tauri::command]
pub fn card_fit(height: f64, win: WebviewWindow) {
    cards::fit(&win, height);
}

/// The calling card closes.
#[tauri::command]
pub fn card_close(win: WebviewWindow) {
    let _ = win.close();
}

// ── setup, updates, the rest ───────────────────────────────────────

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn fonts_list() -> Vec<String> {
    ui::fonts::list()
}

#[tauri::command]
pub async fn setup_state(setup: State<'_, Setup>) -> Result<SetupState, ()> {
    Ok(setup.state().await)
}

#[tauri::command]
pub async fn setup_recheck(setup: State<'_, Setup>) -> Result<SetupState, ()> {
    Ok(setup.run(false).await)
}

/// Setup's Apply, and Turn on for a bridge left off in the installer.
#[tauri::command]
pub async fn setup_apply(setup: State<'_, Setup>) -> Result<SetupState, ()> {
    Ok(setup.apply().await)
}

#[tauri::command]
pub fn update_state(updater: State<Updater>) -> UpdateState {
    updater.state()
}

#[tauri::command]
pub async fn update_check(app: AppHandle, updater: State<'_, Updater>) -> Result<UpdateState, ()> {
    Ok(updater.check(&TauriHost(app)).await)
}

#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    updater: State<'_, Updater>,
) -> Result<UpdateState, ()> {
    Ok(updater.install(&TauriHost(app)).await)
}

/// Version control: any release, older or newer.
#[tauri::command]
pub async fn update_bump(
    version: String,
    flavor: Option<String>,
    settings_json: Option<String>,
    app: AppHandle,
    updater: State<'_, Updater>,
) -> Result<UpdateState, ()> {
    Ok(updater.bump(&TauriHost(app), version, flavor, settings_json).await)
}

#[tauri::command]
pub fn update_skip(updater: State<Updater>) {
    updater.skip();
}

#[tauri::command]
pub async fn update_releases(updater: State<'_, Updater>) -> Result<Value, ()> {
    Ok(updater.releases().await)
}

#[tauri::command]
pub async fn app_uninstall(app: AppHandle) -> Value {
    match install::uninstall(&app).await {
        Ok(()) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[tauri::command]
pub fn copy_text(text: String, app: AppHandle) {
    let _ = app.clipboard().write_text(text);
}

/// https only: the page never opens anything else.
#[tauri::command]
pub fn open_link(url: String, app: AppHandle) {
    if url.starts_with("https://") {
        let _ = app.opener().open_url(url, None::<&str>);
    }
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
pub fn win_set_hidden(hidden: bool, app: AppHandle, w: State<Widget>) {
    if let Some(win) = app.get_webview_window(ui::WIDGET) {
        w.set_hidden(&win, hidden);
    }
}

#[tauri::command]
pub fn win_fullscreen(on: bool, win: WebviewWindow, w: State<Widget>) -> bool {
    w.set_fullscreen(&win, on)
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
