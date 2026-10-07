// Spotify OAuth (Authorization Code + PKCE) and token storage.
//
// The Spicy Lyrics API is keyed by Spotify track id and needs a real user
// token. Only the token is minted here: the track id comes from the Spicetify
// extension, which reads it straight out of the playing client.
//
// This used to also resolve SMTC's title/artist to an id through the Web API
// (Search plus currently-playing). That was deleted: the bridged id is exact,
// free and cannot be rate limited, and the Web API path had never once produced
// a result — its on-disk id cache was never written. See bridgedIdFor in
// main.cjs.

const http = require("http");
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");
const { app, shell } = require("electron");

const REDIRECT_PORT = 8888;
const REDIRECT_URI = `http://127.0.0.1:${REDIRECT_PORT}/callback`;
// Enough to mint a user-identified token, which is all the Spicy Lyrics API
// needs. No playback scopes: nothing here reads playback state any more.
const SCOPES = "user-read-private";

let tokens = null; // { access_token, refresh_token, expires_at }
let manual = null; // { access_token, expires_at } — pasted by hand
let authInFlight = null;

/**
 * Spotify access tokens are JWT-shaped, so the expiry can be read straight off
 * the payload without verifying the signature (we are not trusting it for
 * anything — it only drives the "expires in N min" hint and lets us discard a
 * dead token before spending a request on it).
 */
function readJwtExpiry(token) {
  try {
    const payload = token.split(".")[1];
    if (!payload) return null;
    const json = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
    return typeof json.exp === "number" ? json.exp * 1000 : null;
  } catch {
    return null;
  }
}

let manualLoadLogged = false;

/**
 * How long to trust a token whose expiry we cannot read.
 *
 * A hand-pasted token is opaque rather than JWT-shaped, so readJwtExpiry finds
 * nothing and the record is stored with expires_at: null. Treating that as
 * "never expires" is what let a token spent hours ago stay in use: every
 * api.spotify.com call answered 401, no track id was ever resolved, Spicy
 * Lyrics was skipped for every track, and isSignedIn() still reported true — so
 * the UI never offered to take a new one. Spotify's web tokens last an hour;
 * aging an opaque one from when it was saved makes it lapse on its own and
 * brings back the "Paste Spotify token" prompt.
 */
const OPAQUE_TOKEN_TTL_MS = 55 * 60_000;

function loadManual() {
  if (manual !== null) return manual;
  try {
    const file = configPath("spotify-manual.json");
    manual = JSON.parse(fs.readFileSync(file, "utf8"));
    // Records written before this had a TTL carry expires_at: null. Age them
    // from the file's own mtime rather than migrating anything.
    if (manual && manual.expires_at == null) {
      manual.expires_at = fs.statSync(file).mtimeMs + OPAQUE_TOKEN_TTL_MS;
    }
    if (!manualLoadLogged) {
      manualLoadLogged = true;
      console.log("[spotify] loaded saved token from disk");
    }
  } catch (e) {
    if (!manualLoadLogged) {
      manualLoadLogged = true;
      console.log(
        e.code === "ENOENT"
          ? "[spotify] no saved token on disk"
          : `[spotify] saved token unreadable: ${e.message}`,
      );
    }
    manual = null;
  }
  return manual;
}

/**
 * @param raw the access token
 * @param knownExpiry epoch ms the token really dies, when the caller knows it.
 *   The Spotify client hands its own expiry out alongside the token, so a
 *   bridged token needs none of the guesswork an opaque pasted one does.
 */
