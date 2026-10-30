// Packages the app into release/player-win32-x64 for electron-builder's
// --prepackaged step (electron-builder's own packaging fails on this machine).
//
// What goes in is an allow-list: the built UI, the main process, the bridge,
// package.json, and exactly the production dependencies npm reports. Packager's
// own pruning dropped electron-updater, so it is turned off here.

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const { packager } = require("@electron/packager");

const root = path.resolve(__dirname, "..");

// Matches build.publish in package.json.
const APP_UPDATE_YML = "owner: dev-avs\nrepo: player\nprovider: github\nupdaterCacheDirName: player-updater\n";

const prodModules = execSync("npm ls --omit=dev --all --parseable", { cwd: root, encoding: "utf8" })
  .split(/\r?\n/)
  .map((p) => path.relative(root, p.trim()).split(path.sep).join("/"))
  .filter((p) => p.startsWith("node_modules/"));

const allowed = ["dist", "electron", "spicetify", "package.json", ...prodModules].map((p) => `/${p}`);

// Keep a path when it is an allowed entry, inside one, or a folder leading to one.
const keep = (p) => allowed.some((a) => p === a || p.startsWith(`${a}/`) || a.startsWith(`${p}/`));

packager({
  dir: root,
  name: "player",
  platform: "win32",
  arch: "x64",
  out: path.join(root, "release"),
  overwrite: true,
  icon: path.join(root, "build", "icon.ico"),
  asar: { unpackDir: "electron" },
  prune: false,
  ignore: (p) => p !== "" && !keep(p.split(path.sep).join("/")),
})
  .then((paths) => {
    // electron-builder writes this into resources when it packages the app
    // itself; with --prepackaged it does not, and electron-updater needs it to
    // download an update (electron/updater.cjs).
    for (const dir of paths) {
      fs.writeFileSync(path.join(dir, "resources", "app-update.yml"), APP_UPDATE_YML);
    }
    console.log(`packaged ${paths.join(", ")} with ${prodModules.length} production modules`);
  })
  .catch((e) => {
    console.error(e);
    process.exit(1);
  });
