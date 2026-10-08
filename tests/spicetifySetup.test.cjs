const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { check, ensure, shouldShowSetup, defaultPaths, BRIDGE } = require("../electron/spicetifySetup.cjs");

const SOURCE = Buffer.from("// bridge v2\n");

/**
 * A pretend machine: Spotify, an optional Spicetify, and a fake `spicetify`
 * command that edits config-xpui.ini and "applies" the way the real one does.
 */
function machine(t, { spicetify = true, spotify = true, extensions = [], files = {}, appliedFiles = {}, applyFailsUntilBackup = false } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "player-setup-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const paths = {
    cli: path.join(root, "local", "spicetify", "spicetify.exe"),
    configDir: path.join(root, "roaming", "spicetify"),
    spotifyExe: path.join(root, "roaming", "Spotify", "Spotify.exe"),
    appliedDir: path.join(root, "roaming", "Spotify", "Apps", "xpui", "extensions"),
  };
  const put = (file, body) => {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, body);
  };
  if (spotify) put(paths.spotifyExe, "");
  if (spicetify) {
    put(paths.cli, "");
    put(path.join(paths.configDir, "config-xpui.ini"), `[AdditionalOptions]\nextensions            = ${extensions.join("|")}\n`);
  }
  for (const [n, body] of Object.entries(files)) put(path.join(paths.configDir, "Extensions", n), body);
  for (const [n, body] of Object.entries(appliedFiles)) put(path.join(paths.appliedDir, n), body);

  const calls = [];
  let backedUp = !applyFailsUntilBackup;
  const run = async (cli, args) => {
    assert.equal(cli, paths.cli);
    calls.push(args.join(" "));
    const ini = path.join(paths.configDir, "config-xpui.ini");
    const list = () => fs.readFileSync(ini, "utf8").match(/^extensions\s*=\s*(.*)$/m)[1].split("|").filter(Boolean);
    const save = (l) => fs.writeFileSync(ini, `[AdditionalOptions]\nextensions            = ${l.join("|")}\n`);
    if (args[0] === "config" && args[1] === "extensions") {
      const name = args[2];
      if (name.endsWith("-")) save(list().filter((n) => n !== name.slice(0, -1)));
      else if (!list().includes(name)) save([...list(), name]);
    } else if (args.join(" ") === "apply" || args.join(" ") === "backup apply") {
      if (args[0] === "apply" && !backedUp) throw new Error("backup first");
      backedUp = true;
      fs.rmSync(paths.appliedDir, { recursive: true, force: true });
      for (const n of list()) {
        const src = path.join(paths.configDir, "Extensions", n);
        if (fs.existsSync(src)) put(path.join(paths.appliedDir, n), fs.readFileSync(src));
      }
    }
  };
  return { paths, run, calls };
}

test("no Spicetify: reports it and changes nothing", async (t) => {
  const m = machine(t, { spicetify: false });
  const r = await ensure(SOURCE, { allowApply: true, paths: m.paths, run: m.run });
  assert.equal(r.status.spicetify, false);
  assert.equal(r.status.spotify, true);
  assert.deepEqual(m.calls, []);
});

test("no Spotify desktop app is reported", (t) => {
  const m = machine(t, { spotify: false });
  assert.equal(check(SOURCE, m.paths).spotify, false);
});

test("fresh Spicetify: copies and registers, and waits to apply until allowed", async (t) => {
  const m = machine(t);
  const r = await ensure(SOURCE, { allowApply: false, paths: m.paths, run: m.run });
  assert.equal(r.status.bridgeCopied, true);
  assert.equal(r.status.registered, true);
  assert.equal(r.needsApply, true);
  assert.deepEqual(m.calls, [`config extensions ${BRIDGE}`]);
});

test("first run applies, and the bridge ends up live in Spotify", async (t) => {
  const m = machine(t);
  const r = await ensure(SOURCE, { allowApply: true, paths: m.paths, run: m.run });
  assert.equal(r.needsApply, false);
  assert.equal(r.status.appliedCurrent, true);
  assert.equal(r.ready, true);
  assert.deepEqual(m.calls, [`config extensions ${BRIDGE}`, "apply"]);
});

test("a never-applied Spicetify falls back to backup apply", async (t) => {
  const m = machine(t, { applyFailsUntilBackup: true });
  const r = await ensure(SOURCE, { allowApply: true, paths: m.paths, run: m.run });
  assert.equal(r.ready, true);
  assert.deepEqual(m.calls.slice(-2), ["apply", "backup apply"]);
});

