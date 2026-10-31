// The user's own Spicy Lyrics API key, and the window that gets one.
//
// Each person creates an application on the Spicy Lyrics dashboard and gets
// their own key, with its own rate limit. (A catalog template, where Add makes
// the key, is submitted for review; until it is approved, this is the way.)
// Nobody types it: "Get key" opens the dashboard in a window of its own, a
// toast on the page says what to do next, and the key is picked up the moment
// it lands on the clipboard. Settings can also take a pasted one.
//
// Stored with Electron's safeStorage (DPAPI on Windows) when it is available,
// so the file on disk is not the key itself.

const fs = require("fs");
const path = require("path");
const { app, BrowserWindow, clipboard, safeStorage, screen } = require("electron");
const { getLyrics } = require("./spicy.cjs");

/** Where a new application, and with it a key, is made. */
const KEY_PAGE_URL = "https://developers.spicylyrics.org/dashboard/applications";
const SITE_HOST = "developers.spicylyrics.org";
// A track every key can be checked against: the docs' own example.
const PROBE_TRACK = "1QV6tiMFM6fSOKOGLMHYYg";
const KEY_RE = /^sl_(pk|sk)_[A-Za-z0-9_-]{8,}$/;
const CLIPBOARD_POLL_MS = 400;
// Long enough to read "Got your key!" before the window goes.
const CLOSE_AFTER_MS = 1200;

let key = null;
let keyWin = null;

const file = () => path.join(app.getPath("userData"), "spicy-key.json");

function load() {
  try {
    const saved = JSON.parse(fs.readFileSync(file(), "utf8"));
    key = saved.encrypted
      ? safeStorage.decryptString(Buffer.from(saved.encrypted, "base64"))
      : (saved.key ?? null);
  } catch {
    key = null;
  }
  return key;
}

function store(next) {
  key = next;
  try {
    if (!next) {
      fs.rmSync(file(), { force: true });
    } else if (safeStorage.isEncryptionAvailable()) {
      const encrypted = safeStorage.encryptString(next).toString("base64");
      fs.writeFileSync(file(), JSON.stringify({ encrypted }));
    } else {
      fs.writeFileSync(file(), JSON.stringify({ key: next }));
    }
  } catch (e) {
    console.error("[spicy-key] could not save:", e.message);
  }
}

const get = () => key;

/** What Settings and Setup show: never the key itself. */
function status() {
  return { set: !!key, ending: key ? key.slice(-4) : null };
}

/**
 * Ask the API whether it takes this key. Not-found and rate-limited answers
 * still mean the key was accepted.
 * @returns {Promise<{ok: true} | {ok: false, error: string}>}
 */
async function verify(candidate) {
  if (!KEY_RE.test(candidate)) {
    return { ok: false, error: "That isn't a Spicy Lyrics key. They start with sl_sk_ or sl_pk_" };
  }
  try {
    const r = await getLyrics(PROBE_TRACK, candidate);
    if (r.httpStatus === 401 || r.httpStatus === 403) {
      return { ok: false, error: r.message ?? `Spicy Lyrics refused the key (${r.error ?? r.httpStatus}).` };
    }
    return { ok: true };
  } catch (e) {
    return { ok: false, error: `Couldn't reach Spicy Lyrics: ${e.message}` };
  }
}

/** Check, then keep, a key typed or pasted by hand. */
async function set(raw) {
  const candidate = String(raw ?? "").trim();
  const checked = await verify(candidate);
  if (checked.ok) store(candidate);
  return checked;
}

function clear() {
  store(null);
}

// Injected into the dashboard: one toast pinned to the bottom, like the
// app's own. The site's React hydration removes elements it did not render,
// so the toast puts itself back; re-run after every navigation, since a full
// load drops the timer too.
const toastScript = (text) => `(() => {
  window.__playerToastText = ${JSON.stringify(text)};
  const ensure = () => {
    let t = document.getElementById("player-key-toast");
    if (!t) {
      t = document.createElement("div");
      t.id = "player-key-toast";
      t.style.cssText = "position:fixed;left:50%;bottom:18px;transform:translateX(-50%);z-index:2147483647;" +
        "display:flex;align-items:center;gap:9px;padding:7px 14px 7px 11px;border-radius:6px;" +
        "background:#21141a;border:1px solid #3a2730;color:#ddd3d7;" +
        "font:600 13px/1.3 'Segoe UI',system-ui,sans-serif;pointer-events:none;white-space:nowrap";
      t.innerHTML = '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" ' +
        'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="2.5 8.5 6 12 13.5 4"/></svg><span></span>';
      (document.body ?? document.documentElement).appendChild(t);
    }
    const label = t.querySelector("span");
    if (label.textContent !== window.__playerToastText) label.textContent = window.__playerToastText;
  };
  ensure();
  if (!window.__playerToastTimer) window.__playerToastTimer = setInterval(ensure, 300);
})()`;

