// Spicy Lyrics API client.
//
// Mirrors src/utils/API/Query.ts from the spicy-lyrics project. Two details
// differ from how the API is usually described, and both were taken from that
// source rather than from prose:
//   * the token goes in a `SpicyLyrics-WebAuth` header, not `Authorization`
//   * the client does not send `operationId`; the server assigns it, and it
//     comes back as the *string* "0"
//
// Only the HTTP hop lives here. The response body is binary-packed and is
// unpacked in the renderer (src/spicy.ts), the same split we already use for
// LRC: the main process exists to escape CORS, not to own parsing.

const API_URL = "https://api.spicylyrics.org";

/**
 * The origin their extension runs on, and the one thing that decides which
 * tier the API serves.
 *
 * Without Origin the API has returned untimed lyrics. The client version and
 * the user's Spotify token also affect which timing data the server returns.
 *
 * A browser sets Origin itself and page code cannot change it, so this check
 * only ever filters browsers on other origins — Node sends none at all, which
 * is why this player was being served the untrusted tier. Used here with the
 * project owner's explicit permission for third-party clients; if they would
 * rather callers identify themselves some other way, this is the line to
 * change.
 */
const CLIENT_ORIGIN = "https://xpui.app.spotify.com";

/**
 * The client version, which decides the *tier* the server serves.
 *
 * This is not cosmetic and it is not only about being recognised. Measured:
 * with a valid token and the right Origin, sending 6.2.3 or 6.3.1 returns
 * `Type: "Static"` (246 bytes, untimed) for every track, while 6.3.12 returns
 * full syllables for the same ids. The server has raised its floor at least
 * once, and a hardcoded version silently decays into "no lyrics anywhere" —
 * which is exactly how this broke, months after it was written, with nothing in
 * the app indicating why.
 *
 * So the version is asked for rather than assumed. Their `ext_version`
 * operation answers with the current value even when the
 * request itself carries a stale one, which is what makes this recoverable
 * without a release. The constant below is only the bootstrap, used until the
 * first answer arrives and whenever the lookup fails.
 */
const FALLBACK_VERSION = "6.3.20";
const VERSION_TTL_MS = 15 * 60_000;
const VERSION_RETRY_MS = 60_000;

let cachedVersion = null;
let cachedVersionAt = 0;
let versionRequest = null;
let retryVersionAt = 0;

function baseHeaders(version) {
  return {
    "Content-Type": "application/json",
    "SpicyLyrics-Version": version,
    "X-mode": "2",
    Origin: CLIENT_ORIGIN,
  };
}

/** Ask the server what it considers current; reject failed or malformed replies. */
async function fetchServerVersion(accessToken) {
  const version = cachedVersion ?? FALLBACK_VERSION;
  const res = await fetch(`${API_URL}/query`, {
    method: "POST",
    headers: {
      ...baseHeaders(version),
      ...(accessToken ? { "SpicyLyrics-WebAuth": `Bearer ${accessToken}` } : {}),
    },
    body: JSON.stringify({
      queries: [{ operation: "ext_version" }],
      client: { version },
    }),
    signal: AbortSignal.timeout(5_000),
  });
  if (!res.ok) throw new Error(`version lookup HTTP ${res.status}`);
  const job = ((await res.json())?.queries ?? []).find((q) => String(q?.operationId) === "0");
  if (job?.result?.httpStatus !== 200) throw new Error("version lookup returned no successful result");
  const text = job?.result?.data;
  // Their own ParseVersion shape. Anything else is not a version and is ignored
  // rather than sent as a header that would earn a flat 400.
  const match = typeof text === "string" ? text.match(/\d+\.\d+\.\d+/) : null;
  if (!match) throw new Error("version lookup returned an invalid version");
  return match[0];
}

/**
 * The version to send. Refreshed after 15 minutes of use, and when a reply
 * comes back without syllables — an older version can receive line-only data.
 */