function setManualToken(raw, knownExpiry) {
  const token = String(raw ?? "")
    .trim()
    .replace(/^Bearer\s+/i, "");
  if (!token) return { ok: false, error: "empty token" };

  // A Spotify token is long and opaque; catch obvious paste mistakes early
  // rather than letting them fail as a confusing 401 later.
  if (token.length < 40 || /\s/.test(token)) {
    return { ok: false, error: "that does not look like an access token" };
  }

  const jwtExpiry = readJwtExpiry(token);
  if (jwtExpiry && jwtExpiry < Date.now()) {
    return { ok: false, error: "that token has already expired" };
  }
  // Expiries arrive as epoch ms, but some Spotify builds report seconds. A
  // seconds value is a 1970 timestamp when read as ms, so it looks long expired
  // and the token is thrown away — with the extension retrying forever against
  // a rejection it never surfaced. Normalise before comparing.
  const reportedRaw = Number(knownExpiry);
  const normalised =
    Number.isFinite(reportedRaw) && reportedRaw > 0
      ? reportedRaw < 1e12
        ? reportedRaw * 1000
        : reportedRaw
      : NaN;
  const reportedExpiry = normalised > Date.now() ? normalised : null;
  if (knownExpiry != null && !reportedExpiry) {
    return { ok: false, error: "that token has already expired" };
  }
  // The extension re-sends the same token every few seconds. Rewriting the file
  // and logging each time is pure noise while the saved copy is still good.
  const current = loadManual();
  if (current?.access_token === token && current.expires_at > Date.now() + 5 * 60_000) {
    return { ok: true, expiresAt: current.expires_at };
  }

  // Opaque tokens get the conservative TTL — see OPAQUE_TOKEN_TTL_MS.
  const expiresAt = reportedExpiry ?? jwtExpiry ?? Date.now() + OPAQUE_TOKEN_TTL_MS;

  manual = { access_token: token, expires_at: expiresAt };
  const file = configPath("spotify-manual.json");
  try {
    fs.writeFileSync(file, JSON.stringify(manual), { mode: 0o600 });
    // Read back rather than assume: a silent persistence failure is what makes
    // a token appear to work until the next restart.
    const check = JSON.parse(fs.readFileSync(file, "utf8"));
    if (check.access_token !== token) throw new Error("verify-after-write mismatch");
    console.log(
      `[spotify] token saved (expires ${new Date(expiresAt).toISOString()}` +
        `${jwtExpiry ? "" : ", assumed — token is opaque"})`,
    );
  } catch (e) {
    console.error("[spotify] COULD NOT PERSIST TOKEN:", e.message);
    return { ok: false, error: `could not save token: ${e.message}` };
  }
  return { ok: true, expiresAt };
}

function clearManualToken() {
  manual = null;
  manualLoadLogged = false;
  try {
    fs.unlinkSync(configPath("spotify-manual.json"));
    console.log("[spotify] saved token cleared");
  } catch {
    /* already gone */
  }
}

function configPath(name) {
  return path.join(app.getPath("userData"), name);
}

// Client id comes from the user's own Spotify dashboard app. Env var wins so it
// can be injected in dev without writing it to disk.
function clientId() {
  if (process.env.SPOTIFY_CLIENT_ID) return process.env.SPOTIFY_CLIENT_ID.trim();
  try {
    const raw = fs.readFileSync(configPath("spotify.json"), "utf8");
    const id = JSON.parse(raw).clientId;
    return id ? String(id).trim() : null;
  } catch {
    return null;
  }
}

function loadTokens() {
  if (tokens) return tokens;
  try {
    tokens = JSON.parse(fs.readFileSync(configPath("spotify-tokens.json"), "utf8"));
  } catch {
    tokens = null;
  }
  return tokens;
}

function saveTokens(next) {
  tokens = next;
  try {
    fs.writeFileSync(configPath("spotify-tokens.json"), JSON.stringify(next), {
      mode: 0o600,
    });
  } catch (e) {
    console.error("[spotify] could not persist tokens:", e.message);
  }
}

function b64url(buf) {
  return buf.toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function exchange(params) {
  const res = await fetch("https://accounts.spotify.com/api/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(params),
  });
  if (!res.ok) {
    throw new Error(`token endpoint ${res.status}: ${(await res.text()).slice(0, 200)}`);
  }
  const j = await res.json();
  return {
    access_token: j.access_token,
    // a refresh response may omit refresh_token; keep the existing one
    refresh_token: j.refresh_token ?? tokens?.refresh_token ?? null,
    expires_at: Date.now() + (j.expires_in ?? 3600) * 1000 - 60_000,
  };
}

/**
 * Runs the interactive login. Opens the consent page in the user's real
 * browser (never an embedded window — that trains people to type credentials
 * into an app-controlled webview) and waits for the loopback redirect.
 */
