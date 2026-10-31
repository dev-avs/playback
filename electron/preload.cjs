const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("player", {
  // Subscribe to SMTC state/art/error events. Returns an unsubscribe fn.
  onEvent(cb) {
    const handler = (_e, msg) => cb(msg);
    ipcRenderer.on("smtc:event", handler);
    // SMTC usually reports before the page has subscribed, and that first
    // state would otherwise be lost until the next ~5 s heartbeat. Ask for it.
    let live = true;
    ipcRenderer.invoke("smtc:last").then((msg) => {
      if (live && msg) cb(msg);
    });
    return () => {
      live = false;
      ipcRenderer.off("smtc:event", handler);
    };
  },

  play: () => ipcRenderer.invoke("player:command", "play"),
  pause: () => ipcRenderer.invoke("player:command", "pause"),
  playPause: () => ipcRenderer.invoke("player:command", "playpause"),
  next: () => ipcRenderer.invoke("player:command", "next"),
  prev: () => ipcRenderer.invoke("player:command", "prev"),
  seek: (seconds) => ipcRenderer.invoke("player:command", `seek ${seconds}`),
  setVolume: (level) => ipcRenderer.invoke("player:command", `volume ${level}`),
  shuffle: () => ipcRenderer.invoke("player:command", "shuffle"),
  repeat: () => ipcRenderer.invoke("player:command", "repeat"),

  fetchLyrics: (req) => ipcRenderer.invoke("lyrics:fetch", req),
  fetchArt: (req) => ipcRenderer.invoke("art:fetch", req),
  fetchRomanized: (req) => ipcRenderer.invoke("lyrics:romanized", req),
  fetchSpicyLyrics: (req) => ipcRenderer.invoke("spicy:lyrics", req),

  // Fires when the Spicetify extension hands us a fresh token, so the lyrics
  // can be looked up again without waiting for the next track change.
  onTokenArrived(cb) {
    const handler = () => cb();
    ipcRenderer.on("spotify:tokenArrived", handler);
    return () => ipcRenderer.off("spotify:tokenArrived", handler);
  },

  // Fires when the Spicetify extension hands over lyrics Spicy Lyrics had
  // already cached, so the track playing now can upgrade to syllable timing
  // without waiting for the next track change.
  onSpicyLyrics(cb) {
    const handler = () => cb();
    ipcRenderer.on("spicy:lyricsArrived", handler);
    return () => ipcRenderer.off("spicy:lyricsArrived", handler);
  },

  // Fires when the Spicetify extension reports a new track, so a cover looked
  // up before Spotify said what was playing can be replaced with Spotify's own.
  onBridgedTrack(cb) {
    const handler = () => cb();
    ipcRenderer.on("bridge:track", handler);
    return () => ipcRenderer.off("bridge:track", handler);
  },

  // Updates from GitHub (electron/updater.cjs); nothing installs without a click.
  updateState: () => ipcRenderer.invoke("update:state"),
  checkForUpdate: () => ipcRenderer.invoke("update:check"),
  installUpdate: () => ipcRenderer.invoke("update:install"),
  skipUpdate: () => ipcRenderer.invoke("update:skip"),
  listReleases: () => ipcRenderer.invoke("update:releases"),
  bumpTo: (version) => ipcRenderer.invoke("update:bump", version),
  openVersions: () => ipcRenderer.invoke("versions:open"),
  onUpdateChanged(cb) {
    const handler = (_e, state) => cb(state);
    ipcRenderer.on("update:changed", handler);
    return () => ipcRenderer.off("update:changed", handler);
  },
  closeUpdate: () => ipcRenderer.invoke("changelog:close"),
  resizeUpdate: (height) => ipcRenderer.invoke("changelog:resize", height),

  // Removes the bridge, the app and its data, then quits.
  uninstall: () => ipcRenderer.invoke("app:uninstall"),

  // The user's own Spicy Lyrics API key (electron/spicyKey.cjs).
  spicyKeyStatus: () => ipcRenderer.invoke("spicyKey:status"),
  connectSpicyKey: () => ipcRenderer.invoke("spicyKey:connect"),
  setSpicyKey: (raw) => ipcRenderer.invoke("spicyKey:set", raw),
  clearSpicyKey: () => ipcRenderer.invoke("spicyKey:clear"),
  onSpicyKeyChanged(cb) {
    const handler = () => cb();
    ipcRenderer.on("spicyKey:changed", handler);
    return () => ipcRenderer.off("spicyKey:changed", handler);
  },

  spotifyStatus: () => ipcRenderer.invoke("spotify:status"),
  spotifyLogin: () => ipcRenderer.invoke("spotify:login"),
  spotifySetToken: (token) => ipcRenderer.invoke("spotify:setToken", token),
  spotifySignOut: () => ipcRenderer.invoke("spotify:signOut"),

  // Window dragging. The renderer only marks the start and end — the main
  // process follows the OS cursor itself, so no pointer event ever has to make
  // a round trip while the window is moving.
  dragStart: () => ipcRenderer.invoke("win:dragStart"),
  dragEnd: () => ipcRenderer.invoke("win:dragEnd"),

  resizeTo: (height, hovered) => ipcRenderer.invoke("win:resize", height, hovered),
  setAlwaysOnTop: (on) => ipcRenderer.invoke("win:setAlwaysOnTop", on),

  // The settings window.
  toggleSettings: () => ipcRenderer.invoke("settings:toggle"),
  openSettings: () => ipcRenderer.invoke("settings:open"),
  closeSettings: () => ipcRenderer.invoke("settings:close"),
  resizeSettings: (height) => ipcRenderer.invoke("settings:resize", height),
  // The changelog window.
  appVersion: () => ipcRenderer.invoke("app:version"),
  openChangelog: () => ipcRenderer.invoke("changelog:open"),
  openPastChangelog: () => ipcRenderer.invoke("changelog:openPast"),
  checkChangelog: (enabled) => ipcRenderer.invoke("changelog:check", enabled),
  setWidgetHidden: (hidden) => ipcRenderer.invoke("win:setHidden", hidden),
  setWidgetWidth: (width) => ipcRenderer.invoke("win:setWidth", width),
  closeChangelog: () => ipcRenderer.invoke("changelog:close"),
  resizeChangelog: (height) => ipcRenderer.invoke("changelog:resize", height),
  // The Setup window (Spotify, Spicetify and the bridge).
  setupState: () => ipcRenderer.invoke("setup:state"),
  recheckSetup: () => ipcRenderer.invoke("setup:recheck"),
  applySetup: () => ipcRenderer.invoke("setup:apply"),
  copyText: (text) => ipcRenderer.invoke("setup:copy", text),
  openLink: (url) => ipcRenderer.invoke("setup:openLink", url),
  // closes and sizes itself the way the changelog does
  closeSetup: () => ipcRenderer.invoke("changelog:close"),
  resizeSetup: (height) => ipcRenderer.invoke("changelog:resize", height),
  listFonts: () => ipcRenderer.invoke("fonts:list"),
  getNext: (req) => ipcRenderer.invoke("bridge:next", req),
  minimize: () => ipcRenderer.invoke("win:minimize"),
  close: () => ipcRenderer.invoke("win:close"),
});
