// Refresh coarser cached lyrics when the request version changes. Keep the
// previous payload available if authentication or the network is unavailable.
//
// With the user's own API key, the public API is asked first; the Spotify
// token route through the Spicetify bridge is only the fallback for when the
// key is missing, refused, rate limited or the API is down.

// Stored as the cache entry's clientVersion, so an API answer is not asked for
// again on every play.
const API_VERSION = "v1";
// Set from a 429's Retry-After; until then the API is skipped.
let apiBlockedUntil = 0;

async function resolveSpicyLyrics(trackId, clientLyrics, { spicy, cache, getAccessToken, apiKey, enabled = true }) {
  if (!trackId) return { status: "no-track" };

  const stored = cache.get(trackId);
  // clientLyrics is a { kind, data } tagged entry — the extension bridge sends
  // both Spicy's unpacked cache format and the packed array its API returns.
  const bridged = clientLyrics ?? null;
  // Prefer disk on a tie: it may already have been checked with today's version.
  const cached = spicy.timingTier(bridged?.data) > spicy.timingTier(stored?.data)
    ? bridged : stored ?? bridged;
  const asResponse = (entry) => ({ status: "ok", [entry.kind]: entry.data, trackId });
  const fallback = (response) => cached ? asResponse(cached) : response;

  // The finest timing needs neither a network request nor a token.
  if (cached && spicy.timingTier(cached.data) === 2) return asResponse(cached);
  if (!enabled) return fallback({ status: "disabled" });

  if (apiKey && Date.now() >= apiBlockedUntil) {
    if (cached?.clientVersion === API_VERSION) return asResponse(cached);
    try {
      const r = await spicy.getLyrics(trackId, apiKey);
      if (r.httpStatus === 200 && r.data) {
        const tier = spicy.timingTier(r.data);
        if (cached && tier < spicy.timingTier(cached.data)) return asResponse(cached);
        if (tier > 0) cache.put(trackId, "lyrics", r.data, API_VERSION);
        return { status: "ok", lyrics: r.data, trackId };
      }
      // The same catalogue the token route asks, so it has nothing either.
      if (r.httpStatus === 404) return fallback({ status: "not-found", httpStatus: 404 });
      if (r.retryAfter) apiBlockedUntil = Date.now() + r.retryAfter * 1000;
      console.error(`[spicy] API answered ${r.httpStatus}${r.error ? ` (${r.error})` : ""}; trying the bridge`);
    } catch (e) {
      console.error("[spicy] API unreachable; trying the bridge:", e.message);
    }
  }

  try {
    const token = await getAccessToken();
    if (!token) return fallback({ status: "no-auth" });
    const version = await spicy.clientVersion(token);
    if (cached?.clientVersion === version) return asResponse(cached);

    const result = await spicy.queryLyrics(trackId, token);
    if (!result) return fallback({ status: "error" });
    if (result.httpStatus === 200) {
      const tier = spicy.timingTier(result.data);
      if (cached && tier < spicy.timingTier(cached.data)) return asResponse(cached);
      if (tier > 0) cache.put(trackId, "packed", result.data, result.clientVersion);
      return { status: "ok", packed: result.data, trackId };
    }
    const status = result.httpStatus === 404 ? "not-found"
      : result.httpStatus === 503 ? "queued" : "error";
    return fallback({ status, httpStatus: result.httpStatus });
  } catch (e) {
    console.error("[spicy]", e.message);
    return fallback({ status: "error", error: e.message });
  }
}

module.exports = { resolveSpicyLyrics };