// Filled into the dashboard's "New application" form. The terms checkbox is
// left alone: agreeing is the user's to do.
const APP_NAME = "Playback";
const PROJECT_URL = "https://github.com/dev-avs";

// Fills each field once, when it appears empty, so anything the user types
// stays. React owns these inputs, so the value goes through the native setter
// and an input event, or React would put the empty value back.
const autofillScript = `(() => {
  if (window.__playerAutofill) return;
  const set = (input, value) => {
    if (!input || input.dataset.playerFilled || input.value) return;
    input.dataset.playerFilled = "1";
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const inputAfter = (labelText) => {
    const label = [...document.querySelectorAll("label")].find((l) => l.textContent.trim().toLowerCase() === labelText);
    if (!label) return null;
    return (label.htmlFor && document.getElementById(label.htmlFor)) ||
      label.querySelector("input") || label.parentElement?.querySelector("input");
  };
  window.__playerAutofill = setInterval(() => {
    set(inputAfter("name"), ${JSON.stringify(APP_NAME)});
    set(document.querySelector('input[placeholder*="github.com/owner"]') || inputAfter("project url"), ${JSON.stringify(PROJECT_URL)});
  }, 300);
})()`;

function stepFor(url) {
  try {
    const u = new URL(url);
    if (u.host !== SITE_HOST || /^\/sign-(in|up)/.test(u.pathname)) return "Let's get you signed-in first.";
  } catch {
    /* about:blank and the like */
  }
  return "Create an application, then copy the key to your clipboard";
}

/**
 * Open the dashboard and wait for a key on the clipboard.
 * @returns {Promise<{ok: true} | {ok: false, error?: string}>} ok once a key is
 *   checked and saved; not ok if the window was closed first.
 */
function connect(parent) {
  if (keyWin) {
    keyWin.focus();
    return keyWin.pending;
  }
  const near = parent && !parent.isDestroyed() ? parent.getBounds() : screen.getPrimaryDisplay().workArea;
  const area = screen.getDisplayMatching(near).workArea;
  const width = Math.min(980, area.width - 40);
  const height = Math.min(780, area.height - 40);
  keyWin = new BrowserWindow({
    x: Math.round(area.x + (area.width - width) / 2),
    y: Math.round(area.y + (area.height - height) / 2),
    width,
    height,
    title: "Get your Spicy Lyrics key",
    autoHideMenuBar: true,
    alwaysOnTop: true,
    backgroundColor: "#111014",
    webPreferences: {
      // its own session, so the sign-in is remembered for next time
      partition: "persist:spicylyrics",
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  const w = keyWin;
  // Setup sits at this level; the key page has to open above it.
  w.setAlwaysOnTop(true, "screen-saver");
  let toast = stepFor(KEY_PAGE_URL);
  const showToast = (text) => {
    if (text) toast = text;
    if (!w.isDestroyed()) w.webContents.executeJavaScript(toastScript(toast)).catch(() => {});
  };
  // Signing in lands on the site's home page, not where it started; send the
  // first landing after a sign-in back to the applications page.
  let signingIn = false;
  const follow = (_e, url) => {
    let u = null;
    try {
      u = new URL(url);
    } catch {
      /* not a page */
    }
    const onSite = u?.host === SITE_HOST;
    if (!onSite || /^\/sign-(in|up)/.test(u.pathname)) {
      signingIn = true;
    } else if (signingIn) {
      signingIn = false;
      if (!u.pathname.startsWith("/dashboard/applications")) {
        w.loadURL(KEY_PAGE_URL);
        return;
      }
    }
    showToast(stepFor(url));
  };
  w.webContents.on("did-navigate", follow);
  w.webContents.on("did-navigate-in-page", follow);
  w.webContents.on("did-finish-load", () => {
    showToast();
    w.webContents.executeJavaScript(autofillScript).catch(() => {});
  });
  // Sign-in pop-ups (Discord, Google) stay in this window rather than a bare
  // Electron window without the toast.
  w.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https:\/\//.test(url)) w.loadURL(url);
    return { action: "deny" };
  });

  w.pending = new Promise((resolve) => {
    // A key already on the clipboard before the window opened is not the new one.
    let last = clipboard.readText().trim();
    let checking = false;
    let done = false;
    const timer = setInterval(async () => {
      const text = clipboard.readText().trim();
      if (text === last || checking || done) return;
      last = text;
      if (!KEY_RE.test(text)) return;
      checking = true;
      showToast("Checking your key");
      const result = await set(text);
      checking = false;
      if (!result.ok) {
        showToast(result.error);
        return;
      }
      done = true;
      showToast("Got your key!");
      clearInterval(timer);
      resolve({ ok: true });
      setTimeout(() => !w.isDestroyed() && w.close(), CLOSE_AFTER_MS);
    }, CLIPBOARD_POLL_MS);
    w.on("closed", () => {
      clearInterval(timer);
      keyWin = null;
      if (!done) resolve({ ok: false });
    });
  });
  w.loadURL(KEY_PAGE_URL);
  return w.pending;
}

module.exports = { load, get, status, set, clear, connect };
