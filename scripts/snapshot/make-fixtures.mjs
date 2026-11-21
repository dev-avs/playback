// One-off generator for fixtures/fake-cover.png and fixtures/fake-state.json.
// Run once (`node scripts/snapshot/make-fixtures.mjs`) and commit the output;
// nothing regenerates them at run time, so Electron and GPUI read the same bytes.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const OUT = path.join(ROOT, "fixtures");
fs.mkdirSync(OUT, { recursive: true });

// A 64x64 two-tone cover, so the glow and tint have colour to read.
const n = 64;
const png = new PNG({ width: n, height: n });
for (let y = 0; y < n; y++) {
  for (let x = 0; x < n; x++) {
    const i = (y * n + x) * 4;
    const t = (x + y) / (2 * n);
    png.data[i] = Math.round(200 - 60 * t); // R
    png.data[i + 1] = Math.round(70 + 40 * t); // G
    png.data[i + 2] = Math.round(40 + 60 * t); // B
    png.data[i + 3] = 255;
  }
}
fs.writeFileSync(path.join(OUT, "fake-cover.png"), PNG.sync.write(png));

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
const yrc = [];
for (let k = 0; k < 15; k++) {
  const text = LINES[k % LINES.length];
  const start = 2000 + k * 4000;
  const words = text.split(" ");
  const parts = words.map((w, i) => `(${start + i * 600},600,0)${w}${i < words.length - 1 ? " " : ""}`);
  yrc.push(`[${start},${words.length * 600}]${parts.join("")}`);
}

const ready = { firstRun: false, ready: true, needsApply: false, actions: [], status: { spotify: true, spicetify: true, bridgeCopied: true, registered: true, applied: true, appliedCurrent: true, legacy: [] } };
const dev = { status: "dev", version: null, downloaded: 0, total: null, error: null };

const fake = {
  // Documented in scripts/snapshot/preload.cjs. `state` is the SMTC event the widget gets
  // (updatedAt/sampledAt are stamped with the clock's "now" when it is delivered).
  state: {
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
  },
  cover: "fake-cover.png",
  // What each async window.player call resolves to. Calls not listed here do nothing.
  responses: {
    fetchLyrics: { wordLyrics: yrc.join("\n"), source: "netease" },
    fetchRomanized: null,
    fetchReadings: null,
    fetchSpicyLyrics: { status: "not-found" },
    updateState: dev,
    checkForUpdate: dev,
    installUpdate: dev,
    bumpTo: dev,
    listReleases: { ok: false, error: "snapshot" },
    uninstall: { ok: false, error: "snapshot" },
    spicyKeyStatus: { set: false, ending: null },
    connectSpicyKey: { ok: false },
    setSpicyKey: { ok: false },
    spotifyStatus: { configured: false, signedIn: false, redirectUri: "", tokenExpiresAt: null },
    spotifyLogin: { ok: false },
    spotifySetToken: { ok: false },
    setFullscreen: false,
    appVersion: "0.0.0",
    changelogOpen: false,
    setupState: ready,
    listFonts: [],
    getNext: null,
  },
};
fs.writeFileSync(path.join(OUT, "fake-state.json"), JSON.stringify(fake, null, 2) + "\n");
console.log("wrote fixtures/");
