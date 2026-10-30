const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

function setup(t, respond) {
  const originalFetch = global.fetch;
  const requests = [];
  global.fetch = async (_url, options) => {
    const body = JSON.parse(options.body);
    requests.push({ ...body, headers: options.headers, signal: options.signal });
    return respond(body, options);
  };
  delete require.cache[require.resolve("../electron/spicy.cjs")];
  const spicy = require("../electron/spicy.cjs");
  t.after(() => { global.fetch = originalFetch; });
  return { spicy, requests };
}

function reply(data, httpStatus = 200) {
  return Response.json({ queries: [{ operationId: "0", result: { httpStatus, data } }] });
}

// Representative objpack value tables: the main process checks timing markers,
// while the renderer remains responsible for decoding the stream.
const syllable = [["Type", "Syllable"], []];
const line = [["Type", "Line"], []];
const untimed = [["Type", "Static"], []];

test("discovers and sends the current version in both request fields", async (t) => {
  const { spicy, requests } = setup(t, (body) =>
    reply(body.queries[0].operation === "ext_version" ? "6.3.21" : syllable));
  const result = await spicy.queryLyrics("track", "test-token");
  assert.equal(requests[1].headers["SpicyLyrics-Version"], "6.3.21");
  assert.equal(requests[1].client.version, "6.3.21");
  assert.equal(requests[0].headers["SpicyLyrics-WebAuth"], "Bearer test-token");
  assert.ok(requests.every(r => r.signal instanceof AbortSignal));
  assert.equal(result.clientVersion, "6.3.21");
});

test("a line-only response rechecks the version and upgrades to syllables", async (t) => {
  let lookups = 0;
  const { spicy, requests } = setup(t, (body) => {
    if (body.queries[0].operation === "ext_version") {
      return reply(++lookups === 1 ? "6.3.20" : "6.3.21");
    }
    return reply(body.client.version === "6.3.20" ? line : syllable);
  });
  assert.deepEqual((await spicy.queryLyrics("track", "test-token")).data, syllable);
  assert.equal(requests.length, 4);
});

test("genuine line lyrics do not loop when the version is unchanged", async (t) => {
  const { spicy, requests } = setup(t, body =>
    reply(body.queries[0].operation === "ext_version" ? "6.3.21" : line));
  assert.deepEqual((await spicy.queryLyrics("track", "test-token")).data, line);
  assert.equal(requests.filter(r => r.queries[0].operation === "lyrics").length, 1);
});

test("a failed version lookup keeps the last known version", async (t) => {
  let lookups = 0;
  const { spicy, requests } = setup(t, body => {
    if (body.queries[0].operation === "ext_version") {
      if (++lookups > 1) throw new Error("offline");
      return reply("6.3.21");
    }
    return reply(untimed);
  });
  const result = await spicy.queryLyrics("track", "test-token");
  assert.equal(result.clientVersion, "6.3.21");
  assert.equal(requests.filter(r => r.queries[0].operation === "lyrics").length, 1);
});

test("concurrent lyric requests share the version lookup", async (t) => {
  const { spicy, requests } = setup(t, async body => {
    await new Promise(resolve => setImmediate(resolve));
    return reply(body.queries[0].operation === "ext_version" ? "6.3.21" : syllable);
  });
  await Promise.all([spicy.queryLyrics("one", "test-token"), spicy.queryLyrics("two", "test-token")]);
  assert.equal(requests.filter(r => r.queries[0].operation === "ext_version").length, 1);
});

test("the current version is refreshed after 15 minutes", async (t) => {
  let now = Date.now();
  t.mock.method(Date, "now", () => now);
  let latest = "6.3.20";
  const { spicy, requests } = setup(t, body =>
    reply(body.queries[0].operation === "ext_version" ? latest : syllable));
  await spicy.queryLyrics("first", "test-token");
  latest = "6.3.21";
  now += 16 * 60_000;
  const result = await spicy.queryLyrics("second", "test-token");
  assert.equal(result.clientVersion, "6.3.21");
  assert.equal(requests.filter(r => r.queries[0].operation === "ext_version").length, 2);
});

