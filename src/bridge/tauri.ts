// window.player for the Tauri build (src-tauri/). The page was written against
// Electron's preload (electron/preload.cjs); this gives it the same API on top
// of Tauri's commands and events, so the components do not change.
//
// Does nothing outside Tauri. What src-tauri does not do yet answers like a
// player with nothing to offer (no lyrics, no art, not signed in), and the
// optional parts are left out, which the page already handles.

import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
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
    setVolume: async () => false,
    shuffle: () => command("shuffle"),
    repeat: () => command("repeat"),

    fetchLyrics: async () => null,
    fetchArt: async () => null,
    fetchRomanized: async () => null,
    fetchReadings: async () => null,
    fetchSpicyLyrics: async () => ({ status: "disabled" }),

    spotifyStatus: async () => ({ configured: false, signedIn: false, redirectUri: "", tokenExpiresAt: null }),
    spotifyLogin: async () => ({ ok: false, error: "not in this build yet" }),
    spotifySetToken: async () => ({ ok: false, error: "not in this build yet" }),
    spotifySignOut: async () => {},

    dragStart: () => invoke("win_drag_start"),
    dragEnd: () => invoke("win_drag_end"),
    resizeTo: (height, hovered) => invoke("win_resize", { height, hovered }),
    setWidgetWidth: (width) => invoke("win_set_width", { width }),
    setAlwaysOnTop: (on) => invoke("win_set_always_on_top", { on }),
    minimize: () => invoke("win_minimize"),
    close: () => invoke("win_close"),
  };
}

if ("__TAURI_INTERNALS__" in window && !window.player) window.player = bridge();
