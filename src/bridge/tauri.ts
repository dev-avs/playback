// window.player for the Tauri build (src-tauri/). The page was written against
// Electron's preload (electron/preload.cjs); this gives it the same API on top
// of Tauri's commands and events, so the components do not change.
//
// Does nothing outside Tauri. The optional parts src-tauri does not do yet
// are left out (installing updates, Setup's apply, fullscreen), which the
// page already handles.

import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { PlayerBridge, SmtcEvent } from "../player.d.ts";

/** listen() is async; the page wants a plain unsubscribe back. */
function on<T>(event: string, cb: EventCallback<T>): () => void {
  let stop: (() => void) | null = null;
  let stopped = false;
  listen<T>(event, cb).then((un) => {
    if (stopped) un();
    else stop = un;
  });
  return () => {
    stopped = true;
    stop?.();
  };
}

const command = (cmd: string) => invoke<boolean>("player_command", { cmd });

/** The card a window holds sizes its window, and closes it. */
const fit = (height: number) => invoke<void>("card_fit", { height });
const closeCard = () => invoke<void>("card_close");

/**
 * WebView2 ignores `-webkit-app-region: drag`, which the cards' headers use
 * to move their window. This reads it off the pressed element instead: the
 * nearest element that sets it decides, as in Chromium.
 */
function dragByAppRegion() {
  addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    for (let el = e.target as Element | null; el; el = el.parentElement) {
      const region = getComputedStyle(el).getPropertyValue("-webkit-app-region") || getComputedStyle(el).getPropertyValue("app-region");
      if (region === "no-drag") return;
      if (region === "drag") {
        e.preventDefault();
        getCurrentWindow().startDragging();
        return;
      }
    }
  });
}

function bridge(): PlayerBridge {
  return {
    onEvent(cb) {
      // the newest state first, then every change
      invoke<SmtcEvent | null>("smtc_last").then((s) => s && cb(s));
      return on<SmtcEvent>("smtc:event", (e) => cb(e.payload));
    },
    play: () => command("play"),
    pause: () => command("pause"),
    playPause: () => command("playpause"),
    next: () => command("next"),
    prev: () => command("prev"),
    seek: (seconds) => command(`seek ${seconds}`),
    setVolume: (level) => command(`volume ${level}`),
    shuffle: () => command("shuffle"),
    repeat: () => command("repeat"),

    fetchLyrics: (req) => invoke("lyrics_fetch", { req }),
    fetchArt: (req) => invoke("art_fetch", { req }),
    fetchRomanized: (req) => invoke("lyrics_romanized", { req }),
    fetchReadings: (texts) => invoke("lyrics_readings", { texts }),
    fetchSpicyLyrics: (req) => invoke("spicy_lyrics", { req }),
    onSpicyLyrics: (cb) => on("spicy:lyricsArrived", () => cb()),
    getNext: (req) => invoke("bridge_next", { req }),
    onBridgedTrack: (cb) => on("bridge:track", () => cb()),
    onBridgedProgress: (cb) => on<{ pos: number; at: number; playing: boolean; name: string | null }>("bridge:progress", (e) => cb(e.payload)),

    spicyKeyStatus: () => invoke("spicy_key_status"),
    connectSpicyKey: () => invoke("spicy_key_connect"),
    setSpicyKey: (raw) => invoke("spicy_key_set", { raw }),
    clearSpicyKey: () => invoke("spicy_key_clear"),
    onSpicyKeyChanged: (cb) => on("spicyKey:changed", () => cb()),

    spotifyStatus: () => invoke("spotify_status"),
    spotifyLogin: () => invoke("spotify_login"),
    spotifySetToken: (token) => invoke("spotify_set_token", { token }),
    spotifySignOut: () => invoke("spotify_sign_out"),
    onTokenArrived: (cb) => on("spotify:tokenArrived", () => cb()),

    updateState: () => invoke("update_state"),
    checkForUpdate: () => invoke("update_state"),
    listReleases: () => invoke("update_releases"),
    openVersions: () => invoke("versions_open"),
    closeUpdate: closeCard,
    resizeUpdate: fit,

    toggleSettings: () => invoke("settings_toggle"),
    openSettings: () => invoke("settings_open"),
    closeSettings: () => invoke("settings_close"),
    resizeSettings: fit,

    appVersion: () => invoke("app_version"),
    openChangelog: () => invoke("changelog_open"),
    changelogOpen: () => invoke("changelog_is_open"),
    onChangelogOpen: (cb) => on<boolean>("changelog:openChanged", (e) => cb(e.payload)),
    openPastChangelog: () => invoke("changelog_open_past"),
    checkChangelog: (enabled) => invoke("changelog_check", { enabled }),
    closeChangelog: closeCard,
    resizeChangelog: fit,

    setupState: () => invoke("setup_state"),
    recheckSetup: () => invoke("setup_state"),
    copyText: (text) => invoke("copy_text", { text }),
    openLink: (url) => invoke("open_link", { url }),
    closeSetup: closeCard,
    resizeSetup: fit,
    listFonts: () => invoke("fonts_list"),

    dragStart: () => invoke("win_drag_start"),
    dragEnd: () => invoke("win_drag_end"),
    resizeTo: (height, hovered) => invoke("win_resize", { height, hovered }),
    setWidgetWidth: (width) => invoke("win_set_width", { width }),
    setWidgetHidden: (hidden) => invoke("win_set_hidden", { hidden }),
    setAlwaysOnTop: (on) => invoke("win_set_always_on_top", { on }),
    minimize: () => invoke("win_minimize"),
    close: () => invoke("win_close"),
  };
}

if ("__TAURI_INTERNALS__" in window && !window.player) {
  window.player = bridge();
  dragByAppRegion();
}