for (const failure of ["network", "HTTP", "queued", "untimed"]) {
  test(`a ${failure} version retry preserves the first usable response`, async (t) => {
    let lookups = 0;
    const { spicy } = setup(t, body => {
      if (body.queries[0].operation === "ext_version") return reply(++lookups === 1 ? "6.3.20" : "6.3.21");
      if (body.client.version === "6.3.20") return reply(line);
      if (failure === "network") throw new Error("offline");
      if (failure === "HTTP") return new Response(null, { status: 503 });
      return failure === "queued" ? reply(null, 503) : reply(untimed);
    });
    const result = await spicy.queryLyrics("track", "test-token");
    assert.deepEqual(result?.data, line);
    assert.equal(result.clientVersion, "6.3.20");
  });
}

function cacheSetup(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "player-spicy-test-"));
  const cache = require("../electron/lyricsCache.cjs");
  cache.init(dir);
  t.after(() => {
    // Only remove the directory this test created, beneath the temporary root.
    assert.equal(path.dirname(dir), path.resolve(os.tmpdir()));
    assert.ok(path.basename(dir).startsWith("player-spicy-test-"));
    fs.rmSync(dir, { recursive: true, force: true });
  });
  return cache;
}

test("disk cache preserves the request version across a reload", (t) => {
  const cache = cacheSetup(t);
  cache.put("track", "packed", line, "6.3.21");
  assert.equal(cache.get("track").clientVersion, "6.3.21");
});

test("legacy line cache upgrades without deleting it first", async (t) => {
  const { spicy } = setup(t, body =>
    reply(body.queries[0].operation === "ext_version" ? "6.3.21" : syllable));
  const cache = cacheSetup(t);
  cache.put("track", "packed", line);
  const { resolveSpicyLyrics } = require("../electron/spicyResolver.cjs");
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, getAccessToken: async () => "test-token",
  });
  assert.deepEqual(result.packed, syllable);
  assert.equal(cache.get("track").clientVersion, "6.3.21");
});

test("line cache from an older version refreshes, current line cache is reused", async (t) => {
  const { spicy, requests } = setup(t, body =>
    reply(body.queries[0].operation === "ext_version" ? "6.3.21" : line));
  const cache = cacheSetup(t);
  cache.put("track", "packed", line, "6.3.20");
  const { resolveSpicyLyrics } = require("../electron/spicyResolver.cjs");
  const options = { spicy, cache, getAccessToken: async () => "test-token" };
  await resolveSpicyLyrics("track", null, options);
  await resolveSpicyLyrics("track", null, options);
  assert.equal(requests.filter(r => r.queries[0].operation === "lyrics").length, 1);
  assert.equal(cache.get("track").clientVersion, "6.3.21");
});

test("network failure preserves cached lyrics", async (t) => {
  const { spicy } = setup(t, () => { throw new Error("offline"); });
  const cache = cacheSetup(t);
  cache.put("track", "packed", line);
  const { resolveSpicyLyrics } = require("../electron/spicyResolver.cjs");
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, getAccessToken: async () => "test-token",
  });
  assert.deepEqual(result.packed, line);
  assert.deepEqual(cache.get("track").data, line);
});

test("syllable cache works without a token or a version request", async (t) => {
  const { spicy, requests } = setup(t, () => { throw new Error("unexpected network"); });
  const cache = cacheSetup(t);
  cache.put("track", "packed", syllable);
  const { resolveSpicyLyrics } = require("../electron/spicyResolver.cjs");
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, getAccessToken: async () => { throw new Error("unexpected auth"); },
  });
  assert.deepEqual(result.packed, syllable);
  assert.equal(requests.length, 0);
});

// ── the public API with the user's own key ──

function apiSetup(t, respond) {
  const originalFetch = global.fetch;
  const requests = [];
  global.fetch = async (url, options = {}) => {
    requests.push({ url: String(url), method: options.method ?? "GET", headers: options.headers ?? {} });
    return respond(String(url), options);
  };
  delete require.cache[require.resolve("../electron/spicy.cjs")];
  delete require.cache[require.resolve("../electron/spicyResolver.cjs")];
  const spicy = require("../electron/spicy.cjs");
  const { resolveSpicyLyrics } = require("../electron/spicyResolver.cjs");
  t.after(() => { global.fetch = originalFetch; });
  return { spicy, resolveSpicyLyrics, requests };
}

