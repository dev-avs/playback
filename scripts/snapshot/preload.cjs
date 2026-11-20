// Fake window.player for scripts/snapshot/main.cjs: everything fixed, nothing
// reaches the network, Spotify or the OS.

const { contextBridge, ipcRenderer } = require("electron");

const arg = process.argv.find((a) => a.startsWith("--snapshot-scene="));
const scene = JSON.parse(decodeURIComponent(arg.slice("--snapshot-scene=".length)));

// Settings the scene asks for, in place before the page's first read.
for (const [k, v] of Object.entries(scene.storage ?? {})) localStorage.setItem(k, JSON.stringify(v));

const STATE = {
  type: "state",
  appId: "Spotify.exe",
  title: "Test Song",
  artist: "Test Artist",
  album: "Test Album",
  trackKey: "Test Artist|Test Song",
  status: "Paused",
  playing: false,
  position: 31.5,
  duration: 200,
  shuffle: false,
  repeat: "None",
  volume: 0.5,
  caps: { play: true, pause: true, next: true, prev: true, seek: true, shuffle: true, repeat: true },
};

// NetEase yrc: a line every 4 s, words 0.6 s apart. Position 31.5 s sits mid-word.
const LINES = [
  "Morning light across the water",
  "Every window open wide",
  "We were singing something louder",
  "Than the engines on the tide",
  "Hold the moment, let it carry",
  "Every word we left behind",
  "And the city keeps on turning",
  "Like a record in my mind",
];
function yrc() {
  const out = [];
  for (let n = 0; n < 15; n++) {
    const text = LINES[n % LINES.length];
    const start = 2000 + n * 4000;
    const words = text.split(" ");
    const parts = words.map((w, i) => `(${start + i * 600},600,0)${w}${i < words.length - 1 ? " " : ""}`);
    out.push(`[${start},${words.length * 600}]${parts.join("")}`);
  }
  return out.join("\n");
}

const ok = async () => true;
const none = async () => undefined;
const off = () => () => {};
const resize = (h) => ipcRenderer.invoke("snap:resize", h);

contextBridge.exposeInMainWorld("player", {
  onEvent(cb) {
    const t = setTimeout(() => cb({ ...STATE, updatedAt: Date.now(), sampledAt: Date.now() }), 50);
    return () => clearTimeout(t);
  },
  onTokenArrived: off,
  play: ok,
  pause: ok,
  playPause: ok,
  next: ok,
  prev: ok,
  seek: ok,
  setVolume: ok,
  shuffle: ok,
  repeat: ok,
  fetchLyrics: async () => ({ wordLyrics: yrc(), source: "netease" }),
  fetchArt: () => ipcRenderer.invoke("snap:art"),
  fetchRomanized: async () => null,
  fetchReadings: async () => null,
  fetchSpicyLyrics: async () => ({ status: "not-found" }),
  onSpicyLyrics: off,
  updateState: async () => ({ status: "dev", version: null, downloaded: 0, total: null, error: null }),
  checkForUpdate: async () => ({ status: "dev", version: null, downloaded: 0, total: null, error: null }),
  installUpdate: async () => ({ status: "dev", version: null, downloaded: 0, total: null, error: null }),
  skipUpdate: none,
  listReleases: async () => ({ ok: false, error: "snapshot" }),
  bumpTo: async () => ({ status: "dev", version: null, downloaded: 0, total: null, error: null }),
  openVersions: none,
  onUpdateChanged: off,
  closeUpdate: none,
  resizeUpdate: resize,
  uninstall: async () => ({ ok: false, error: "snapshot" }),
  spicyKeyStatus: async () => ({ set: false, ending: null }),
  connectSpicyKey: async () => ({ ok: false }),
  setSpicyKey: async () => ({ ok: false }),
  clearSpicyKey: none,
  onSpicyKeyChanged: off,
  onBridgedTrack: off,
  onBridgedProgress: off,
  spotifyStatus: async () => ({ configured: false, signedIn: false, redirectUri: "", tokenExpiresAt: null }),
  spotifyLogin: async () => ({ ok: false }),
  spotifySetToken: async () => ({ ok: false }),
  spotifySignOut: none,
  dragStart: none,
  dragEnd: none,
  setFullscreen: async () => false,
  onFullscreenToggle: off,
  resizeTo: resize,
  setAlwaysOnTop: none,
  toggleSettings: none,
  openSettings: none,
  closeSettings: none,
  resizeSettings: resize,
  appVersion: async () => "0.0.0",
  openChangelog: none,
  changelogOpen: async () => false,
  onChangelogOpen: off,
  openPastChangelog: none,
  checkChangelog: none,
  setWidgetHidden: none,
  setWidgetWidth: none,
  closeChangelog: none,
  resizeChangelog: resize,
  setupState: async () => ({ firstRun: false, ready: true, needsApply: false, actions: [], status: { spotify: true, spicetify: true, bridgeCopied: true, registered: true, applied: true, appliedCurrent: true, legacy: [] } }),
  recheckSetup: none,
  applySetup: none,
  copyText: none,
  openLink: none,
  closeSetup: none,
  resizeSetup: resize,
  listFonts: async () => [],
  getNext: async () => null,
  minimize: none,
  close: none,
});