function login() {
  if (authInFlight) return authInFlight;

  const id = clientId();
  if (!id) {
    return Promise.reject(
      new Error(
        "No Spotify client id. Create an app at developer.spotify.com, add " +
          `${REDIRECT_URI} as a redirect URI, then set SPOTIFY_CLIENT_ID or ` +
          "write {\"clientId\":\"...\"} to spotify.json in userData.",
      ),
    );
  }

  const verifier = b64url(crypto.randomBytes(64));
  const challenge = b64url(crypto.createHash("sha256").update(verifier).digest());
  const state = b64url(crypto.randomBytes(16));

  authInFlight = new Promise((resolve, reject) => {
    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url, REDIRECT_URI);
      if (url.pathname !== "/callback") {
        res.writeHead(404).end();
        return;
      }

      const reply = (msg) => {
        res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
        res.end(
          `<!doctype html><meta charset="utf-8"><body style="font:15px system-ui;padding:40px">${msg}</body>`,
        );
      };

      try {
        if (url.searchParams.get("state") !== state) throw new Error("state mismatch");
        const err = url.searchParams.get("error");
        if (err) throw new Error(err);
        const code = url.searchParams.get("code");
        if (!code) throw new Error("no code in redirect");

        saveTokens(
          await exchange({
            grant_type: "authorization_code",
            code,
            redirect_uri: REDIRECT_URI,
            client_id: id,
            code_verifier: verifier,
          }),
        );
        reply("Signed in. You can close this tab and go back to the player.");
        resolve(true);
      } catch (e) {
        reply(`Sign-in failed: ${e.message}`);
        reject(e);
      } finally {
        server.close();
        authInFlight = null;
      }
    });

    server.on("error", (e) => {
      authInFlight = null;
      reject(e);
    });

    server.listen(REDIRECT_PORT, "127.0.0.1", () => {
      const authUrl =
        "https://accounts.spotify.com/authorize?" +
        new URLSearchParams({
          response_type: "code",
          client_id: id,
          scope: SCOPES,
          redirect_uri: REDIRECT_URI,
          state,
          code_challenge_method: "S256",
          code_challenge: challenge,
        });
      shell.openExternal(authUrl);
    });

    // don't leave the loopback listener open forever
    setTimeout(
      () => {
        if (authInFlight) {
          server.close();
          authInFlight = null;
          reject(new Error("sign-in timed out"));
        }
      },
      5 * 60_000,
    );
  });

  return authInFlight;
}

/** A valid access token, refreshing as needed. Null when not signed in. */
async function getAccessToken() {
  // A hand-pasted token wins: it comes from a real Spotify session, which is
  // what the Spicy Lyrics API is built around.
  const m = loadManual();
  if (m?.access_token) {
    if (!m.expires_at || Date.now() < m.expires_at) return m.access_token;
    clearManualToken(); // expired — fall through to PKCE if configured
  }

  const t = loadTokens();
  if (!t) return null;
  if (t.access_token && Date.now() < (t.expires_at ?? 0)) return t.access_token;
  if (!t.refresh_token) return null;

  const id = clientId();
  if (!id) return null;
  try {
    const next = await exchange({
      grant_type: "refresh_token",
      refresh_token: t.refresh_token,
      client_id: id,
    });
    saveTokens(next);
    return next.access_token;
  } catch (e) {
    console.error("[spotify] refresh failed:", e.message);
    return null;
  }
}

function isConfigured() {
  return !!clientId();
}

function isSignedIn() {
  const m = loadManual();
  if (m?.access_token && (!m.expires_at || Date.now() < m.expires_at)) return true;
  return !!loadTokens()?.refresh_token;
}

/** When the pasted token dies, in epoch ms. Null if none or unknown. */
function manualExpiry() {
  return loadManual()?.expires_at ?? null;
}

function signOut() {
  tokens = null;
  try {
    fs.unlinkSync(configPath("spotify-tokens.json"));
  } catch {
    /* already gone */
  }
  // a hand-pasted token is the other half of "signed in", so drop it too
  clearManualToken();
}


module.exports = {
  login,
  signOut,
  getAccessToken,
  isConfigured,
  isSignedIn,
  setManualToken,
  clearManualToken,
  manualExpiry,
  REDIRECT_URI,
};
