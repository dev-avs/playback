// Gets the Spicetify bridge into Spotify, and keeps it current.
//
// The bridge (`spicetify/player-bridge.js`) is a small companion script that
// runs inside Spotify and reads what the widget cannot get on its own: the
// Spotify token, the playing track's id and queue, and lyrics Spicy Lyrics
// already fetched. It posts them to the loopback bridge in tokenBridge.cjs.
//
// For it to run, four things must hold, and on a new PC none of them do:
//   1. Spicetify is installed (only the user can do that: it modifies Spotify);
//   2. the bridge is in Spicetify's Extensions folder, as the current version;
//   3. it is listed in config-xpui.ini (`spicetify config extensions …`);
//   4. Spicetify has been applied since, putting a copy into Spotify itself
//      (`%APPDATA%\Spotify\Apps\xpui\extensions`).
// check() reports each; ensure() does 2 and 3, refreshes an outdated copy in
// place, and does 4 only when allowed, because `spicetify apply` restarts
// Spotify and so cuts off whatever is playing.
//
// Before 1.3.3 the bridge was called player-token.js; ensure() also removes
// that name, so a PC set up then moves over cleanly.

const fs = require("fs");
const path = require("path");
const { execFile } = require("child_process");

const BRIDGE = "player-bridge.js";
const LEGACY = ["player-token.js"];

/** Where Spicetify and Spotify keep things, per their installers' defaults. */
function defaultPaths(env = process.env, platform = process.platform) {
  if (platform === "darwin") return macPaths(env);
  const local = env.LOCALAPPDATA ?? "";
  const roaming = env.APPDATA ?? "";
  return {
    cli: findCli(path.join(local, "spicetify", "spicetify.exe"), env),
    configDir: path.join(roaming, "spicetify"),
    spotifyExe: path.join(roaming, "Spotify", "Spotify.exe"),
    appliedDir: path.join(roaming, "Spotify", "Apps", "xpui", "extensions"),
  };
}

// macOS, from Spicetify's install.sh and docs; not yet checked on a real Mac
// (see mac-port.md). The CLI lands in ~/.spicetify, the config follows XDG,
// and Spotify is an app bundle that `spicetify apply` unpacks in place.
function macPaths(env) {
  const p = path.posix;
  const home = env.HOME ?? "";
  const spotifyApp =
    [p.join(home, "Applications", "Spotify.app"), "/Applications/Spotify.app"].find((a) => fs.existsSync(a)) ??
    "/Applications/Spotify.app";
  return {
    cli: findCli(p.join(home, ".spicetify", "spicetify"), env, p, "spicetify"),
    configDir: p.join(env.XDG_CONFIG_HOME || p.join(home, ".config"), "spicetify"),
    // the bundle stands in for Spotify.exe: check() only asks whether it exists
    spotifyExe: spotifyApp,
    appliedDir: p.join(spotifyApp, "Contents", "Resources", "Apps", "xpui", "extensions"),
  };
}

// The official installer puts spicetify.exe in %LOCALAPPDATA%\spicetify; other
// installs (scoop, winget, Homebrew) only put it on PATH.
function findCli(preferred, env, p = path, name = "spicetify.exe") {
  if (fs.existsSync(preferred)) return preferred;
  for (const dir of (env.PATH ?? "").split(p.delimiter)) {
    const exe = p.join(dir, name);
    if (dir && fs.existsSync(exe)) return exe;
  }
  return preferred;
}

/** The extensions listed in config-xpui.ini, or [] when there is none. */
function registered(configDir) {
  try {
    const ini = fs.readFileSync(path.join(configDir, "config-xpui.ini"), "utf8");
    const line = ini.match(/^extensions\s*=\s*(.*)$/m);
    return line ? line[1].split("|").map((s) => s.trim()).filter(Boolean) : [];
  } catch {
    return [];
  }
}

const sameAs = (file, source) => {
  try {
    return fs.readFileSync(file).equals(source);
  } catch {
    return false;
  }
};

/**
 * Where things stand. `ready` means the current bridge is live in Spotify and
 * nothing old is left behind.
 * @param source the bundled bridge's bytes
 */
