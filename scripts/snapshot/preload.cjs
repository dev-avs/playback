// Fake window.player + virtual clock for scripts/snapshot/main.cjs.
//
// Runs with contextIsolation off, so it shares the page's world: it can replace
// the clock before any page script reads it and set window.player directly.
//
// The fake player state is fixtures/fake-state.json (+ fake-cover.png), read by
// the main process and handed over through `snap:fixture`; a scene's `fake`
// object is deep-merged on top. Nothing reaches the network, Spotify or the OS.
//
// ── Virtual clock ──
// Everything that moves in the page is driven from here, in fixed steps:
//   performance.now, Date (now and `new Date()`), setTimeout/setInterval and
//   their clears, requestAnimationFrame/cancel, Math.random (seeded).
// The clock starts at 0 and does not move by itself. `__snap.step(ms)` runs the
// timers due in that span in order, then the rAF callbacks queued (timestamp =
// the new time), then seeks every CSS transition/animation and Web Animation:
// each is paused the first time it is seen and its currentTime set to
// (virtual now − virtual time it was first seen), finished ones are finished().
// So hovering, springs, fills and fades are exact, frame by frame, whatever
// the real frame rate of the offscreen window.
//
// `__snap` (page world) also carries `emit(name, payload)` for the on* listeners
// and `state(patch)` to push a new now-playing state.

const { ipcRenderer } = require("electron");

const arg = process.argv.find((a) => a.startsWith("--snapshot-scene="));
const scene = JSON.parse(decodeURIComponent(arg.slice("--snapshot-scene=".length)));
const fixture = ipcRenderer.sendSync("snap:fixture");

// Settings the scene asks for (already storage keys), in place before the page's first read.
for (const [k, v] of Object.entries(scene.storage ?? {})) localStorage.setItem(k, JSON.stringify(v));

function merge(base, over) {
  if (over === null || typeof over !== "object" || Array.isArray(over)) return over;
  const out = { ...base };
  for (const [k, v] of Object.entries(over)) out[k] = k in out && out[k] && typeof out[k] === "object" && !Array.isArray(out[k]) ? merge(out[k], v) : v;
  return out;
}
const FAKE = merge(fixture.fake, scene.fake ?? {});
const R = FAKE.responses;

// ── clock ──
const BASE_DATE = Date.UTC(2026, 10, 20, 12, 0, 0);
const RealDate = Date;
const realRaf = window.requestAnimationFrame.bind(window);
let vnow = 0;
let seq = 0;
const timers = new Map(); // id → { at, seq, fn, args, every }
let rafQueue = [];
let nextId = 1;