const apiSyllable = { Type: "Syllable", Content: [{ Type: "Vocal" }], id: "track", source: "spicy_lyrics" };
const apiLine = { Type: "Line", Content: [{ Type: "Vocal" }], id: "track", source: "spotify" };
const apiReply = (Body, status = 200, headers = {}) =>
  Response.json({ Body, Status: status, Type: "object" }, { status, headers });
const isApi = (url) => url.includes("/v1/lyrics/");

test("with a key the API answers first, without a Spotify token", async (t) => {
  const { spicy, resolveSpicyLyrics, requests } = apiSetup(t, () => apiReply(apiSyllable));
  const cache = cacheSetup(t);
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, apiKey: "sl_pk_test1234", getAccessToken: async () => { throw new Error("unexpected auth"); },
  });
  assert.deepEqual(result.lyrics, apiSyllable);
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "https://api.spicylyrics.org/v1/lyrics/track");
  assert.equal(requests[0].headers.Authorization, "Bearer sl_pk_test1234");
  assert.equal(cache.get("track").kind, "lyrics");
});

test("a refused key falls back to the bridge's token route", async (t) => {
  const { spicy, resolveSpicyLyrics, requests } = apiSetup(t, (url, options) => {
    if (isApi(url)) return apiReply({ error: "key_revoked", message: "revoked" }, 401);
    const body = JSON.parse(options.body);
    return reply(body.queries[0].operation === "ext_version" ? "6.3.21" : syllable);
  });
  const cache = cacheSetup(t);
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, apiKey: "sl_pk_test1234", getAccessToken: async () => "test-token",
  });
  assert.deepEqual(result.packed, syllable);
  assert.equal(requests.filter((r) => isApi(r.url)).length, 1);
});

test("a 404 from the API does not ask the token route", async (t) => {
  const { spicy, resolveSpicyLyrics, requests } = apiSetup(t, () =>
    apiReply({ error: "lyrics_not_found", message: "none" }, 404));
  const cache = cacheSetup(t);
  const result = await resolveSpicyLyrics("track", null, {
    spicy, cache, apiKey: "sl_pk_test1234", getAccessToken: async () => { throw new Error("unexpected auth"); },
  });
  assert.equal(result.status, "not-found");
  assert.equal(requests.length, 1);
});

test("line lyrics from the API are cached and not asked for again", async (t) => {
  const { spicy, resolveSpicyLyrics, requests } = apiSetup(t, () => apiReply(apiLine));
  const cache = cacheSetup(t);
  const options = { spicy, cache, apiKey: "sl_pk_test1234", getAccessToken: async () => null };
  await resolveSpicyLyrics("track", null, options);
  const again = await resolveSpicyLyrics("track", null, options);
  assert.deepEqual(again.lyrics, apiLine);
  assert.equal(requests.length, 1);
});

test("a rate-limited API is skipped until Retry-After passes", async (t) => {
  const { spicy, resolveSpicyLyrics, requests } = apiSetup(t, (url) =>
    isApi(url) ? apiReply({ error: "rate_limited", message: "slow down" }, 429, { "Retry-After": "60" }) : reply(null, 404));
  const cache = cacheSetup(t);
  const options = { spicy, cache, apiKey: "sl_pk_test1234", getAccessToken: async () => null };
  await resolveSpicyLyrics("a", null, options);
  await resolveSpicyLyrics("b", null, options);
  assert.equal(requests.filter((r) => isApi(r.url)).length, 1);
});

test("cache entries older than 30 days are deleted and missed", (t) => {
  const cache = cacheSetup(t);
  cache.put("fresh", "lyrics", apiSyllable, "v1");
  cache.put("stale", "lyrics", apiSyllable, "v1");
  const realNow = Date.now;
  t.after(() => { Date.now = realNow; });
  Date.now = () => realNow() + 31 * 24 * 60 * 60_000;
  assert.equal(cache.get("stale"), null);
  Date.now = realNow;
  assert.ok(cache.get("fresh"));
  assert.equal(cache.get("stale"), null);
});
