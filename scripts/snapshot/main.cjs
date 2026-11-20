// Reference images of the UI in fixed states, for the Tauri rewrite to match.
//
//   npm run build
//   npm run snapshot [-- outDir]      (default: snapshots/electron)
//
// Loads dist/ with a fake window.player (preload.cjs): one paused song at a
// fixed position, NetEase-style word-timed lyrics, a generated cover. Each
// scene gets a fresh process and in-memory profile, so every setting is at its default
// unless the scene sets it. Every scene is captured twice, a second apart, and
// the run fails if the two differ: a reference that moves on its own is no use.
//
// Across runs the sung line's units can differ by up to ~20/255: the animator
// only writes a value once it moves by more than its epsilon, so where a
// settled spring stops depends on frame timing. Compare with a tolerance.

const { app, BrowserWindow, ipcMain, nativeImage } = require("electron");
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "..", "..");
const OUT = path.resolve(process.argv.slice(2).find((a) => !a.startsWith("-") && !a.endsWith(".cjs")) ?? path.join(ROOT, "snapshots", "electron"));
const STAGE_PAD = 22; // electron/main.cjs

const SCENES = [
  { name: "widget", hash: "", width: 350, storage: {} },
  { name: "widget-lyrics", hash: "", width: 350, storage: { "set.lyricsOnLaunch": true } },
  { name: "settings", hash: "#settings", width: 600, storage: {} },
  { name: "changelog", hash: "#changelog", width: 320, storage: {} },
  { name: "setup", hash: "#setup", width: 340, storage: {} },
];

// A 64x64 two-tone cover, so the glow and tint have colour to read.
function cover() {
  const n = 64;
  const buf = Buffer.alloc(n * n * 4);
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      const i = (y * n + x) * 4;
      const t = (x + y) / (2 * n);
      buf[i] = Math.round(40 + 60 * t); // B
      buf[i + 1] = Math.round(70 + 40 * t); // G
      buf[i + 2] = Math.round(200 - 60 * t); // R
      buf[i + 3] = 255;
    }
  }
  return nativeImage.createFromBitmap(buf, { width: n, height: n }).toDataURL();
}

// A second offscreen window in the same process fails to load (ERR_FAILED), so
// each scene runs in its own Electron process: the parent only spawns them.
const sceneArg = process.argv.find((a) => a.startsWith("--scene="));
if (!sceneArg) {
  const { spawnSync } = require("child_process");
  let failed = false;
  for (const [i] of SCENES.entries()) {
    const r = spawnSync(process.execPath, [__filename, `--scene=${i}`, OUT], { stdio: "inherit" });
    if (r.status !== 0) failed = true;
  }
  process.exit(failed ? 1 : 0);
}

app.disableHardwareAcceleration();
app.whenReady().then(async () => {
  fs.mkdirSync(OUT, { recursive: true });
  const scene = SCENES[Number(sceneArg.slice("--scene=".length))];
  const width = scene.width + STAGE_PAD * 2;
  const win = new BrowserWindow({
    show: false,
    width,
    height: 420,
    frame: false,
    transparent: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      partition: "snapshot", // no "persist:" → fresh, in memory
      offscreen: true,
      backgroundThrottling: false,
      additionalArguments: [`--snapshot-scene=${encodeURIComponent(JSON.stringify(scene))}`],
    },
  });
  ipcMain.handle("snap:resize", (_e, h) => win.setSize(width, Math.max(60, Math.ceil(h))));
  ipcMain.handle("snap:art", () => cover());
  win.webContents.on("console-message", (e) => {
    if (e.level === "error") console.log(`  [${scene.name}] ${e.message}`);
  });

  await win.loadFile(path.join(ROOT, "dist", "index.html"), { hash: scene.hash.slice(1) || undefined });
  await new Promise((r) => setTimeout(r, 4000)); // springs settle, sizes land
  const a = (await win.webContents.capturePage()).toPNG();
  await new Promise((r) => setTimeout(r, 1000));
  const b = (await win.webContents.capturePage()).toPNG();
  const file = path.join(OUT, `${scene.name}.png`);
  fs.writeFileSync(file, b);
  const stable = a.equals(b);
  console.log(`${scene.name}: ${win.getSize().join("x")} ${stable ? "stable" : "NOT STABLE"} → ${file}`);
  app.exit(stable ? 0 : 1);
});
