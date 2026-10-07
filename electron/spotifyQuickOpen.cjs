// Starts Spotify hidden for a few seconds, for what only a running client has.
//
// A Spotify token is minted by the client's native core and lives only while
// it runs, and the catalog search that turns another app's title/artist into a
// Spotify id is only usable from inside it (the Web API answers the client's
// token with 429). The Spicetify extension does both and posts the results to
// the bridge; this only opens Spotify for it and closes it again.
//
// Never touches a Spotify the user started: if one is already running the
// extension is live in it and nothing needs opening, and only a copy opened
// here is ever closed.

const fs = require("fs");
const path = require("path");
const { spawn, execFile } = require("child_process");

const EXE = path.join(process.env.APPDATA ?? "", "Spotify", "Spotify.exe");
// Token and search both land within ~5 s of launch; past this something is
// wrong (offline, logged out, extension missing) and the client is not worth
// keeping around for it.
const MAX_OPEN_MS = 25_000;

let openedAt = null;
let giveUpTimer = null;

function isRunning() {
  return new Promise((resolve) =>
    execFile(
      "tasklist",
      ["/FI", "IMAGENAME eq Spotify.exe", "/NH"],
      { windowsHide: true },
      (err, out) => resolve(!err && /spotify\.exe/i.test(out)),
    ),
  );
}

// --minimized is what Spotify's own "start with Windows" uses; it still shows
// a taskbar button, which hideWindow takes away. A minimized page also gets
// its timers throttled to one every ~15 s, which made the extension take 24 s
// instead of 6 to answer — Spotify passes Chromium switches through, so these
// turn that off for the few seconds this copy runs.
const ARGS = [
  "--minimized",
  "--disable-background-timer-throttling",
  "--disable-renderer-backgrounding",
  "--disable-backgrounding-occluded-windows",
];

/**
 * True when a hidden Spotify is (now) running on our behalf.
 * @param hideWindow called just before launch to start hiding its window
 */
async function open(hideWindow) {
  if (openedAt) return true;
  if (!fs.existsSync(EXE) || (await isRunning())) return false;
  hideWindow?.();
  spawn(EXE, ARGS, { detached: true, stdio: "ignore", windowsHide: true }).unref();
  openedAt = Date.now();
  giveUpTimer = setTimeout(close, MAX_OPEN_MS);
  console.log("[quick-open] started Spotify hidden");
  return true;
}

function close() {
  if (!openedAt) return;
  clearTimeout(giveUpTimer);
  console.log(`[quick-open] closing Spotify after ${Date.now() - openedAt} ms`);
  openedAt = null;
  execFile("taskkill", ["/IM", "Spotify.exe", "/T", "/F"], { windowsHide: true }, () => {});
}

/** The user has taken this Spotify over (it is playing): leave it running. */
function release() {
  if (!openedAt) return;
  clearTimeout(giveUpTimer);
  openedAt = null;
}

const isOpen = () => openedAt != null;

module.exports = { open, close, release, isOpen };