test("the old player-token name is removed and replaced", async (t) => {
  const m = machine(t, {
    extensions: ["adblock.js", "player-token.js"],
    files: { "player-token.js": "old" },
    appliedFiles: { "player-token.js": "old" },
  });
  assert.deepEqual(check(SOURCE, m.paths).legacy, ["player-token.js"]);
  const r = await ensure(SOURCE, { allowApply: true, paths: m.paths, run: m.run });
  assert.deepEqual(r.status.legacy, []);
  assert.equal(r.ready, true);
  assert.ok(!fs.existsSync(path.join(m.paths.configDir, "Extensions", "player-token.js")));
  const ini = fs.readFileSync(path.join(m.paths.configDir, "config-xpui.ini"), "utf8");
  assert.match(ini, /adblock\.js\|player-bridge\.js/);
  assert.doesNotMatch(ini, /player-token/);
});

test("an outdated copy in Spotify is refreshed in place, without a restart", async (t) => {
  const m = machine(t, {
    extensions: [BRIDGE],
    files: { [BRIDGE]: SOURCE },
    appliedFiles: { [BRIDGE]: "// bridge v1\n" },
  });
  const r = await ensure(SOURCE, { allowApply: false, paths: m.paths, run: m.run });
  assert.equal(r.status.appliedCurrent, true);
  assert.equal(r.needsApply, false);
  assert.deepEqual(m.calls, []);
});

test("everything current: nothing to do", async (t) => {
  const m = machine(t, { extensions: [BRIDGE], files: { [BRIDGE]: SOURCE }, appliedFiles: { [BRIDGE]: SOURCE } });
  const r = await ensure(SOURCE, { allowApply: false, paths: m.paths, run: m.run });
  assert.equal(r.ready, true);
  assert.deepEqual(r.actions, []);
  assert.deepEqual(m.calls, []);
});

test("a PC set up before 1.3.3: migrates at launch, then waits on one apply until it is given", async (t) => {
  // as on the dev PC: player-token.js listed, in Extensions and live in Spotify
  const m = machine(t, {
    extensions: ["adblock.js", "player-token.js"],
    files: { "player-token.js": "old" },
    appliedFiles: { "player-token.js": "old" },
  });
  const launch = await ensure(SOURCE, { allowApply: false, paths: m.paths, run: m.run });
  assert.equal(launch.needsApply, true);
  assert.equal(launch.ready, false);
  assert.deepEqual(launch.status.legacy, []);
  assert.ok(shouldShowSetup(launch, false));
  assert.deepEqual(m.calls, ["config extensions player-token.js-", `config extensions ${BRIDGE}`]);

  // closed without applying: the next launch asks again, and changes nothing
  const next = await ensure(SOURCE, { allowApply: false, paths: m.paths, run: m.run });
  assert.equal(next.needsApply, true);
  assert.deepEqual(next.actions, []);

  // Apply now
  const applied = await ensure(SOURCE, { allowApply: true, paths: m.paths, run: m.run });
  assert.equal(applied.ready, true);
  assert.equal(applied.needsApply, false);
  assert.ok(!shouldShowSetup(applied, false));
  assert.ok(!fs.existsSync(path.join(m.paths.appliedDir, "player-token.js")));
});

test("Setup opens on the first run, and later only when there is something to do", () => {
  const ok = { ready: true, needsApply: false };
  const missing = { ready: false, needsApply: false };
  assert.equal(shouldShowSetup(ok, true), true);
  assert.equal(shouldShowSetup(missing, true), true);
  assert.equal(shouldShowSetup(ok, false), false);
  // no Spicetify after the first run: the widget works without it
  assert.equal(shouldShowSetup(missing, false), false);
  assert.equal(shouldShowSetup({ ...missing, needsApply: true }, false), true);
  assert.equal(shouldShowSetup({ ...missing, error: "spicetify apply: failed" }, false), true);
});

test("macOS paths: CLI in ~/.spicetify, XDG config, Spotify's app bundle", () => {
  const p = defaultPaths({ HOME: "/Users/me", PATH: "/usr/bin:/bin" }, "darwin");
  assert.deepEqual(p, {
    cli: "/Users/me/.spicetify/spicetify",
    configDir: "/Users/me/.config/spicetify",
    spotifyExe: "/Applications/Spotify.app",
    appliedDir: "/Applications/Spotify.app/Contents/Resources/Apps/xpui/extensions",
  });
  assert.equal(defaultPaths({ HOME: "/Users/me", XDG_CONFIG_HOME: "/Users/me/cfg" }, "darwin").configDir, "/Users/me/cfg/spicetify");
});

test("Windows paths are unchanged", () => {
  const p = defaultPaths({ LOCALAPPDATA: "C:\L", APPDATA: "C:\R", PATH: "" }, "win32");
  assert.equal(p.cli, path.join("C:\L", "spicetify", "spicetify.exe"));
  assert.equal(p.appliedDir, path.join("C:\R", "Spotify", "Apps", "xpui", "extensions"));
});
