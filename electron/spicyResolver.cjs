// Refresh coarser cached lyrics when the request version changes. Keep the
// previous payload available if authentication or the network is unavailable.
async function resolveSpicyLyrics(trackId, clientLyrics, { spicy, cache, getAccessToken, enabled = true }) {
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