async function clientVersion(accessToken, force = false) {
  const fresh = cachedVersion && Date.now() - cachedVersionAt < VERSION_TTL_MS;
  if (fresh && !force) return cachedVersion;
  if (versionRequest) return versionRequest;
  if (Date.now() < retryVersionAt) return cachedVersion ?? FALLBACK_VERSION;
  versionRequest = (async () => {
    try {
      const found = await fetchServerVersion(accessToken);
      if (found !== cachedVersion) {
        console.log(`[spicy] client version -> ${found}`);
      }
      cachedVersion = found;
      cachedVersionAt = Date.now();
      retryVersionAt = 0;
    } catch (e) {
      retryVersionAt = Date.now() + VERSION_RETRY_MS;
      console.error("[spicy] could not read the current client version:", e.message);
    }
    return cachedVersion ?? FALLBACK_VERSION;
  })();
  try {
    return await versionRequest;
  } finally {
    versionRequest = null;
  }
}

/** Works for both packed API data and the bridge's unpacked lyrics. */
function timingTier(data) {
  if (data == null) return 0;
  const s = JSON.stringify(data);
  return s.includes('"Syllable"') ? 2 : s.includes('"Line"') ? 1 : 0;
}

async function requestLyrics(trackId, accessToken, version) {
  return fetch(`${API_URL}/query`, {
    method: "POST",
    headers: {
      ...baseHeaders(version),
      "SpicyLyrics-WebAuth": `Bearer ${accessToken}`,
    },
    body: JSON.stringify({
      queries: [
        {
          operation: "lyrics",
          variables: { id: trackId, auth: "SpicyLyrics-WebAuth" },
        },
      ],
      client: { version },
    }),
    signal: AbortSignal.timeout(15_000),
  });
}

/**
 * @returns {Promise<{httpStatus: number, data: unknown, clientVersion: string} | null>}
 *   httpStatus 200 = found, 404 = none, 503 = queued server-side.
 */
async function queryLyrics(trackId, accessToken) {
  let version = await clientVersion(accessToken);
  let res = await requestLyrics(trackId, accessToken, version);

  // An outdated version can return Line as well as Static. Check for an update
  // in either case, and retry lyrics only once and only if the version changed.
  if (res.ok) {
    const peek = await res.clone().json().catch(() => null);
    const job = (peek?.queries ?? []).find((q) => String(q.operationId) === "0");
    if (job?.result?.httpStatus === 200 && timingTier(job.result.data) < 2) {
      const refreshed = await clientVersion(accessToken, true);
      if (refreshed !== version) {
        console.log(`[spicy] no syllables on ${version} — retrying as ${refreshed}`);
        try {
          const retry = await requestLyrics(trackId, accessToken, refreshed);
          const body = retry.ok ? await retry.clone().json().catch(() => null) : null;
          const result = body?.queries?.find((q) => String(q.operationId) === "0")?.result;
          // A failed upgrade must not discard usable lyrics from the first reply.
          if (result?.httpStatus === 200 && timingTier(result.data) >= timingTier(job.result.data)) {
            version = refreshed;
            res = retry;
          }
        } catch (e) {
          console.error("[spicy] version retry failed; keeping original lyrics:", e.message);
        }
      }
    }
  }

  if (!res.ok) {
    console.error(`[spicy] query failed: HTTP ${res.status}`);
    return null;
  }

  const body = await res.json();
  const jobs = body?.queries ?? [];
  const job = jobs.find((q) => String(q.operationId) === "0");

  if (!job?.result) {
    // An unauthenticated request still returns 200, but the job is dropped and
    // the array holds nothing but the server's `_notice`.
    const onlyNotice = jobs.length > 0 && jobs.every((q) => q?._notice && !q?.result);
    console.error(
      onlyNotice
        ? "[spicy] request rejected — the Spotify token was missing or not accepted"
        : "[spicy] no result for operationId 0",
    );
    return null;
  }

  return { httpStatus: job.result.httpStatus, data: job.result.data, clientVersion: version };
}

module.exports = { queryLyrics, clientVersion, timingTier };
