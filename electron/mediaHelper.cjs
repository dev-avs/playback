// Which helper process reports what is playing, per platform.
//
// Every helper speaks the same protocol, so main.cjs does not care which one
// runs: one JSON object per line on stdout (see SmtcEvent in src/player.d.ts),
// one command per line on stdin (`play`, `pause`, `playpause`, `next`, `prev`,
// `seek <sec>`, `volume <0..1>`, `shuffle`, `repeat`).
//
// Windows: smtc.ps1 under Windows PowerShell 5.1 (PowerShell 7 has no WinRT
// projection). macOS has no helper yet; see mac-port.md.

const path = require("path");

/**
 * The command that starts this platform's helper, or null when there is none.
 * @param {{ platform?: string, isPackaged: boolean, resourcesPath?: string, dir: string }} where
 *   `dir` is the electron/ folder (main.cjs's __dirname)
 */
function helperCommand({ platform = process.platform, isPackaged, resourcesPath, dir }) {
  if (platform === "win32") {
    // powershell.exe -File needs a real path on disk. In a packaged build
    // `dir` points inside app.asar, which Electron can read but PowerShell
    // cannot — the spawn would fail and the widget would sit on the idle
    // placeholder forever. smtc.ps1 is listed in build.asarUnpack so it is
    // written out beside the archive; point at that copy instead.
    const script = isPackaged
      ? path.join(resourcesPath, "app.asar.unpacked", "electron", "smtc.ps1")
      : path.join(dir, "smtc.ps1");
    return {
      command: "powershell.exe",
      args: ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script],
    };
  }
  return null;
}

module.exports = { helperCommand };