function check(source, paths = defaultPaths()) {
  const list = registered(paths.configDir);
  const inSpotify = path.join(paths.appliedDir, BRIDGE);
  const status = {
    spotify: fs.existsSync(paths.spotifyExe),
    spicetify: fs.existsSync(paths.cli),
    bridgeCopied: sameAs(path.join(paths.configDir, "Extensions", BRIDGE), source),
    registered: list.includes(BRIDGE),
    applied: fs.existsSync(inSpotify),
    appliedCurrent: sameAs(inSpotify, source),
    legacy: LEGACY.filter(
      (n) =>
        list.includes(n) ||
        fs.existsSync(path.join(paths.configDir, "Extensions", n)) ||
        fs.existsSync(path.join(paths.appliedDir, n)),
    ),
  };
  return status;
}

const isReady = (s) =>
  s.spotify && s.spicetify && s.bridgeCopied && s.registered && s.appliedCurrent && !s.legacy.length;

/** Runs the spicetify command line; rejects with its output when it fails. */
function runCli(cli, args) {
  return new Promise((resolve, reject) => {
    const child = execFile(cli, args, { windowsHide: true, timeout: 180_000 }, (err, stdout, stderr) => {
      if (err) reject(new Error(`spicetify ${args.join(" ")}: ${(stderr || stdout || err.message).trim()}`));
      else resolve(stdout);
    });
    // nobody can answer a prompt here: end its input so one gets the default
    // at once instead of waiting out the timeout
    child.stdin?.end();
  });
}

/**
 * Puts the bridge in place as far as allowed.
 * @returns {{ status, ready: boolean, actions: string[], needsApply: boolean }}
 *   `needsApply`: everything is set but Spotify has not picked it up yet,
 *   because applying (a Spotify restart) was not allowed this time.
 */
async function ensure(source, { allowApply = false, paths = defaultPaths(), run = runCli } = {}) {
  const actions = [];
  let s = check(source, paths);
  if (!s.spicetify) return { status: s, ready: false, actions, needsApply: false };

  let changedList = false;
  for (const name of s.legacy) {
    for (const file of [path.join(paths.configDir, "Extensions", name), path.join(paths.appliedDir, name)]) {
      fs.rmSync(file, { force: true });
    }
    if (registered(paths.configDir).includes(name)) {
      await run(paths.cli, ["config", "extensions", `${name}-`]);
      changedList = true;
    }
    actions.push(`Removed the old ${name}`);
  }

  if (!s.bridgeCopied) {
    const file = path.join(paths.configDir, "Extensions", BRIDGE);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, source);
    actions.push("Copied the bridge into Spicetify");
  }

  if (!s.registered) {
    await run(paths.cli, ["config", "extensions", BRIDGE]);
    changedList = true;
    actions.push("Turned the bridge on in Spicetify");
  }

  // Spotify already has a copy, just an older one: overwrite it. Spotify reads
  // it on its next start, so no restart is forced for an update.
  const inSpotify = path.join(paths.appliedDir, BRIDGE);
  if (s.applied && !s.appliedCurrent) {
    fs.writeFileSync(inSpotify, source);
    actions.push("Updated the bridge inside Spotify");
  }

  const needsApply = changedList || !fs.existsSync(inSpotify);
  if (needsApply && allowApply) {
    try {
      await run(paths.cli, ["apply"]);
    } catch {
      // a Spicetify that has never been applied needs its backup taken first
      await run(paths.cli, ["backup", "apply"]);
    }
    actions.push("Applied Spicetify (Spotify restarted)");
  }

  s = check(source, paths);
  return { status: s, ready: isReady(s), actions, needsApply: needsApply && !allowApply };
}

/**
 * Whether the Setup window opens at launch: always on the first run, later
 * only when the user has something to do there. A missing Spotify or
 * Spicetify is not brought up again after the first run, since the widget
 * works without them; once Spicetify turns up, the bridge waiting on an apply
 * brings the window back.
 */
const shouldShowSetup = (result, firstRun) => firstRun || result.needsApply || !!result.error;

module.exports = { BRIDGE, check, ensure, isReady, shouldShowSetup, defaultPaths };
