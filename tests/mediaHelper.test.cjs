const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { helperCommand } = require("../electron/mediaHelper.cjs");

test("Windows runs smtc.ps1 under Windows PowerShell, from the folder in development", () => {
  const h = helperCommand({ platform: "win32", isPackaged: false, dir: "C:\app\electron" });
  assert.equal(h.command, "powershell.exe");
  assert.deepEqual(h.args, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", path.join("C:\app\electron", "smtc.ps1")]);
});

test("Windows, packaged: the unpacked copy beside app.asar", () => {
  const h = helperCommand({ platform: "win32", isPackaged: true, resourcesPath: "C:\inst\resources", dir: "C:\inst\resources\app.asar\electron" });
  assert.equal(h.args.at(-1), path.join("C:\inst\resources", "app.asar.unpacked", "electron", "smtc.ps1"));
});

test("no helper yet on other platforms", () => {
  assert.equal(helperCommand({ platform: "darwin", isPackaged: false, dir: "/app/electron" }), null);
  assert.equal(helperCommand({ platform: "linux", isPackaged: false, dir: "/app/electron" }), null);
});
