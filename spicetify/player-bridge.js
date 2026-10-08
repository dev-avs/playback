// player-bridge — Spicetify extension that hands the player what only the
// Spotify client has: its access token, the playing track and queue, and
// lyrics Spicy Lyrics already fetched. (Called player-token.js before 1.3.3.)
//
// Installed by the player itself on launch (electron/spicetifySetup.cjs). By
// hand it would be:
//   copy to %APPDATA%\spicetify\Extensions\player-bridge.js
//   spicetify config extensions player-bridge.js
//   spicetify apply
//
// Why this exists: api.spicylyrics.org keys its lyrics off a Spotify track id
// and requires a Spotify token, and the syllable-timed tier is only served to
// an authenticated caller. A token minted by hand lasts about an hour, so the
// lyrics quietly fell back to line-level whenever it lapsed.
//
// spicy-lyrics never has that problem because it runs in here, where Spotify's
// own resolver will hand out a token on request. This does exactly what its
// Platform.ts does, then forwards the result to the player over loopback.

(function PlayerTokenBridge() {
  const ENDPOINT = "http://127.0.0.1:8899/token";
  // Bound up front so a later patch of window.fetch by anything else cannot
  // redirect our own POSTs.
  const rawFetch = window.fetch.bind(window);
  // Short, because a player that was closed or restarted only learns the
  // current track from the next push — at a minute it fell back to searching
  // by name, and missed Spicy lyrics and Spotify's cover, after every launch.
  const RETRY_MS = 5_000;
  // Refresh comfortably before expiry rather than at it.
  const EARLY_MS = 5 * 60_000;
  // Also re-push on a slow heartbeat even when the token is still good. The
  // player may have been restarted since the last push, and it has no way to
  // ask us for one — this is what closes that gap. A loopback POST every few
  // seconds costs nothing.
  const HEARTBEAT_MS = 15_000;

  const log = (...a) => console.log("[player-bridge]", ...a);

  /**
   * The track Spotify is actually playing, straight from the client.
   *
   * This is the other half of what makes spicy-lyrics reliable: it never asks
   * the Web API which track is playing, so it cannot be rate limited and cannot
   * pick the wrong recording. The player's own lookup searches by title/artist
   * and has both problems — a 429 stops it dead, and a fuzzy match can land on
   * a remix. An exact uri from the client sidesteps the entire question.
   */
  function readTrack() {
    const item =
      (Spicetify.Player && Spicetify.Player.data && Spicetify.Player.data.item) || null;
    const uri = item && item.uri;
    if (!uri || !uri.startsWith("spotify:track:")) return null;
    return {
      id: uri.slice("spotify:track:".length),
      name: item.name ?? null,
      artists: (item.artists ?? []).map((a) => a.name).filter(Boolean),
      image: readCover(item),
      next: (() => {
        try {
          return readNext();
        } catch (e) {
          return null; // queue shape changed; the hint is optional
        }
      })(),
    };
  }

  /**
   * The next track in Spotify's queue, for the player's "Next" hint. Newer
   * clients carry it on Player.data.nextItems; older ones on Spicetify.Queue.
   * Delimiters and ads are skipped.
   */
  function readNext() {
    const candidates = [
      ...((Spicetify.Player && Spicetify.Player.data && Spicetify.Player.data.nextItems) || []),
      ...(((Spicetify.Queue && Spicetify.Queue.nextTracks) || []).map((t) => t && t.contextTrack)),
    ];
    const current = Spicetify.Player.data && Spicetify.Player.data.item && Spicetify.Player.data.item.uri;
    for (const item of candidates) {
      const uri = item && item.uri;
      if (!uri || !uri.startsWith("spotify:track:")) continue;
      // a not-yet-updated queue still lists the song that just started
      if (uri === current) continue;
      const m = item.metadata || {};
      const name = item.name || m.title;
      if (!name) continue;
      return {
        id: uri.slice("spotify:track:".length),
        name,
        artist: (item.artists && item.artists[0] && item.artists[0].name) || m.artist_name || "",
        image: readCover(item),
      };
    }
    return null;
  }

  /**
   * The cover Spotify shows for this track. The player otherwise searches
   * iTunes/Deezer by name, which can land on the wrong album.
   */
  function readCover(item) {
    const m = item.metadata || {};
    let raw = m.image_xlarge_url || m.image_large_url || m.image_url;
    if (!raw) {
      const images = [...(item.images || []), ...((item.album && item.album.images) || [])];
      images.sort((a, b) => (b.width || 0) - (a.width || 0));
      raw = images[0] && images[0].url;
    }
    if (typeof raw !== "string") return null;
    if (raw.startsWith("spotify:image:")) {
      return "https://i.scdn.co/image/" + raw.slice("spotify:image:".length);
    }
    return raw.startsWith("https://") ? raw : null;
  }

  // ── Spicy Lyrics bridge ──
  //
  // Re-added 2026-08-07 with the Spicy Lyrics developers' permission, after
  // their API kept answering the player directly with `Type: "Static"` — the
  // untrusted tier — while the extension in this same client got `Syllable`
  // from the same Spotify token. The discriminator is the client, not the
  // account, so the player cannot fetch this for itself.
  //
  // Their notice reads:
  //
  //   "Access is granted solely for personal, individual use through official
  //    Spicy Lyrics clients or their public forks of official repositories. Any
  //    automated data extraction (scraping) or unauthorized redistribution via
  //    third-party applications is strictly prohibited."
  //
  // The grant is what covers this: one user, personal use, on their own
  // machine. It licenses nothing beyond that — the player keeps the payload in
  // memory only, never writes it to disk in a way that ships in a build.
  //
  // Two paths, tried together for coverage:
  //
  //  1. Read Spicy Lyrics' Cache Storage after each songchange. This is their
  //     GetExpireStore in src/utils/Lyrics/fetchLyrics.ts, entries keyed
  //     `/{trackId}` and wrapped as { ExpiresAt, CacheVersion, Content }. The
  //     CacheVersion has moved (1, then 5) so presence is enough — their own
  //     reader keys off the same field. On builds that do not flush to disk
  //     between song changes this path is empty; hence path 2.
  //
  //  2. Intercept the actual `fetch` Spicy makes to api.spicylyrics.org/query
  //     and forward its response for us. Piggybacks on the exact same request
  //     that earned Spicy the syllable tier, so it sidesteps any client-side
  //     discriminator we cannot reproduce from outside. Nothing is scraped:
  //     the request is Spicy's, only the response is shared, and only on this
  //     machine over loopback.
  const LYRICS_CACHE = "SpicyLyrics_LyricsStore_g1";
  const SPICY_ENDPOINT = "http://127.0.0.1:8899/spicy";
  const SPICY_API_HOST = "api.spicylyrics.org";

  // Both we and Spicy Lyrics react to the same songchange, and it only writes
  // the entry once its own request resolves — so the first look is normally too
  // early. Re-check on a backoff rather than racing it.
  const LOOKUP_DELAYS_MS = [0, 800, 2000, 4000, 8000, 15000];

  // Bumped on every track change, so a retry chain for a track the user has
  // already skipped past dies instead of pushing stale lyrics.
  let lookupGeneration = 0;

  async function readCachedLyrics(trackId) {
    if (typeof caches === "undefined") return null;
    const cache = await caches.open(LYRICS_CACHE);
    const res = await cache.match("/" + trackId);
    if (!res) return null;
    const wrapped = await res.json();
    // Same validity rules their own GetItem applies: a numeric CacheVersion
    // must exist, and the entry must not be past its expiry.
    if (!wrapped || typeof wrapped.CacheVersion !== "number") return null;
    if (typeof wrapped.ExpiresAt === "number" && wrapped.ExpiresAt < Date.now()) {
      return null;
    }
    return wrapped.Content ?? null;
  }

  async function pushCachedLyrics() {
    const generation = ++lookupGeneration;
    const track = readTrack();
    if (!track) return;

    for (const delay of LOOKUP_DELAYS_MS) {
      if (delay) await new Promise((r) => setTimeout(r, delay));
      if (generation !== lookupGeneration) return; // track moved on

      let lyrics = null;
      try {
        lyrics = await readCachedLyrics(track.id);
      } catch (e) {
        log("could not read the lyrics cache", e);
        return;
      }
      if (!lyrics) continue;

      await forwardLyrics(track, lyrics, "cache");
      return;
    }
  }

  // ── Fetch on songchange ──
  //
  // Spicy Lyrics only fetches while its lyrics page is open, so paths 1 and 2
  // above stay empty until the user opens it. Make the same request Spicy
  // would, from this same client, through the wrapped fetch so path 2 forwards
  // the response. Headers are replayed from Spicy's own last request when one
  // has been seen; otherwise built the way electron/spicy.cjs builds them.
  const SPICY_API = "https://api.spicylyrics.org/query";
  const PREFETCH_DELAY_MS = 1500; // let the cache/page paths win if they can
  const FALLBACK_VERSION = "6.3.20";

  let lastSpicyHeaders = null;
  let lastSpicyClient = null;
  let serverVersion = null;
  // Ids already forwarded, so a track Spicy itself just fetched is not asked twice.
  const forwardedIds = new Set();
  // The init objects of our own prefetches, so the capture can tell them apart.
  const ownRequests = new WeakSet();

  async function readServerVersion(token) {
    if (serverVersion) return serverVersion;
    try {
      const res = await rawFetch(SPICY_API, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "SpicyLyrics-Version": FALLBACK_VERSION,
          "SpicyLyrics-WebAuth": `Bearer ${token}`,
          "X-mode": "2",
        },
        body: JSON.stringify({
          queries: [{ operation: "ext_version" }],
          client: { version: FALLBACK_VERSION },
        }),
      });
      const job = ((await res.json())?.queries ?? []).find((q) => String(q?.operationId) === "0");
      const match = typeof job?.result?.data === "string" && job.result.data.match(/\d+\.\d+\.\d+/);
      if (match) serverVersion = match[0];
    } catch (e) {
      /* fall back below */
    }
    return serverVersion ?? FALLBACK_VERSION;
  }

  async function prefetchLyrics(generation) {
    await new Promise((r) => setTimeout(r, PREFETCH_DELAY_MS));
    if (generation !== lookupGeneration) return; // track moved on
    const track = readTrack();
    if (!track || forwardedIds.has(track.id)) return;

    const t = await readToken();
    if (!t) return;
    let headers;
    let client;
    if (lastSpicyHeaders) {
      headers = { ...lastSpicyHeaders, "spicylyrics-webauth": `Bearer ${t.token}` };
      client = lastSpicyClient;
    } else {
      const version = await readServerVersion(t.token);
      headers = {
        "Content-Type": "application/json",
        "SpicyLyrics-Version": version,
        "SpicyLyrics-WebAuth": `Bearer ${t.token}`,
        "X-mode": "2",
      };
      client = { version };
    }
    if (generation !== lookupGeneration) return;

    const init = {
      method: "POST",
      headers,
      body: JSON.stringify({
        queries: [
          { operation: "lyrics", variables: { id: track.id, auth: "SpicyLyrics-WebAuth" } },
        ],
        client: client ?? { version: FALLBACK_VERSION },
      }),
    };
    ownRequests.add(init);
    try {
      // window.fetch, not rawFetch: the capture wrapper forwards the response.
      await window.fetch(SPICY_API, init);
    } catch (e) {
      log("lyrics prefetch failed", e);
    }
  }

  async function forwardLyrics(track, lyrics, source) {
    forwardedIds.add(track.id);
    try {
      await rawFetch(SPICY_ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "text/plain" },
        body: JSON.stringify({ track, lyrics }),
      });
      log(`sent ${source} lyrics for ${track.name ?? track.id} (${lyrics.Type ?? "?"})`);
    } catch (e) {
      // Player is not running. The next songchange or heartbeat will retry.
    }
  }

  /**
   * Wrap window.fetch so that a response Spicy Lyrics fetches from its own API
   * is also forwarded to the player. See the block comment above for why this
   * runs alongside the cache reader. Non-Spicy requests pass through
   * unchanged, including our own forwarding POST which is issued through the
   * captured `rawFetch` reference and so cannot re-enter this wrapper.
   */
  function installSpicyResponseCapture() {
    if (typeof window.fetch !== "function") return;
    window.fetch = new Proxy(window.fetch, {
      apply(target, thisArg, args) {
        const [input, init] = args;
        const url = typeof input === "string" ? input : input?.url;
        const promise = Reflect.apply(target, thisArg, args);
        if (typeof url !== "string" || !url.includes(SPICY_API_HOST)) return promise;
        if (!url.includes("/query")) return promise;

        let queriedTrackId = null;
        try {
          const body = init?.body;
          if (typeof body === "string") {
            const parsed = JSON.parse(body);
            for (const q of parsed?.queries ?? []) {
              if (q?.operation === "lyrics" && q?.variables?.id) {
                queriedTrackId = q.variables.id;
                break;
              }
            }
            // Remember how Spicy itself asks, so prefetchLyrics can ask the same
            // way. Our own prefetches are not Spicy's, so they are skipped.
            if (queriedTrackId && !ownRequests.has(init)) {
              lastSpicyHeaders = Object.fromEntries(new Headers(init?.headers ?? {}));
              lastSpicyClient = parsed.client ?? null;
            }
          }
        } catch (e) {
          // Body was not JSON, or was consumed elsewhere. Nothing to capture.
        }
        if (!queriedTrackId) return promise;

        promise
          .then(async (res) => {
            if (!res || !res.ok) return;
            let body;
            try {
              body = await res.clone().json();
            } catch (e) {
              return; // Response was not JSON — Spicy also uses this API for non-lyric ops
            }
            const job = (body?.queries ?? []).find((q) => String(q?.operationId) === "0");
            const data = job?.result?.data;
            if (job?.result?.httpStatus !== 200 || data == null) return;
            const currentTrack = readTrack();
            const track =
              currentTrack && currentTrack.id === queriedTrackId
                ? currentTrack
                : { id: queriedTrackId, name: null, artists: [] };
            await forwardLyrics(track, data, "intercepted");
          })
          .catch(() => {
            /* an intercept error must not surface to Spicy's own caller */
          });

        return promise;
      },
    });
    log("watching for Spicy Lyrics API responses");
  }

  /**
   * Expiries have been seen both as epoch milliseconds and, on some builds, as
   * epoch seconds. A seconds value looks like a timestamp from 1970 in ms, so
   * it reads as long expired and the player rejects the token outright — with
   * the push silently retrying forever. Normalise here instead.
   */
  function normaliseExpiry(value) {
    const n = Number(value);
    if (!Number.isFinite(n) || n <= 0) return null;
    // ~2001 in ms. Anything smaller is far too small to be a real ms timestamp,
    // so treat it as seconds.
    return n < 1e12 ? Math.round(n * 1000) : Math.round(n);
  }

  // Logged once per source so a broken build is obvious in the console instead
  // of failing silently — this exact path went unnoticed for days.
  let lastSource = null;
  function noteSource(source) {
    if (lastSource === source) return;
    lastSource = source;
    log(`reading the token via ${source}`);
  }

  /**
   * A Spotify access token is a long unbroken string. Checking that here, and
   * not just at the player, is what stops a source whose shape has changed from
   * being pushed as junk: `getState().token` is an object on current builds, and
   * sending it produced "[object Object]", which the player rejected with "that
   * does not look like an access token" on every retry.
   */
  const MIN_TOKEN_LENGTH = 40;
  function looksLikeToken(value) {
    return (
      typeof value === "string" && value.length >= MIN_TOKEN_LENGTH && !/\s/.test(value)
    );
  }

  /**
   * Find the token string inside whatever a source returns — the string itself,
   * or an object holding it under any of the names Spotify has used. Depth
   * limited so a self-referential object cannot spin.
   */
  function coerceToken(value, depth) {
    if (looksLikeToken(value)) return value;
    if (!value || typeof value !== "object" || (depth ?? 0) > 3) return null;
    for (const key of ["accessToken", "token", "access_token", "value"]) {
      const found = coerceToken(value[key], (depth ?? 0) + 1);
      if (found) return found;
    }
    return null;
  }

  /** The matching expiry, under whichever name this build uses. */
  function coerceExpiry(value, depth) {
    if (!value || typeof value !== "object" || (depth ?? 0) > 3) return null;
    for (const key of [
      "expiresAtTime",
      "accessTokenExpirationTimestampMs",
      "expiresAt",
      "expiration",
    ]) {
      const at = normaliseExpiry(value[key]);
      if (at) return at;
    }
    for (const key of ["accessToken", "token", "access_token", "value"]) {
      const nested = coerceExpiry(value[key], (depth ?? 0) + 1);
      if (nested) return nested;
    }
    return null;
  }

  /**
   * Every place a token has lived, newest first. Spotify has moved it more than
   * once — the Aug 2026 update removed both of the sources this started with —
   * so each is tried in turn and the first that yields a real token string wins.
   */
  const TOKEN_SOURCES = [
    [
      "Spicetify.Platform.AuthorizationAPI",
      () =>
        Spicetify.Platform &&
        Spicetify.Platform.AuthorizationAPI &&
        Spicetify.Platform.AuthorizationAPI.getState &&
        Spicetify.Platform.AuthorizationAPI.getState(),
    ],
    ["Spicetify.Platform.Session", () => Spicetify.Platform && Spicetify.Platform.Session],
    ["Spicetify.GraphQL.Context", () => Spicetify.GraphQL && Spicetify.GraphQL.Context],
    ["sp://oauth/v2/token", () => Spicetify.CosmosAsync.get("sp://oauth/v2/token")],
    [
      "Spicetify.getAccessToken()",
      () => typeof Spicetify.getAccessToken === "function" && Spicetify.getAccessToken(),
    ],
  ];

  async function readToken() {
    for (const [name, read] of TOKEN_SOURCES) {
      let raw;
      try {
        raw = await read();
      } catch (e) {
        continue; // absent on this build, or the resolver refused
      }
      const token = coerceToken(raw);
      // A source that exists but no longer holds a token string is skipped
      // rather than pushed — the next one may still be good.
      if (!token) continue;
      noteSource(name);
      return { token, expiresAt: coerceExpiry(raw) };
    }

    noteSource("none");
    log(
      "could not read a Spotify token from any known source. The player will " +
        "fall back to line-timed lyrics until this is fixed.",
    );
    return null;
  }

  // Only complain about an unreachable player once per outage, so a player that
  // is simply closed does not fill the console — but a *rejected* token is
  // always reported, because that means something is genuinely wrong.
  let reportedUnreachable = false;

  async function push() {
    let sleep = RETRY_MS;
    try {
      const t = await readToken();
      if (t && t.token) {
        // text/plain keeps this a simple request, so the browser sends it
        // without a preflight the player would have to answer.
        const res = await rawFetch(ENDPOINT, {
          method: "POST",
          headers: { "Content-Type": "text/plain" },
          body: JSON.stringify({ ...t, track: readTrack() }),
        });

        if (reportedUnreachable) {
          reportedUnreachable = false;
          log("player is reachable again");
        }

        // The player answers { ok, error } — a 400 here means it took the
        // request and refused the token, which is a completely different
        // problem from it being closed, and used to look identical.
        if (!res.ok) {
          let detail = "";
          try {
            detail = (await res.json())?.error ?? "";
          } catch (e) {
            /* body was not json */
          }
          log(`player rejected the token (HTTP ${res.status}) ${detail}`);
          schedulePush(RETRY_MS);
          return;
        }

        // Delivered. Wake for the refresh, or for the heartbeat, whichever is
        // sooner.
        const untilRefresh = t.expiresAt
          ? t.expiresAt - Date.now() - EARLY_MS
          : HEARTBEAT_MS;
        sleep = Math.min(Math.max(untilRefresh, RETRY_MS), HEARTBEAT_MS);
      }
    } catch (e) {
      // Player closed, or the request was blocked before it left the renderer
      // (a tightened CSP would look exactly like this). Worth saying once —
      // silence here is what made a days-long outage invisible.
      if (!reportedUnreachable) {
        reportedUnreachable = true;
        log(`could not reach the player on ${ENDPOINT}: ${e && e.message}`);
      }
      sleep = RETRY_MS;
    }
    schedulePush(sleep);
  }

  // One timer for the whole push loop. Every songchange calls push() directly,
  // and each push used to start its own timer, so loops piled up all session.
  let pushTimer = null;
  function schedulePush(ms) {
    clearTimeout(pushTimer);
    pushTimer = setTimeout(push, ms);
  }

  // ── Song lookups for other players ──
  // When Apple Music or a browser is playing, the player has a title and an
  // artist but no Spotify id, and Spicy Lyrics needs the id. It may even have
  // started this Spotify hidden just for this. Spotify's own search box query
  // (searchSuggestions) is used rather than the Web API, which answers the
  // client's token with 429. The player picks among the candidates.
  const LOOKUP_ENDPOINT = "http://127.0.0.1:8899/lookup";
  const RESOLVED_ENDPOINT = "http://127.0.0.1:8899/resolved";
  const LOOKUP_POLL_MS = 2_000;

  async function searchCandidates(title, artist) {
    const res = await Spicetify.GraphQL.Request(Spicetify.GraphQL.Definitions.searchSuggestions, {
      query: `${title} ${artist}`,
      limit: 10,
      numberOfTopResults: 10,
      offset: 0,
      includeAuthors: false,
      includeEpisodeContentRatingsV2: false,
    });
    const hits = (res && res.data && res.data.searchV2 && res.data.searchV2.topResultsV2 &&
      res.data.searchV2.topResultsV2.itemsV2) || [];
    return hits
      .filter((h) => h.item && h.item.__typename === "TrackResponseWrapper")
      .map((h) => h.item.data)
      .map((d) => ({
        id: String(d.uri || "").split(":")[2],
        name: d.name,
        artists: ((d.artists && d.artists.items) || [])
          .map((a) => a.profile && a.profile.name)
          .filter(Boolean),
        durationMs: d.duration && d.duration.totalMilliseconds,
      }));
  }

  async function pollLookups() {
    try {
      // GraphQL.Request is filled in a moment after the rest of Spicetify
      const ready =
        Spicetify.GraphQL && typeof Spicetify.GraphQL.Request === "function" &&
        Spicetify.GraphQL.Definitions && Spicetify.GraphQL.Definitions.searchSuggestions;
      if (ready) {
        const { lookups } = await (await rawFetch(LOOKUP_ENDPOINT)).json();
        for (const q of lookups || []) {
          let candidates = [];
          let error;
          try {
            candidates = await searchCandidates(q.title, q.artist);
          } catch (e) {
            error = (e && e.message) || "search failed";
          }
          await rawFetch(RESOLVED_ENDPOINT, {
            method: "POST",
            headers: { "Content-Type": "text/plain" },
            body: JSON.stringify({ title: q.title, artist: q.artist, candidates, error }),
          });
        }
      }
    } catch (e) {
      /* player closed — push() already reports that */
    }
    setTimeout(pollLookups, LOOKUP_POLL_MS);
  }

  async function start() {
    while (
      !(window.Spicetify && Spicetify.CosmosAsync && Spicetify.Platform && Spicetify.Player)
    ) {
      await new Promise((r) => setTimeout(r, 300));
    }
    log("ready — forwarding the Spotify token and track id to the player");

    // Push immediately on a track change so the id is waiting before the player
    // has finished reacting to the same change through SMTC.
    try {
      Spicetify.Player.addEventListener("songchange", () => {
        push();
        // Spotify updates its queue a moment after songchange, so the push
        // above can still name the old "next" (often the song now playing).
        // Send again once it has settled rather than waiting for the heartbeat.
        setTimeout(push, 1500);
        pushCachedLyrics(); // bumps lookupGeneration synchronously
        prefetchLyrics(lookupGeneration);
      });
    } catch (e) {
      log("could not subscribe to songchange", e);
    }

    push();
    // The extension may have loaded mid-song, with the current track's lyrics
    // already sitting in the cache.
    pushCachedLyrics();
    installSpicyResponseCapture();
    prefetchLyrics(lookupGeneration);
    pollLookups();
  }

  start();
})();
