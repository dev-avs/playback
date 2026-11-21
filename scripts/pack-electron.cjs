// Builds the Electron flavor: release/falcon (the app) and, in release/nsis-web,
// playback-electron-setup-<version>.exe + playback-electron-<version>-x64.nsis.7z,
// plus release/falcon-latest.yml (its feed, naming those files; it is the
// release's latest.yml only on an Electron revision, 1.6.1-a).
//
// Built straight from the nsis-web target, not `dir` + --prepackaged: only the
// installer target writes resources/app-update.yml, without which
// electron-updater fails ("ENOENT … app-update.yml", Electron 1.6.1).
//
//   node scripts/pack-electron.cjs   (after `npm run build`)

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const root = path.resolve(__dirname, "..");
const release = path.join(root, "release");
const tmp = path.join(release, "falcon-build");
const { version } = require(path.join(root, "package.json"));

fs.rmSync(tmp, { recursive: true, force: true });
execSync(
  `npx electron-builder --win nsis-web --publish never "--config.directories.output=release/falcon-build" "--config.nsisWeb.artifactName=playback-electron-setup-\${version}.exe"`,
  { cwd: root, stdio: "inherit" },
);

const unpacked = path.join(tmp, "win-unpacked");
if (!fs.existsSync(path.join(unpacked, "resources", "app-update.yml"))) {
  throw new Error("the Electron build has no resources/app-update.yml");
}
fs.rmSync(path.join(release, "falcon"), { recursive: true, force: true });
fs.renameSync(unpacked, path.join(release, "falcon"));

const web = path.join(tmp, "nsis-web");
const out = path.join(release, "nsis-web");
fs.mkdirSync(out, { recursive: true });
const stub = `playback-electron-setup-${version}.exe`;
const pkg = `playback-electron-${version}-x64.nsis.7z`;
fs.renameSync(path.join(web, stub), path.join(out, stub));
fs.renameSync(path.join(web, `playback-${version}-x64.nsis.7z`), path.join(out, pkg));
const yml = fs
  .readFileSync(path.join(web, "latest.yml"), "utf8")
  .replaceAll(`playback-${version}-x64.nsis.7z`, pkg);
fs.writeFileSync(path.join(release, "falcon-latest.yml"), yml);
fs.rmSync(tmp, { recursive: true, force: true });
console.log(`staged release/falcon, release/nsis-web/${stub}, ${pkg} and release/falcon-latest.yml`);
