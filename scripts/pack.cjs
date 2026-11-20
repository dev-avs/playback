// Builds the app and stages it in release/Playback-win32-x64 for
// electron-builder's --prepackaged step, which makes the web installer
// (release/nsis-web: the stub, the package and latest.yml).
//
// The app is one exe: `tauri build` embeds the built UI (dist/) and the
// Spicetify bridge in it. electron-builder's installer is kept from the
// Electron build so that every install made by it, the installer
// (installer/) and the in-app updater all carry on as they are; it looks for
// <productName>.exe, so the exe is staged as Playback.exe.
//
//   node scripts/pack.cjs [out]   (out defaults to release/Playback-win32-x64)

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const root = path.resolve(__dirname, "..");
const out = path.resolve(root, process.argv[2] ?? "release/Playback-win32-x64");

// runs `npm run build` first (tauri.conf.json: beforeBuildCommand)
execSync("npx tauri build --no-bundle", { cwd: root, stdio: "inherit" });

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
fs.copyFileSync(path.join(root, "src-tauri", "target", "release", "playback.exe"), path.join(out, "Playback.exe"));
console.log(`staged ${path.join(out, "Playback.exe")}`);
