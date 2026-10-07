// Receives a Spotify access token from the Spicetify extension.
//
// This is how spicy-lyrics itself gets a token: it runs inside the Spotify
// client and asks Spotify's own resolver (`sp://oauth/v2/token`, see
// spicy-lyrics/src/components/Global/Platform.ts). There is no OAuth app, no
// client id and no pasting — the client already holds a session, and an
// extension can simply read it.
//
// We are a separate process, so we cannot call `sp://` ourselves. The companion
// extension in `spicetify/player-token.js` does it inside Spotify and posts the
// result here. It also knows the real expiry, so the token never has to be aged
// by guesswork the way a hand-pasted one does.
//
// Loopback only, and there is deliberately no route that hands a token back
// out: this accepts credentials, it never serves them. The one GET, /lookup,
// hands out the title and artist of a song playing in another app, for the
// extension to find on Spotify (see spotifyLookup.cjs) — and only to Spotify's
// own page, so no website can read what is playing.

const http = require("http");

const PORT = 8899;
const SPOTIFY_ORIGIN = "https://xpui.app.spotify.com";
// A token push is tiny, but a captured lyrics payload is not.
const MAX_BODY = 4 * 1024 * 1024;

/**
 * @param onToken (token, expiresAt, track) => { ok: boolean, error?: string }
 *   `track` is { id, name, artists } straight from the Spotify client, or null.
 * @param lookups { pending: () => [{ title, artist }], resolve: (payload) => void }
 *   `payload` is { title, artist, candidates, error? } from the extension.
 */
function startTokenBridge(onToken, onSpicyCapture, lookups) {
  const server = http.createServer((req, res) => {
    // The extension runs on a Spotify origin, so the browser treats this as
    // cross-origin. The POST itself is sent as text/plain to stay a "simple"
    // request and skip preflight, but answer OPTIONS anyway in case a future
    // Spotify build tightens that.
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    res.setHeader("Access-Control-Allow-Methods", "POST, OPTIONS");

    if (req.method === "OPTIONS") {
      res.writeHead(204).end();
      return;
    }
    const route = (req.url ?? "").split("?")[0];
    if (req.method === "GET" && route === "/lookup") {
      res.setHeader("Access-Control-Allow-Origin", SPOTIFY_ORIGIN);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ lookups: lookups?.pending() ?? [] }));
      return;
    }
    if (req.method !== "POST" || !["/token", "/spicy", "/resolved"].includes(route)) {
      res.writeHead(404).end();
      return;
    }

    let body = "";
    let aborted = false;
    req.on("data", (chunk) => {
      body += chunk;
      if (body.length > MAX_BODY) {
        aborted = true;
        res.writeHead(413).end();
        req.destroy();
      }
    });
    req.on("end", () => {
      if (aborted) return;
      try {
        const payload = JSON.parse(body);
        if (route === "/resolved") {
          lookups?.resolve(payload);
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end('{"ok":true}');
          return;
        }
        if (route === "/spicy") {
          const r = onSpicyCapture?.(payload) ?? { ok: true };
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(r));
          return;
        }
        const { token, expiresAt, track } = payload;
        const result = onToken(token, expiresAt, track) ?? {
          ok: false,
          error: "not handled",
        };
        res.writeHead(result.ok ? 200 : 400, { "Content-Type": "application/json" });
        res.end(JSON.stringify(result));
      } catch (e) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: false, error: e.message }));
      }
    });
  });

  server.on("error", (e) => {
    // EADDRINUSE usually means a second copy of the player is already running;
    // that copy owns the bridge and this one still reads the same token file.
    console.error(`[bridge] could not listen on ${PORT}: ${e.message}`);
  });

  server.listen(PORT, "127.0.0.1", () => {
    console.log(`[bridge] listening for the Spicetify extension on 127.0.0.1:${PORT}`);
  });

  return server;
}

module.exports = { startTokenBridge, PORT };
