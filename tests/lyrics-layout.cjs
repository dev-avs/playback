// Real renderer regression: build first, then run with Electron. Uses a hidden
// window, isolated temporary profile, and synthetic player events/lyrics.
const path = require("node:path");

if (process.type === "renderer") {
  let listener;
  const send = (trackKey, position = 1, count = 12) => {
    window.testLineCount = count;
    listener?.({ type: "state", appId: "spotify.exe", title: trackKey,
      artist: "Layout test", trackKey, position, duration: 180, playing: false,
      caps: { seek: true }, updatedAt: Date.now() });
  };
  window.testTrack = send;
  window.player = {
    onEvent(cb) { listener = cb; setTimeout(() => send("Opening"), 0); return () => { listener = null; }; },
    fetchSpicyLyrics: async () => ({ status: "ok", lyrics: {
      Type: "Line", Content: Array.from({ length: window.testLineCount }, (_, i) => ({
        Text: i === 0 ? "Opening words should stay clearly visible" : `Lyric line ${i + 1}`,
        StartTime: i * 10, EndTime: (i + 1) * 10,
      })),
    } }),
    fetchLyrics: async () => null,
    fetchArt: async () => null,
    fetchRomanized: async () => null,
    spotifyStatus: async () => ({ signedIn: true, configured: false }),
    resizeTo() {}, seek() {}, dragStart() {}, dragEnd() {},
  };
} else {
  const { app, BrowserWindow, session } = require("electron");
  const fs = require("node:fs");
  const os = require("node:os");
  const assert = require("node:assert/strict");
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "player-layout-test-"));
  app.setPath("userData", profile);
  app.disableHardwareAcceleration();
  let win;
  app.whenReady().then(async () => {
    session.defaultSession.webRequest.onBeforeRequest({ urls: ["https://*/*", "http://*/*"] }, (_details, done) => done({ cancel: true }));
    win = new BrowserWindow({ show: false, width: 440, height: 650,
      webPreferences: { preload: __filename, contextIsolation: false, sandbox: false, backgroundThrottling: false, offscreen: true } });
    win.webContents.on("console-message", ({ message }) => {
      if (String(message).includes("Error")) console.log(message);
    });
    await win.loadFile(path.join(__dirname, "../dist/index.html"));
    const run = code => win.webContents.executeJavaScript(code);
    const settle = () => run(`new Promise(resolve => setTimeout(resolve, 650))`);
    await settle();
    await run(`document.querySelector('[aria-label="Lyrics"]').click()`);
    await settle();
    async function check(label) {
      const box = await run(`(() => {
        const panelElement = document.querySelector('.lyrics-panel');
        const panel = panelElement.getBoundingClientRect();
        const line = document.querySelector('.lyric-line.current:not(.lyric-gap), .lyric-line.intro-preview');
        if (!line) throw new Error('No active or opening lyric');
        const rect = line.getBoundingClientRect();
        return { top: rect.top - panel.top, bottom: rect.bottom - panel.top, height: panel.height,
          classes: document.querySelector('.widget').className };
      })()`);
      console.log(label, JSON.stringify(box));
      assert.ok(box.top >= 0, `${label}: lyric is clipped at the top`);
      assert.ok(box.top <= 12, `${label}: too much empty space above the lyric`);
      assert.ok(box.bottom <= box.height * 0.82, `${label}: lyric is inside the bottom fade`);
    }
    await check("first line");
    await run(`window.testTrack('Opening', 111)`);
    await settle();
    await check("last line");
    await run(`window.testTrack('New opening', 1)`);
    await settle();
    await check("track change to first line");
    await run(`document.querySelector('[aria-label="Lyrics"]').click()`);
    await settle();
    await run(`window.testTrack('Short song', 1, 1)`);
    await settle();
    await run(`document.querySelector('[aria-label="Lyrics"]').click()`);
    await settle();
    await check("reopened one-line song");
    console.log("PASS: lyric positioning");
  }).then(() => app.exit(0)).catch(error => {
    console.error(error);
    app.exit(1);
  });
  app.on("will-quit", () => {
    // Electron may retain profile handles until process exit; cleanup is best effort.
    if (path.dirname(profile) === path.resolve(os.tmpdir()) && path.basename(profile).startsWith("player-layout-test-")) {
      try { fs.rmSync(profile, { recursive: true, force: true }); } catch { /* profile still open */ }
    }
  });
}
