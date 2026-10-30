// Updates from the GitHub releases of dev-avs/player, never without a click:
// on launch the update window asks (Later / Update), and Settings → Behavior
// has Check and Update. Update downloads only what changed (the release's
// blockmap) and restarts player on the new version.
//
// Only the installed app updates; the dev build reports "dev".

const { app } = require("electron");
const { autoUpdater } = require("electron-updater");

/**
 * status: idle | checking | none | available | downloading | error | dev
 * @type {{status: string, version: string|null, percent: number, error: string|null}}
 */
let state = { status: app.isPackaged ? "idle" : "dev", version: null, percent: 0, error: null };
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
  autoUpdater.setFeedURL({ provider: "github", owner: "dev-avs", repo: "player" });
  autoUpdater.on("download-progress", (p) => set({ percent: Math.round(p.percent ?? 0) }));
  autoUpdater.on("error", (e) => set({ status: "error", error: e?.message ?? String(e) }));
}

const getState = () => state;

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
  set({ status: "downloading", percent: 0 });
  try {
    await autoUpdater.downloadUpdate();
    // silent install, then start the new version
    autoUpdater.quitAndInstall(true, true);
  } catch (e) {
    set({ status: "error", error: e.message });
  }
  return state;
}

module.exports = { init, getState, check, install };
