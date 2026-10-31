// Updates from the GitHub releases of dev-avs/player, never without a click:
// on launch the update window asks (Later / Update), and Settings → Behavior
// has Check and Update. Update downloads only what changed (the release's
// blockmap) and restarts player on the new version.
//
// Settings → Behavior → Version control installs any release, newer or older.
// From 1.3.7 on that goes through the updater, like an update. Older ones
// (1.3.3–1.3.6) carry no latest.yml, only a full installer: that is downloaded
// and run, and player quits for it. Those versions have no updater, so the
// window warns first that there is no way back from them but a reinstall.
//
// Only the installed app updates; the dev build reports "dev".

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");
const { Readable } = require("stream");
const { pipeline } = require("stream/promises");
const { app } = require("electron");
const { autoUpdater } = require("electron-updater");

const OWNER = "dev-avs";
const REPO = "player";
const GITHUB_FEED = { provider: "github", owner: OWNER, repo: REPO };

/**
 * status: idle | checking | none | available | downloading | error | dev
 * downloaded: bytes so far; total: the whole download's size, when known
 * @type {{status: string, version: string|null, downloaded: number, total: number|null, error: string|null}}
 */
let state = { status: app.isPackaged ? "idle" : "dev", version: null, downloaded: 0, total: null, error: null };
let onChange = () => {};

function set(next) {
  state = { ...state, ...next };
  onChange(state);
}

/** @param {(state) => void} notify called on every change, to broadcast it */
function init(notify) {
  onChange = notify;
  if (!app.isPackaged) return;
  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.logger = null;
  autoUpdater.setFeedURL(GITHUB_FEED);
  autoUpdater.on("download-progress", onProgress);
  autoUpdater.on("error", (e) => set({ status: "error", error: e?.message ?? String(e) }));
}

const getState = () => state;

/**
 * electron-updater's progress, added up into one running byte count.
 *
 * Its own percent is per part, not per update: the installer stub goes 0→100,
 * then the package's changed blocks arrive in batches that each go 0→100 of
 * their own size, so the shown percent kept starting over. Nothing reports
 * the size of the whole download, so the bytes are counted instead. A part
 * ends where its count drops or its total changes.
 */
let partsDone = 0;
let part = { total: 0, transferred: 0 };

function startDownload(version) {
  partsDone = 0;
  part = { total: 0, transferred: 0 };
  set({ status: "downloading", version, downloaded: 0, total: null, error: null });
}

function onProgress(p) {
  if (p.total !== part.total || p.transferred < part.transferred) {
    partsDone += part.transferred;
    part = { total: p.total, transferred: 0 };
  }
  part.transferred = p.transferred;
  set({ downloaded: partsDone + p.transferred });
}

async function check() {
  if (state.status === "dev" || state.status === "checking" || state.status === "downloading") return state;
  set({ status: "checking", error: null });
  try {
    const r = await autoUpdater.checkForUpdates();
    const available = !!r?.isUpdateAvailable;
    set({ status: available ? "available" : "none", version: r?.updateInfo?.version ?? null });
  } catch (e) {
    set({ status: "error", error: e.message });
  }
  return state;
}

/** Downloads the update and restarts into it. Checks first if nobody has. */
async function install() {
  if (state.status === "dev" || state.status === "downloading") return state;
  if (state.status !== "available") await check();
  if (state.status !== "available") return state;
  startDownload(state.version);
  try {
    await autoUpdater.downloadUpdate();
    // silent install, then start the new version
    autoUpdater.quitAndInstall(true, true);
  } catch (e) {
    set({ status: "error", error: e.message });
  }
  return state;
}

/**
 * Every release version control can install, newest first, with its notes (the release
 * body: `### Section` headings and `- ` points, as CLAUDE.md writes them).
 * @returns {Promise<{version: string, date: string, notes: string}[]>}
 */
// The full installer of each legacy release, from the last listing.
const legacyInstallers = new Map();

async function releases() {
  const res = await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases?per_page=100`, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
  const list = [];
  for (const r of await res.json()) {
    if (r.draft) continue;
    const version = r.tag_name.replace(/^v/, "");
    const legacy = !r.assets?.some((a) => a.name === "latest.yml");
    const installer = r.assets?.find((a) => a.name === `player-setup-${version}.exe`);
    if (legacy && !installer) continue; // nothing to install
    if (legacy) legacyInstallers.set(version, installer.browser_download_url);
    list.push({ version, date: r.published_at, notes: r.body ?? "", legacy });
  }
  return list;
}

/**
 * A legacy release: downloads its full installer and runs it as it is (not
 * silent, so it starts player again when it is done), then quits so it can
 * replace this one.
 */
async function installLegacy(version, url) {
  const res = await fetch(url);
  if (!res.ok || !res.body) throw new Error(`GitHub answered ${res.status}`);
  const total = Number(res.headers.get("content-length")) || 0;
  let got = 0;
  const file = path.join(os.tmpdir(), `player-setup-${version}.exe`);
  const body = Readable.fromWeb(res.body);
  body.on("data", (chunk) => {
    got += chunk.length;
    set({ downloaded: got, total: total || null });
  });
  await pipeline(body, fs.createWriteStream(file));
  spawn(file, [], { detached: true, stdio: "ignore" }).unref();
  app.quit();
}

/**
 * Installs one release, newer or older than this one, and restarts into it.
 * Points the updater at that release's own latest.yml for the one download,
 * and back at GitHub's newest if it fails.
 */
async function bump(version) {
  if (state.status === "dev" || state.status === "checking" || state.status === "downloading") return state;
  if (version === app.getVersion()) return state;
  startDownload(version);
  const legacy = legacyInstallers.get(version);
  if (legacy) {
    try {
      await installLegacy(version, legacy);
    } catch (e) {
      set({ status: "error", error: e.message });
    }
    return state;
  }
  try {
    autoUpdater.allowDowngrade = true;
    autoUpdater.setFeedURL({
      provider: "generic",
      url: `https://github.com/${OWNER}/${REPO}/releases/download/v${version}`,
    });
    const r = await autoUpdater.checkForUpdates();
    if (r?.updateInfo?.version !== version) throw new Error(`v${version} is not on GitHub`);
    await autoUpdater.downloadUpdate();
    autoUpdater.quitAndInstall(true, true);
  } catch (e) {
    autoUpdater.allowDowngrade = false;
    autoUpdater.setFeedURL(GITHUB_FEED);
    set({ status: "error", error: e.message });
  }
  return state;
}

// "Skip this version" in the update window: launch stops asking about that
// version, and asks again when a newer one is out. Check in Settings still
// finds it.
const skipFile = () => path.join(app.getPath("userData"), "update-skip.json");

function skip(version) {
  try {
    fs.writeFileSync(skipFile(), JSON.stringify({ version }));
  } catch (e) {
    console.error("[update] could not record the skip:", e.message);
  }
}

function isSkipped(version) {
  try {
    return JSON.parse(fs.readFileSync(skipFile(), "utf8")).version === version;
  } catch {
    return false;
  }
}

module.exports = { init, getState, check, install, releases, bump, skip, isSkipped };