class FakeDate extends RealDate {
  constructor(...a) {
    if (a.length === 0) super(BASE_DATE + vnow);
    else super(...a);
  }
  static now() {
    return BASE_DATE + vnow;
  }
}
window.Date = FakeDate;
Object.defineProperty(performance, "now", { configurable: true, value: () => vnow });
window.setTimeout = (fn, delay = 0, ...args) => {
  const id = nextId++;
  timers.set(id, { at: vnow + Math.max(0, Number(delay) || 0), seq: seq++, fn, args, every: 0 });
  return id;
};
window.setInterval = (fn, delay = 0, ...args) => {
  const id = nextId++;
  const every = Math.max(1, Number(delay) || 0);
  timers.set(id, { at: vnow + every, seq: seq++, fn, args, every });
  return id;
};
window.clearTimeout = window.clearInterval = (id) => void timers.delete(id);
window.requestAnimationFrame = (cb) => {
  const id = nextId++;
  rafQueue.push({ id, cb });
  return id;
};
window.cancelAnimationFrame = (id) => {
  rafQueue = rafQueue.filter((r) => r.id !== id);
};
window.requestIdleCallback = (cb) => window.setTimeout(() => cb({ didTimeout: false, timeRemaining: () => 50 }), 1);
window.cancelIdleCallback = (id) => window.clearTimeout(id);
{
  let s = 0x9e3779b9;
  Math.random = () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const animStart = new WeakMap();
function sweepAnimations() {
  for (const a of document.getAnimations()) {
    if (a.playState === "finished") continue;
    let s = animStart.get(a);
    if (s === undefined) {
      s = vnow;
      animStart.set(a, s);
      a.pause();
    }
    const t = vnow - s;
    const end = a.effect?.getComputedTiming?.().endTime;
    if (typeof end === "number" && Number.isFinite(end) && t >= end) a.finish();
    else a.currentTime = t;
  }
}

function step(dt) {
  const target = vnow + dt;
  for (;;) {
    let next = null;
    for (const [id, t] of timers) if (t.at <= target && (!next || t.at < next.t.at || (t.at === next.t.at && t.seq < next.t.seq))) next = { id, t };
    if (!next) break;
    vnow = Math.max(vnow, next.t.at);
    if (next.t.every) {
      next.t.at += next.t.every;
      next.t.seq = seq++;
    } else timers.delete(next.id);
    if (typeof next.t.fn === "function") next.t.fn(...next.t.args);
  }
  vnow = target;
  const q = rafQueue;
  rafQueue = [];
  for (const r of q) r.cb(vnow);
  sweepAnimations();
}

// ── player ──
const handlers = {};
const on = (name) => (cb) => {
  (handlers[name] ??= new Set()).add(cb);
  return () => handlers[name].delete(cb);
};
let state = FAKE.state;
const stateEvent = () => ({ ...state, updatedAt: Date.now(), sampledAt: Date.now() });

window.__snap = {
  step,
  now: () => vnow,
  /** deliver an on* event to the page's listeners */
  emit: (name, payload) => (handlers[name] ?? []).forEach((cb) => cb(payload)),
  /** change the now-playing state and tell the page */
  state: (patch) => {
    state = merge(state, patch);
    (handlers.event ?? []).forEach((cb) => cb(stateEvent()));
  },
  /** the real rAF, for the runner to wait for painted frames */
  realFrame: () => new Promise((r) => realRaf(() => r())),
};

const ok = async () => true;
const none = async () => undefined;
// "__hang__" never resolves (a lookup still in flight)
const reply = (name) => () => (R[name] === "__hang__" ? new Promise(() => {}) : Promise.resolve(R[name]));
const resize = (h) => ipcRenderer.invoke("snap:resize", h);

window.player = {
  onEvent(cb) {
    (handlers.event ??= new Set()).add(cb);
    const id = FAKE.noEvent ? 0 : window.setTimeout(() => cb(stateEvent()), 50);
    return () => {
      window.clearTimeout(id);
      handlers.event.delete(cb);
    };
  },
  onTokenArrived: on("tokenArrived"),
  play: ok,
  pause: ok,
  playPause: ok,
  next: ok,
  prev: ok,
  seek: ok,
  setVolume: ok,
  shuffle: ok,
  repeat: ok,
  fetchLyrics: reply("fetchLyrics"),
  fetchArt: () => ipcRenderer.invoke("snap:art"),
  fetchRomanized: reply("fetchRomanized"),
  fetchReadings: reply("fetchReadings"),
  fetchSpicyLyrics: reply("fetchSpicyLyrics"),
  onSpicyLyrics: on("spicyLyrics"),
  updateState: reply("updateState"),
  checkForUpdate: reply("checkForUpdate"),
  installUpdate: reply("installUpdate"),
  skipUpdate: none,
  listReleases: reply("listReleases"),
  bumpTo: reply("bumpTo"),
  openVersions: none,
  onUpdateChanged: on("updateChanged"),
  closeUpdate: none,
  resizeUpdate: resize,
  uninstall: reply("uninstall"),
  spicyKeyStatus: reply("spicyKeyStatus"),
  connectSpicyKey: reply("connectSpicyKey"),
  setSpicyKey: reply("setSpicyKey"),
  clearSpicyKey: none,
  onSpicyKeyChanged: on("spicyKeyChanged"),
  onBridgedTrack: on("bridgedTrack"),
  onBridgedProgress: on("bridgedProgress"),
  spotifyStatus: reply("spotifyStatus"),
  spotifyLogin: reply("spotifyLogin"),
  spotifySetToken: reply("spotifySetToken"),
  spotifySignOut: none,
  dragStart: none,
  dragEnd: none,
  setFullscreen: async (on) => {
    ipcRenderer.invoke("snap:fullscreen", !!on);
    return R.setFullscreen;
  },
  onFullscreenToggle: on("fullscreen"),
  resizeTo: resize,
  setAlwaysOnTop: none,
  toggleSettings: none,
  openSettings: none,
  closeSettings: none,
  resizeSettings: resize,
  appVersion: reply("appVersion"),
  openChangelog: none,
  changelogOpen: reply("changelogOpen"),
  onChangelogOpen: on("changelogOpen"),
  openPastChangelog: none,
  checkChangelog: none,
  setWidgetHidden: none,
  setWidgetWidth: (w) => ipcRenderer.invoke("snap:width", w),
  closeChangelog: none,
  resizeChangelog: resize,
  setupState: reply("setupState"),
  recheckSetup: none,
  applySetup: none,
  copyText: none,
  openLink: none,
  closeSetup: none,
  resizeSetup: resize,
  listFonts: reply("listFonts"),
  getNext: reply("getNext"),
  minimize: none,
  close: none,
};
