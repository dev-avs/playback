const {
  app,
  BrowserWindow,
  ipcMain,
  shell,
  clipboard,
  Menu,
  screen,
  session,
  desktopCapturer,
} = require("electron");
const { spawn, execFile } = require("child_process");
const spotify = require("./spotify.cjs");
const spicy = require("./spicy.cjs");
const { resolveSpicyLyrics } = require("./spicyResolver.cjs");
const { pickNeteaseSong } = require("./pick.cjs");
const { startTokenBridge } = require("./tokenBridge.cjs");
const spicetifySetup = require("./spicetifySetup.cjs");
const lyricsCache = require("./lyricsCache.cjs");
const { pickTrack, createIdCache } = require("./spotifyLookup.cjs");
const quickOpen = require("./spotifyQuickOpen.cjs");
const spicyKey = require("./spicyKey.cjs");
const updater = require("./updater.cjs");

// One widget at a time. A second launch (a shortcut, the Start menu, the
// installer's run-after-finish) would add a second widget fighting the first
// for the media helper and the bridge port; it brings the running one forward
// instead and leaves before anything has started.
if (!app.requestSingleInstanceLock()) {
  app.exit(0);
}
app.on("second-instance", () => {
  if (!win) return;
  if (win.isMinimized()) win.restore();
  win.show();
  win.focus();
});

/**
 * How finely a source knows the timing. Higher always wins, whatever the
 * provider — the renderer keys its animation off the same ladder, so these
 * values are mirrored in src/lyrics.ts.
 */
/**
 * Whether to query api.spicylyrics.org.
 *
 * On since 2026-08-07: the Spicy Lyrics developers granted this player access
 * directly. Their standard notice restricts the API to official clients and
 * public forks of their repositories, so before that grant this was off — see
 * the spicy:lyrics handler for what an ungranted client is served.
 */
const SPICY_ENABLED = true;

const TIER = { none: 0, line: 1, word: 2, syllable: 3 };
const TIER_NAME = ["untimed", "line-timed", "word-timed", "syllable-timed"];

/**
 * What the Spotify client last told us it was playing, via the Spicetify
 * extension. Exact, so it beats searching by name outright — see bridgedIdFor.
 */
let bridgedTrack = null;

/** How long a bridged track id is trusted without a fresh push. */
const BRIDGED_TRACK_TTL_MS = 15 * 60_000;

/**
 * Lyrics Spicy Lyrics already fetched inside the Spotify client, handed over by
 * the Spicetify extension. Keyed by Spotify track id; the payload is their
 * already-unpacked lyrics object.
 *
 * A small hot cache in front of lyricsCache.cjs, which is the durable copy.
 * (These were memory-only until the user confirmed permission with the project
 * owner, who was happy for people to build their own clients; see the note on
 * the on-disk cache. Still never shipped in a build.)
 */
const spicyFromClient = new Map();
const MAX_SPICY_FROM_CLIENT = 40;

/**
 * The bridge hands us lyrics in one of two shapes: the *unpacked* object Spicy
 * Lyrics stores in its own Cache Storage (has `Type`, `Content`, …), and the
 * *packed* array Spicy's API returns and which the extension unpacks on the
 * fly. Both are legitimate; only the unpack step differs, and the renderer
 * already knows how to read either.
 */
function rememberSpicyLyrics(trackId, lyrics) {
  const kind = Array.isArray(lyrics) ? "packed" : "lyrics";
  const entry = { kind, data: lyrics };
  spicyFromClient.delete(trackId); // re-insert so eviction is least-recently-set
  spicyFromClient.set(trackId, entry);
  if (spicyFromClient.size > MAX_SPICY_FROM_CLIENT) {
    spicyFromClient.delete(spicyFromClient.keys().next().value);
  }
  lyricsCache.put(trackId, kind, lyrics);
}

function looseMatch(a, b) {
  const norm = (s) =>
    String(s ?? "")
      .toLowerCase()
      .replace(/\s*[([].*$/, "") // drop "(feat. …)", "[Remastered]"
      .replace(/[^\p{L}\p{N}]+/gu, "")
      .trim();
  const x = norm(a);
  const y = norm(b);
  if (!x || !y) return false;
  return x === y || x.includes(y) || y.includes(x);
}

/**
 * The bridged id, but only when it is plainly the same track SMTC is reporting.
 *
 * Both sides are watching the same Spotify, so they normally agree. The title
 * check is what stops a stale push being used against a track that has since
 * changed — using the wrong id would fetch confidently wrong lyrics, which is
 * worse than falling back to a search.
 */
function bridgedIdFor(req) {
  if (!bridgedTrack?.id) return null;
  if (Date.now() - bridgedTrack.at > BRIDGED_TRACK_TTL_MS) return null;
  if (looseMatch(bridgedTrack.name, req?.title)) return bridgedTrack.id;
  // The song after it: on a track change SMTC usually reports the new title
  // before the extension's next push, and the queue already named it.
  const next = bridgedTrack.next;
  if (next?.id && looseMatch(next.name, req?.title)) return next.id;
  return null;
}

/**
 * Fetch the queued song's Spicy lyrics into the disk cache ahead of time, so
 * they are on hand the moment it starts instead of a request later. Each id is
 * tried once per run; resolveSpicyLyrics does the caching.
 */
const preloaded = new Set();
function preloadNext(next) {
  if (!next?.id || preloaded.has(next.id) || !SPICY_ENABLED) return;
  preloaded.add(next.id);
  resolveSpicyLyrics(next.id, spicyFromClient.get(next.id), {
    spicy,
    cache: lyricsCache,
    getAccessToken: () => spotify.getAccessToken(),
    apiKey: spicyKey.get(),
  })
    .then((r) => console.log(`[preload] ${next.name}: ${r.status}`))
    .catch(() => {});
}

// ── Other players: ids and tokens from a hidden Spotify ──
// Apple Music, a browser and the like give a title and artist but no Spotify
// id, and once Spotify is closed its token runs out within the hour. Both come
// from a Spotify started hidden for a few seconds; see spotifyQuickOpen.cjs and
// spotifyLookup.cjs.
let idCache = null; // created once userData is known
let pendingLookup = null; // { title, artist, duration } for the extension to find
const lookedUpThisRun = new Set(); // a failed search is not retried until restart
// A token this close to expiry is renewed now rather than mid-song.
const TOKEN_MARGIN_MS = 5 * 60_000;

const isSpotifyPlaying = () => /spotify/i.test(lastState?.appId ?? "");
const lookupKey = (req) => `${req.artist}|${req.title}`;

function tokenFresh() {
  if (!spotify.isSignedIn()) return false;
  const exp = spotify.manualExpiry();
  return exp == null || exp - Date.now() > TOKEN_MARGIN_MS;
}

/** The song's Spotify id, from the live bridge or an earlier lookup. */
function spotifyIdFor(req) {
  return bridgedIdFor(req) ?? idCache?.get(req)?.id ?? null;
}

/**
 * Have a hidden Spotify supply whatever this song still lacks: its id, a
 * fresh token, or both. Spotify playing it itself needs nothing — the bridge
 * already has both.
 */
function requestFromSpotify(req) {
  if (isSpotifyPlaying() || !req?.title || !idCache) return;
  const needId =
    !spotifyIdFor(req) && idCache.get(req) === undefined && !lookedUpThisRun.has(lookupKey(req));
  if (needId) {
    lookedUpThisRun.add(lookupKey(req));
    pendingLookup = { title: req.title, artist: req.artist ?? "", duration: req.duration };
  }
  if (needId || (spotifyIdFor(req) && !tokenFresh())) {
    quickOpen.open(() => sendCommand("hide-spotify"));
  }
}

/** Close a Spotify opened for us once it has handed over all it was asked for. */
function closeQuickOpenWhenDone() {
  if (!quickOpen.isOpen()) return;
  if (isSpotifyPlaying()) return quickOpen.release(); // the user has taken it over
  if (!pendingLookup && tokenFresh()) quickOpen.close();
}

function resolveLookup({ title, artist, candidates, error } = {}) {
  const req = pendingLookup;
  if (!req || req.title !== title || req.artist !== artist) return;
  pendingLookup = null;
  if (error) {
    console.error(`[lookup] search failed for ${title}: ${error}`);
  } else {
    const id = pickTrack(req, candidates);
    idCache.set(req, id);
    console.log(`[lookup] ${artist} – ${title} → ${id ?? "not on Spotify"}`);
    if (id) {
      for (const w of BrowserWindow.getAllWindows()) w.webContents.send("spicy:lyricsArrived");
    }
  }
  closeQuickOpenWhenDone();
}
const path = require("path");
const fs = require("fs");
const readline = require("readline");

const DEV_URL = process.env.VITE_DEV_SERVER_URL;
// Widget width + stage padding either side. The padding has to be wide enough
// for the widget's drop shadow, or the transparent window clips it into a hard
// black edge. Must match .widget width in App.css.
const STAGE_PAD = 22;
const WIDGET_W = 350 + STAGE_PAD * 2;
// The window's width right now: WIDGET_W until the Widget width setting moves it.
let widgetOuterW = WIDGET_W;

let win = null;
let ps = null;
let psRestartTimer = null;
let shrinkTimer = null;
let lastState = null; // replayed to the renderer on reload

// ── SMTC bridge ────────────────────────────────────────────────
// One long-lived Windows PowerShell 5.1 process: JSON state on stdout,
// commands on stdin. 5.1 specifically — PowerShell 7 has no WinRT projection.
function startSmtc() {
  // powershell.exe -File needs a real path on disk. In a packaged build
  // __dirname points inside app.asar, which Electron can read but PowerShell
  // cannot — the spawn would fail and the widget would sit on the idle
  // placeholder forever. smtc.ps1 is listed in build.asarUnpack so it is
  // written out beside the archive; point at that copy instead.
  const script = app.isPackaged
    ? path.join(process.resourcesPath, "app.asar.unpacked", "electron", "smtc.ps1")
    : path.join(__dirname, "smtc.ps1");
  ps = spawn(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script],
    { windowsHide: true },
  );

  readline.createInterface({ input: ps.stdout }).on("line", (line) => {
    if (!line.trim()) return;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      console.error("[smtc] unparseable:", line);
      return;
    }
    if (msg.type === "state") lastState = msg;
    if (msg.type === "error") console.error("[smtc]", msg.message);
    win?.webContents.send("smtc:event", msg);
  });

  ps.stderr.on("data", (d) => console.error("[smtc:stderr]", d.toString().trim()));

  ps.on("exit", (code) => {
    console.error(`[smtc] exited (${code}) — restarting in 2s`);
    ps = null;
    if (!app.isQuittingApp) psRestartTimer = setTimeout(startSmtc, 2000);
  });
}

function sendCommand(cmd) {
  if (!ps?.stdin.writable) return false;
  ps.stdin.write(cmd + "\n");
  return true;
}

// ── Lyrics (fetched here so we are not subject to browser CORS) ──
// Which writing systems a string uses. Used to reject lyrics that come back in
// the wrong language — a Korean track must not silently get an English
// translation, since translation is not something the user asked for.
function scriptsOf(text = "") {
  return {
    hangul: /[가-힣]/.test(text),
    kana: /[぀-ヿ]/.test(text),
    han: /[一-鿿]/.test(text),
    cyrillic: /[Ѐ-ӿ]/.test(text),
  };
}

function hasAnyScript(s) {
  return s.hangul || s.kana || s.han || s.cyrillic;
}

// Uploads that are explicitly a translation or a transliteration.
const DERIVED_RE =
  /\b(english|eng\s*(sub|trans)|translation|translated|romani[sz]ed|romaji|romanization|pinyin|vietsub)\b|번역|翻訳|译文|譯文/i;

function isDerived(entry) {
  return DERIVED_RE.test(`${entry.trackName ?? ""} ${entry.artistName ?? ""}`);
}

/**
 * Reject lyrics whose script disagrees with the track's own metadata. Only
 * applied when the metadata itself is non-Latin, since a Latin title tells us
 * nothing about the language of the words.
 */
function scriptMatches(expected, lyrics) {
  if (!hasAnyScript(expected)) return true;
  const got = scriptsOf(lyrics);
  if (expected.hangul) return got.hangul;
  if (expected.kana) return got.kana || got.han;
  if (expected.han) return got.han;
  if (expected.cyrillic) return got.cyrillic;
  return true;
}

function lyricsTextOf(entry) {
  return `${entry.syncedLyrics ?? ""}\n${entry.plainLyrics ?? ""}`;
}

async function lrclib(artist, track, album, duration) {
  const expected = scriptsOf(`${track} ${artist} ${album ?? ""}`);

  const qs = new URLSearchParams({ artist_name: artist, track_name: track });
  if (album) qs.set("album_name", album);
  if (duration) qs.set("duration", String(Math.round(duration)));

  // Exact match first; fall back to fuzzy search, which tolerates the
  // "Song (feat. X) - Remastered" shapes Spotify reports.
  const res = await fetch(`https://lrclib.net/api/get?${qs}`);
  if (res.ok) {
    const d = await res.json();
    if (
      (d.syncedLyrics || d.plainLyrics) &&
      !isDerived(d) &&
      scriptMatches(expected, lyricsTextOf(d))
    ) {
      return { ...d, source: "lrclib" };
    }
  }

  const sRes = await fetch(
    `https://lrclib.net/api/search?${new URLSearchParams({ artist_name: artist, track_name: track })}`,
  );
  if (!sRes.ok) return null;
  const list = await sRes.json();
  if (!Array.isArray(list) || !list.length) return null;

  const scored = list
    .filter((x) => x.syncedLyrics || x.plainLyrics)
    .map((x) => {
      let score = 0;
      if (scriptMatches(expected, lyricsTextOf(x))) score += 100;
      if (isDerived(x)) score -= 80;
      if (x.syncedLyrics) score += 20;
      if (duration && x.duration && Math.abs(x.duration - duration) <= 3) score += 15;
      if ((x.trackName ?? "").toLowerCase() === track.toLowerCase()) score += 10;
      return { x, score };
    })
    .sort((a, b) => b.score - a.score);

  const best = scored[0];
  // A negative-scoring best means everything on offer was the wrong language
  // or a translation; better to show nothing than the wrong words.
  if (!best || best.score < 0) return null;
  return { ...best.x, source: "lrclib" };
}

const NETEASE_HEADERS = {
  Referer: "https://music.163.com",
  "User-Agent": "Mozilla/5.0",
};

async function neteaseSearch(artist, track, duration) {
  const q = encodeURIComponent(`${track} ${artist}`);
  // 10, not 5: the right recording is often buried under re-uploads, and the
  // picker needs to see it before it can choose it.
  const sRes = await fetch(
    `https://music.163.com/api/search/get?s=${q}&type=1&limit=10`,
    { headers: NETEASE_HEADERS },
  );
  if (!sRes.ok) return null;
  const songs = (await sRes.json())?.result?.songs;
  if (!songs?.length) return null;
  const hit = pickNeteaseSong(songs, { artist, track, duration });
  if (!hit) {
    console.log(`[netease] no candidate matched "${track}" closely enough`);
    return null;
  }
  return hit.id;
}

// rv=-1 asks for `romalrc`, NetEase's romanized track — the only source here
// that can romanise kanji, which has no algorithmic reading.
// yv=1 asks for `yrc`, its word-by-word karaoke track. LrcLib is mostly
// line-level, so this is the main way to get real per-word timing without a
// Spotify token.
async function neteaseLyric(id) {
  const lRes = await fetch(
    `https://music.163.com/api/song/lyric?id=${id}&lv=1&kv=1&tv=-1&rv=-1&yv=1`,
    { headers: NETEASE_HEADERS },
  );
  if (!lRes.ok) return null;
  return await lRes.json();
}

async function netease(artist, track, duration) {
  const id = await neteaseSearch(artist, track, duration);
  if (!id) return null;
  const data = await neteaseLyric(id);
  // `romalrc` is NetEase's transliteration — only `lrc`/`yrc` hold the words as
  // sung. The `tlyric` translation track is deliberately ignored: it only ever
  // translates *into* Chinese, which is not what this player is for.
  const lrc = data?.lrc?.lyric;
  const yrc = data?.yrc?.lyric;
  const body = yrc || lrc;
  if (!body) return null;
  if (!scriptMatches(scriptsOf(`${track} ${artist}`), body)) return null;
  return yrc
    ? { wordLyrics: yrc, syncedLyrics: lrc || undefined, source: "netease" }
    : { syncedLyrics: lrc, source: "netease" };
}

const romaCache = new Map();

async function fetchRomanized({ artist, title }) {
  const key = `${artist}|${title}`;
  if (romaCache.has(key)) return romaCache.get(key);

  let lrc = null;
  try {
    const id = await neteaseSearch(artist, title);
    if (id) {
      const data = await neteaseLyric(id);
      const roma = data?.romalrc?.lyric;
      if (roma && roma.trim()) lrc = roma;
    }
  } catch (e) {
    console.error("[roma:netease]", e.message);
  }

  romaCache.set(key, lrc);
  if (romaCache.size > 100) romaCache.delete(romaCache.keys().next().value);
  return lrc;
}

async function genius(artist, track, apiKey) {
  const q = encodeURIComponent(`${track} ${artist}`);
  const res = await fetch(`https://api.genius.com/search?q=${q}`, {
    headers: { Authorization: `Bearer ${apiKey}` },
  });
  if (!res.ok) return null;
  const hits = (await res.json())?.response?.hits;
  if (!hits?.length) return null;

  const tl = track.toLowerCase();
  const al = artist.toLowerCase();
  const hit =
    hits.find(
      (h) =>
        h.type === "song" &&
        h.result.title.toLowerCase().includes(tl) &&
        h.result.primary_artist.name.toLowerCase().includes(al),
    ) ?? hits[0];

  const page = await fetch(hit.result.url, { headers: { "User-Agent": "Mozilla/5.0" } });
  if (!page.ok) return null;
  const html = await page.text();
  const blocks = [...html.matchAll(/data-lyrics-container="true"[^>]*>([\s\S]*?)<\/div>/g)];
  const text = blocks
    .map(([, inner]) =>
      inner
        .replace(/<br\s*\/?>/gi, "\n")
        .replace(/<[^>]+>/g, "")
        .replace(/&amp;/g, "&")
        .replace(/&apos;|&#x27;/g, "'")
        .replace(/&quot;/g, '"'),
    )
    .join("\n")
    .trim();
  return text ? { plainLyrics: text, source: "genius" } : null;
}

// Spotify decorates titles in ways lyrics databases do not carry.
function cleanTitle(title) {
  return title
    .replace(/\s*[-–]\s*(Remaster(ed)?|Radio Edit|Single Version|.*Version|.*Mix|Live).*$/i, "")
    .replace(/\s*[([](feat\.?|ft\.?|with)[^)\]]*[)\]]/gi, "")
    .trim();
}

async function fetchLyrics({ artist, title, album, duration, geniusKey, only }) {
  const primaryArtist = artist.split(/,|&|\sfeat\.?\s|\sft\.?\s/i)[0].trim();
  const candidates = [
    [artist, title],
    [primaryArtist, cleanTitle(title)],
  ];

  // Sync quality outranks provider order — a coarser source is only taken once
  // every provider has been asked. Syllable and word are ranked apart because
  // they are genuinely different: a syllable track knows when each *part* of a
  // word is sung, which is what drives the per-letter wave.
  //   3 = per-syllable  (NetEase yrc)
  //   2 = per-word      (enhanced LRC's <mm:ss.xx> tags)
  //   1 = per-line      (ordinary LRC — word timings get calculated from it)
  //   0 = untimed       (plain text — reported as no lyrics found)
  const tierOf = (r) => {
    if (!r) return -1;
    if (r.wordLyrics) return TIER.syllable;
    if (r.syncedLyrics) return /<\d+:\d+/.test(r.syncedLyrics) ? TIER.word : TIER.line;
    return TIER.none;
  };

  let best = null;
  let bestTier = TIER.none;

  const consider = (r) => {
    const tier = tierOf(r);
    // Provider order settles ties, which is the only reason it matters.
    if (tier > bestTier) {
      best = r;
      bestTier = tier;
    }
    // Nothing beats per-syllable, so there is no reason to keep asking.
    return tier === TIER.syllable ? r : null;
  };

  // NetEase before LrcLib: NetEase is the only one of the two that carries a
  // syllable track, so asking it first is what makes the early exit above pay
  // off. At equal tier the earlier provider keeps the result.
  const providers = [];
  for (const [a, t] of candidates) {
    if (!a || !t) continue;
    providers.push(["netease", () => netease(a, t, duration)]);
  }
  for (const [a, t] of candidates) {
    if (!a || !t) continue;
    providers.push(["lrclib", () => lrclib(a, t, album, duration)]);
  }
  if (geniusKey) {
    providers.push([
      "genius",
      () => genius(primaryArtist, cleanTitle(title), geniusKey),
    ]);
  }

  // `only` asks one provider by itself — the user picked it for this song.
  for (const [name, run] of only ? providers.filter(([n]) => n === only) : providers) {
    try {
      const hit = consider(await run());
      if (hit) return hit;
    } catch (e) {
      console.error(`[lyrics:${name}]`, e.message);
    }
  }

  if (!best) return null;
  console.log(`[lyrics] best available is ${TIER_NAME[bestTier]} (${best.source})`);
  // Untimed lyrics cannot be followed, so the track counts as having none.
  return bestTier >= TIER.line ? { ...best, tier: bestTier } : null;
}

// ── Album art ──────────────────────────────────────────────────
// SMTC does expose a thumbnail, but PowerShell cannot read it (its WinRT
// projection breaks on IAsyncOperation<interface>), and the thumbnail is low
// resolution anyway. iTunes/Deezer give 600px+ artwork with no API key.
const artCache = new Map();

async function itunesArt(artist, title, album) {
  const term = `${artist} ${album || title}`.trim();
  const url =
    "https://itunes.apple.com/search?" +
    new URLSearchParams({ term, entity: album ? "album" : "song", limit: "5" });
  const res = await fetch(url);
  if (!res.ok) return null;
  const { results } = await res.json();
  // results[0] is often another artist's album of the same name. A wrong cover
  // is worse than none, which lets Deezer have a go.
  const hit = results?.find((r) => looseMatch(r.artistName, artist));
  if (!hit) return null;
  const art = hit.artworkUrl100 || hit.artworkUrl60;
  // the size is just a path segment — ask for something worth blurring
  return art ? art.replace(/\/\d+x\d+bb\./, "/600x600bb.") : null;
}

async function deezerArt(artist, title) {
  const url =
    "https://api.deezer.com/search?" +
    new URLSearchParams({ q: `artist:"${artist}" track:"${title}"`, limit: "1" });
  const res = await fetch(url);
  if (!res.ok) return null;
  const { data } = await res.json();
  return data?.[0]?.album?.cover_xl ?? data?.[0]?.album?.cover_big ?? null;
}

/**
 * The cover Spotify itself shows, pushed by the Spicetify extension. Exact, so
 * it beats any search. SMTC usually reports the track a moment before the push
 * lands, so give the push a short window to catch up when the bridge is live.
 */
async function bridgedArtFor(req) {
  for (let i = 0; i < 10; i++) {
    if (!bridgedTrack || Date.now() - bridgedTrack.at > BRIDGED_TRACK_TTL_MS) return null;
    if (looseMatch(bridgedTrack.name, req?.title)) {
      return bridgedTrack.image ?? (await spotifyOEmbedArt(bridgedTrack.id));
    }
    // the queued song just started: its cover is already known (and already
    // loaded, by the "Next" hint), so there is no need to wait for the push
    const next = bridgedTrack.next;
    if (next?.image && looseMatch(next.name, req?.title)) return next.image;
    await new Promise((r) => setTimeout(r, 200));
  }
  return null;
}

/**
 * Spotify's own cover for a track id, from its public oEmbed endpoint — no
 * token, and it does not depend on the extension sending the image itself.
 */
async function spotifyOEmbedArt(trackId) {
  if (!trackId) return null;
  try {
    const res = await fetch(
      `https://open.spotify.com/oembed?url=spotify:track:${encodeURIComponent(trackId)}`,
    );
    if (!res.ok) return null;
    const url = (await res.json())?.thumbnail_url;
    // oEmbed hands out the 300px size; the 640px one differs only in this code.
    return typeof url === "string" ? url.replace("ab67616d00001e02", "ab67616d0000b273") : null;
  } catch (e) {
    console.error("[art:spotify]", e.message);
    return null;
  }
}

async function fetchArt({ artist, title, album }) {
  if (!artist && !title) return null;
  const bridged = await bridgedArtFor({ title });
  if (bridged) return bridged;
  const key = `${artist}|${album}|${title}`;
  if (artCache.has(key)) return artCache.get(key);

  let url = null;
  try {
    url = await itunesArt(artist, title, album);
  } catch (e) {
    console.error("[art:itunes]", e.message);
  }
  if (!url) {
    try {
      url = await deezerArt(artist, title);
    } catch (e) {
      console.error("[art:deezer]", e.message);
    }
  }

  if (!url) console.error(`[art] no artwork found for ${key}`);
  artCache.set(key, url);
  if (artCache.size > 200) artCache.delete(artCache.keys().next().value);
  return url;
}

// ── Window dragging ────────────────────────────────────────────
// -webkit-app-region: drag rubberbands on this window. Chromium moves the
// frame on its own compositor schedule while the OS runs its drag loop, and a
// transparent always-on-top window with backdrop-filter presents slowly enough
// that the two visibly diverge — the window trails the pointer, then snaps.
//
// So we run the drag ourselves. The renderer reports only "down" and "up"; the
// position comes from polling the OS cursor here, which means no pointer event
// ever makes a round trip while the window is moving, and there is nothing to
// fall behind and catch up.
let dragTimer = null;
let dragOrigin = null;

const DRAG_INTERVAL_MS = 8; // ~120 Hz, comfortably above any display refresh

function startDrag() {
  stopDrag();
  if (!win) return;
  const cursor = screen.getCursorScreenPoint();
  const [x, y] = win.getPosition();
  dragOrigin = { cursorX: cursor.x, cursorY: cursor.y, winX: x, winY: y };

  dragTimer = setInterval(() => {
    if (!win || win.isDestroyed() || !dragOrigin) return stopDrag();
    const now = screen.getCursorScreenPoint();
    win.setPosition(
      dragOrigin.winX + (now.x - dragOrigin.cursorX),
      dragOrigin.winY + (now.y - dragOrigin.cursorY),
    );
  }, DRAG_INTERVAL_MS);
}

function stopDrag() {
  if (dragTimer) clearInterval(dragTimer);
  dragTimer = null;
  dragOrigin = null;
  // Any resize that arrived while the drag was running was dropped. The
  // renderer will not ask again on its own, so replay the last request here.
  replayWantedHeight();
}

/**
 * The last height the renderer asked for, and whether it was hovering.
 *
 * Needed because a request can be refused: mid-drag it is dropped outright.
 * The ResizeObserver driving this only fires when the widget's *own* size
 * changes, so a refused request is never repeated — the window keeps the old
 * height and the bottom of the widget is clipped, which is what made the
 * controls vanish. Holding the last request lets the refusal be undone once
 * whatever blocked it has passed.
 */
let wantedHeight = null;
let wantedHovered = false;

/**
 * The size each window last set itself to, so a display scale change can put
 * it back. Windows leaves a non-resizable window at its old pixel size when the
 * scale changes, which at the new scale is the wrong size (the widget was cut
 * off on the right), and nothing in the page changes size to ask again.
 */
const windowSizes = new WeakMap();

function setWindowSize(w, width, height) {
  windowSizes.set(w, [width, height]);
  // Windows ignores setSize on a non-resizable window, so lift the flag
  // for the call and put it straight back — no user drag-resize.
  w.setResizable(true);
  w.setSize(width, height);
  w.setResizable(false);
}

function applyWindowHeight(h, hovered) {
  if (!win) return;
  // Cancel any pending shrink first, before the drag and jitter checks below
  // can return: a widget that collapses then re-expands within the debounce
  // would otherwise leave the old shrink armed, and it would fire and clip the
  // bottom half — mid-drag included, where the re-expand itself is deferred.
  clearTimeout(shrinkTimer);

  // Never resize mid-drag. setSize on a window the user is moving fights the
  // move and is felt as a stutter, and the resizable flag has to be toggled
  // for it (see below), which is worse still while a drag is in flight.
  // stopDrag replays it.
  if (dragTimer) return;

  const current = win.getSize()[1];
  // Sub-pixel layout jitter would otherwise cause a constant setSize churn.
  if (Math.abs(current - h) < 3) return;

  const apply = () => {
    if (!win) return;
    setWindowSize(win, win.getSize()[0], h);
  };

  if (h > current) {
    apply(); // grow at once, so content is never left clipped
    return;
  }
  // Shrinking is what makes the widget flicker when the pointer sits near
  // its bottom edge: the window pulls out from under the cursor, the widget
  // un-hovers and collapses, which moves the edge again. Never shrink while
  // hovered, and debounce it otherwise. Un-hovering changes the widget's size,
  // so the renderer asks again by itself — no replay needed for that case.
  if (hovered) return;
  shrinkTimer = setTimeout(apply, 220);
}

function replayWantedHeight() {
  if (wantedHeight != null) applyWindowHeight(wantedHeight, wantedHovered);
}

// ── Window ─────────────────────────────────────────────────────
function createWindow() {
  win = new BrowserWindow({
    width: WIDGET_W,
    height: 420,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: false,
    hasShadow: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      // nothing here is typed into but a font search and a token
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      // An always-on-top overlay is almost never the focused window, and
      // Chromium otherwise throttles rAF/timers to ~1 Hz when unfocused or
      // occluded — which is what makes the clock and lyrics visibly lag.
      backgroundThrottling: false,
      // shortcuts are blocked in both modes; keep the tools themselves
      // reachable while developing
      devTools: !!DEV_URL,
    },
  });

  // Renderer logs would otherwise only reach DevTools, which is disabled in
  // production — surface them on the terminal so the app is debuggable.
  win.webContents.on("console-message", (e) => {
    const level = ["debug", "info", "warning", "error"][e.level] ?? "info";
    if (level === "debug") return;
    console.log(`[renderer:${level}] ${e.message}`);
  });

  // Swallow the devtools shortcuts.
  win.webContents.on("before-input-event", (event, input) => {
    const key = (input.key || "").toLowerCase();
    const devtoolsCombo =
      (input.control && input.shift && (key === "i" || key === "j" || key === "c")) ||
      key === "f12";
    if (devtoolsCombo) event.preventDefault();
  });

  win.setAlwaysOnTop(true, "screen-saver");

  // Closing the widget from the taskbar closes only this window. An open
  // settings window would keep the app alive, and the SMTC feed would go on
  // sending to the destroyed widget and throw. The widget is the app, so its
  // close is a quit, same as its own close button.
  win.on("closed", () => {
    win = null;
    app.quit();
  });

  // setSize on a minimized window is lost: restore brings back the size from
  // before minimizing, and the widget will not ask again unless its own size
  // changes. Re-apply the last request, or a widget that grew while minimized
  // comes back with its bottom cut off.
  win.on("restore", replayWantedHeight);

  if (DEV_URL) {
    // Electron usually wins the race against the Vite dev server; retry
    // rather than pulling in a wait-on dependency.
    const load = () => win.loadURL(DEV_URL).catch(() => setTimeout(load, 400));
    load();
  } else {
    win.loadFile(path.join(__dirname, "..", "dist", "index.html"));
  }

  // Replay the most recent state so a reload does not show an empty widget
  // while we wait for the next SMTC emit.
  win.webContents.on("did-finish-load", () => {
    if (lastState) win.webContents.send("smtc:event", lastState);
  });

  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: "deny" };
  });
}

// ── Settings window ────────────────────────────────────────────
// Same page as the widget, loaded with #settings. Settings themselves travel
// through localStorage, which both windows share, so this only manages the
// window.
let settingsWin = null;
const SETTINGS_W = 600 + STAGE_PAD * 2;

function openSettings() {
  if (settingsWin) {
    settingsWin.focus();
    return;
  }
  // Beside the widget, on whichever side has room.
  const [wx, wy] = win ? win.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: wx, y: wy }).workArea;
  const right = wx + widgetOuterW;
  const x = right + SETTINGS_W <= area.x + area.width ? right - STAGE_PAD : wx - SETTINGS_W + STAGE_PAD;

  settingsWin = new BrowserWindow({
    x,
    y: wy,
    width: SETTINGS_W,
    height: 520,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: win?.isAlwaysOnTop() ?? true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      // nothing here is typed into but a font search and a token
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  if (win?.isAlwaysOnTop()) settingsWin.setAlwaysOnTop(true, "screen-saver");
  settingsWin.once("ready-to-show", () => settingsWin?.show());
  // The header is a drag region, where Windows would show its system menu on
  // right-click instead of the page getting contextmenu. Right-click closes.
  settingsWin.on("system-context-menu", (e) => {
    e.preventDefault();
    closeSettings();
  });
  settingsWin.on("closed", () => {
    settingsWin = null;
    // re-read next time, so a font installed meanwhile appears
    fontList = null;
  });
  if (DEV_URL) settingsWin.loadURL(`${DEV_URL}#settings`);
  else settingsWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash: "settings" });
}

function closeSettings() {
  settingsWin?.close();
}

// ── Changelog window ───────────────────────────────────────────
// What changed, after an update (from the version recorded at the last run)
// or from Settings → What's new. Same page again, loaded with #changelog; the
// entries themselves are bundled in src/changelog.ts.
const CHANGELOG_W = 320 + STAGE_PAD * 2;
let changelogWin = null;
let pastChangelogWin = null;

/** Past changelogs: every version before the current one, beside the latest. */
function openPastChangelog() {
  if (pastChangelogWin) {
    pastChangelogWin.focus();
    return;
  }
  const anchor = changelogWin ?? win;
  const [ax, ay] = anchor ? anchor.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: ax, y: ay }).workArea;
  const x = ax + CHANGELOG_W <= area.x + area.width - CHANGELOG_W ? ax + CHANGELOG_W - STAGE_PAD : ax - CHANGELOG_W + STAGE_PAD;
  pastChangelogWin = new BrowserWindow({
    x,
    y: ay,
    width: CHANGELOG_W,
    height: 480,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      // nothing here is typed into but a font search and a token
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  pastChangelogWin.setAlwaysOnTop(true, "screen-saver");
  pastChangelogWin.once("ready-to-show", () => pastChangelogWin?.show());
  pastChangelogWin.on("closed", () => {
    pastChangelogWin = null;
  });
  if (DEV_URL) pastChangelogWin.loadURL(`${DEV_URL}#changelog-past`);
  else pastChangelogWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash: "changelog-past" });
}

function openChangelog(fromVersion = null) {
  if (changelogWin) {
    changelogWin.focus();
    return;
  }
  // Centred on the widget's screen: it is read once, so it can sit apart.
  const [wx, wy] = win ? win.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: wx, y: wy }).workArea;
  changelogWin = new BrowserWindow({
    x: Math.round(area.x + (area.width - CHANGELOG_W) / 2),
    y: Math.round(area.y + area.height * 0.2),
    width: CHANGELOG_W,
    height: 480,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      // nothing here is typed into but a font search and a token
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  changelogWin.setAlwaysOnTop(true, "screen-saver");
  changelogWin.once("ready-to-show", () => changelogWin?.show());
  changelogWin.on("closed", () => {
    changelogWin = null;
  });
  const hash = fromVersion ? `changelog?from=${encodeURIComponent(fromVersion)}` : "changelog";
  if (DEV_URL) changelogWin.loadURL(`${DEV_URL}#${hash}`);
  else changelogWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash });
}

// Version control (#versions), from Settings → Behavior: every release, to
// update or revert to. Centred on the widget's screen, like the changelog.
let versionsWin = null;

function openVersions() {
  if (versionsWin) {
    versionsWin.focus();
    return;
  }
  const [wx, wy] = win ? win.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: wx, y: wy }).workArea;
  versionsWin = new BrowserWindow({
    x: Math.round(area.x + (area.width - CHANGELOG_W) / 2),
    y: Math.round(area.y + area.height * 0.12),
    width: CHANGELOG_W,
    height: 560,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  versionsWin.setAlwaysOnTop(true, "screen-saver");
  versionsWin.once("ready-to-show", () => versionsWin?.show());
  versionsWin.on("closed", () => {
    versionsWin = null;
  });
  if (DEV_URL) versionsWin.loadURL(`${DEV_URL}#versions`);
  else versionsWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash: "versions" });
}

// The update window (#update): asks on launch when GitHub has a newer version.
const UPDATE_W = 300 + STAGE_PAD * 2;
let updateWin = null;

function openUpdate() {
  if (updateWin) {
    updateWin.focus();
    return;
  }
  const [wx, wy] = win ? win.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: wx, y: wy }).workArea;
  updateWin = new BrowserWindow({
    x: Math.round(area.x + (area.width - UPDATE_W) / 2),
    y: Math.round(area.y + area.height * 0.2),
    width: UPDATE_W,
    height: 240,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  updateWin.setAlwaysOnTop(true, "screen-saver");
  updateWin.once("ready-to-show", () => updateWin?.show());
  updateWin.on("closed", () => {
    updateWin = null;
  });
  if (DEV_URL) updateWin.loadURL(`${DEV_URL}#update`);
  else updateWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash: "update" });
}

/**
 * Opens the changelog on the first run of each version, a fresh install
 * included, unless the user turned it off. The widget calls this once per
 * launch with that setting; the version is recorded either way.
 */
function showChangelogAfterUpdate(enabled) {
  const file = path.join(app.getPath("userData"), "last-version.json");
  let previous = null;
  try {
    previous = JSON.parse(fs.readFileSync(file, "utf8")).version ?? null;
  } catch {
    /* first run */
  }
  const current = app.getVersion();
  try {
    fs.writeFileSync(file, JSON.stringify({ version: current }));
  } catch (e) {
    console.error("[changelog] could not record the version:", e.message);
  }
  // after the Setup window, when it is up, so the two never stack
  if (enabled && previous !== current) setupSettled.then(() => openChangelog(previous));
}

// ── Setup window ───────────────────────────────────────────────
// Spotify, Spicetify and the bridge (spicetifySetup.cjs), as a checklist.
// Opens on the first run, and later only when the user has something to do
// (shouldShowSetup). Same page again, loaded with #setup.
const SETUP_W = 340 + STAGE_PAD * 2;
let setupWin = null;
let firstRun = false;
/** the latest ensure(), as a SetupState (src/player.d.ts) */
let setupWork = null;
// Settles once Setup is out of the way: not needed this launch, or closed.
// The changelog waits on it.
let setupDone;
const setupSettled = new Promise((resolve) => (setupDone = resolve));

/** Puts the bridge in place; `allowApply` lets it run `spicetify apply`, which restarts Spotify. */
function runSetup(allowApply) {
  const source = fs.readFileSync(path.join(app.getAppPath(), "spicetify", spicetifySetup.BRIDGE));
  setupWork = spicetifySetup
    .ensure(source, { allowApply })
    .catch((e) => {
      // say what failed, and offer the apply again
      const status = spicetifySetup.check(source);
      const ready = spicetifySetup.isReady(status);
      return { status, ready, actions: [], needsApply: status.spicetify && !ready, error: e.message };
    })
    .then((r) => {
      for (const a of r.actions) console.log("[setup]", a);
      if (r.error) console.error("[setup]", r.error);
      else if (r.needsApply) console.log("[setup] the bridge waits on `spicetify apply`");
      return { ...r, firstRun };
    });
  return setupWork;
}

function openSetup() {
  if (setupWin) {
    setupWin.focus();
    return;
  }
  // Centred on the widget's screen, like the changelog it comes before.
  const [wx, wy] = win ? win.getPosition() : [100, 100];
  const area = screen.getDisplayNearestPoint({ x: wx, y: wy }).workArea;
  setupWin = new BrowserWindow({
    x: Math.round(area.x + (area.width - SETUP_W) / 2),
    y: Math.round(area.y + area.height * 0.2),
    width: SETUP_W,
    height: 480,
    frame: false,
    transparent: true,
    resizable: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      spellcheck: false,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: !!DEV_URL,
    },
  });
  setupWin.setAlwaysOnTop(true, "screen-saver");
  setupWin.once("ready-to-show", () => setupWin?.show());
  setupWin.on("closed", () => {
    setupWin = null;
    setupDone();
  });
  if (DEV_URL) setupWin.loadURL(`${DEV_URL}#setup`);
  else setupWin.loadFile(path.join(__dirname, "..", "dist", "index.html"), { hash: "setup" });
}

/**
 * Installed font families, for the settings window's font list. Read through
 * .NET's GDI view of the system, which includes per-user installs, so a font
 * the user just installed by right-click shows up. One spawn, then cached.
 */
let fontList = null;
function listFonts() {
  if (fontList) return fontList;
  fontList = new Promise((resolve) => {
    const ps = spawn(
      "powershell.exe",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8;" +
          "Add-Type -AssemblyName System.Drawing;" +
          "(New-Object System.Drawing.Text.InstalledFontCollection).Families | ForEach-Object Name",
      ],
      { windowsHide: true },
    );
    let out = "";
    ps.stdout.on("data", (d) => (out += d));
    ps.on("error", () => resolve([]));
    ps.on("exit", () => {
      const names = [...new Set(out.split(/\r?\n/).map((s) => s.trim()).filter(Boolean))];
      // An empty answer is a failure; let the next request try again.
      if (!names.length) fontList = null;
      resolve(names);
    });
  });
  return fontList;
}

/** Tell every window to re-read the token status (and the widget to re-look-up lyrics). */
function broadcastTokenChange() {
  for (const w of BrowserWindow.getAllWindows()) {
    w.webContents.send("spotify:tokenArrived");
  }
}

app.whenReady().then(() => {
  Menu.setApplicationMenu(null); // also removes the default devtools accelerators

  // Display scale changed (Windows Settings → Display → Scale): put every
  // window back to the size it asked for. See windowSizes.
  screen.on("display-metrics-changed", (_e, _display, changed) => {
    if (!changed.includes("scaleFactor")) return;
    for (const w of BrowserWindow.getAllWindows()) {
      const size = windowSizes.get(w);
      if (size) setWindowSize(w, ...size);
    }
  });

  // Lets the renderer capture the system audio mix, so the visualiser reacts
  // to whatever is actually playing. Windows needs a video source in the
  // request for loopback to be granted; the renderer stops that track at once.
  session.defaultSession.setDisplayMediaRequestHandler(
    (_request, callback) => {
      desktopCapturer
        .getSources({ types: ["screen"] })
        .then((sources) => callback({ video: sources[0], audio: "loopback" }))
        .catch(() => callback({}));
    },
    { useSystemPicker: false },
  );

  lyricsCache.init(app.getPath("userData"));
  spicyKey.load();
  idCache = createIdCache(path.join(app.getPath("userData"), "spotify-ids.json"));
  // Keep the Spicetify bridge in place and current (spicetifySetup.cjs). Only
  // a first run (no version recorded yet) applies Spicetify on its own, which
  // restarts Spotify; later launches leave that to Setup's Apply now. Read
  // before the widget records this version.
  firstRun = !fs.existsSync(path.join(app.getPath("userData"), "last-version.json"));
  // a first run shows Setup at once, so the Spotify restart is explained
  if (firstRun) openSetup();
  runSetup(firstRun)
    .then((s) => {
      if (spicetifySetup.shouldShowSetup(s, firstRun)) openSetup();
      else setupDone();
    })
    .catch((e) => {
      console.error("[setup]", e.message);
      setupDone();
    });
  createWindow();
  startSmtc();

  updater.init((state) => {
    for (const w of BrowserWindow.getAllWindows()) w.webContents.send("update:changed", state);
  });
  // Asked once per launch, after Setup is out of the way so the two never stack,
  // unless the user said to skip this version.
  setupSettled.then(async () => {
    const state = await updater.check();
    if (state.status === "available" && !updater.isSkipped(state.version)) openUpdate();
  });
  ipcMain.handle("update:state", () => updater.getState());
  ipcMain.handle("update:check", () => updater.check());
  ipcMain.handle("update:install", () => updater.install());
  ipcMain.handle("update:skip", () => {
    const { version } = updater.getState();
    if (version) updater.skip(version);
  });
  ipcMain.handle("update:releases", () =>
    updater.releases().then(
      (list) => ({ ok: true, list }),
      (e) => ({ ok: false, error: e.message }),
    ),
  );
  ipcMain.handle("update:bump", (_e, version) => updater.bump(String(version)));
  ipcMain.handle("versions:open", () => openVersions());

  ipcMain.handle("player:command", (_e, cmd) => sendCommand(cmd));
  ipcMain.handle("lyrics:fetch", (_e, req) => fetchLyrics(req));
  ipcMain.handle("art:fetch", (_e, req) => fetchArt(req));
  ipcMain.handle("lyrics:romanized", (_e, req) => fetchRomanized(req));

  // ── Spotify / Spicy Lyrics ──
  ipcMain.handle("spotify:status", () => ({
    configured: spotify.isConfigured(),
    signedIn: spotify.isSignedIn(),
    redirectUri: spotify.REDIRECT_URI,
    tokenExpiresAt: spotify.manualExpiry(),
  }));
  ipcMain.handle("spotify:setToken", (_e, token) => {
    const res = spotify.setManualToken(token);
    if (res.ok) broadcastTokenChange();
    return res;
  });
  ipcMain.handle("smtc:last", () => lastState);
  // What Spotify will play next, if the bridged track is the one on screen.
  ipcMain.handle("bridge:next", (_e, req) => {
    if (!bridgedIdFor(req)) return null;
    const next = bridgedTrack.next ?? null;
    // a stale queue can still name the song now playing
    return next && next.id !== bridgedTrack.id ? next : null;
  });
  ipcMain.handle("settings:toggle", () => (settingsWin ? closeSettings() : openSettings()));
  ipcMain.handle("settings:open", () => openSettings());
  ipcMain.handle("settings:close", () => closeSettings());
  ipcMain.handle("app:version", () => app.getVersion());
  ipcMain.handle("changelog:open", () => openChangelog());
  ipcMain.handle("changelog:openPast", () => openPastChangelog());
  ipcMain.handle("changelog:check", (_e, enabled) => showChangelogAfterUpdate(!!enabled));
  // Both changelog windows and Setup: whichever one asked.
  ipcMain.handle("changelog:close", (e) => BrowserWindow.fromWebContents(e.sender)?.close());
  ipcMain.handle("changelog:resize", (e, height) => {
    const w = BrowserWindow.fromWebContents(e.sender);
    if (!w) return;
    const h = Math.max(120, Math.min(1000, Math.ceil(height)));
    const [width, current] = w.getSize();
    if (Math.abs(current - h) < 2) return;
    setWindowSize(w, width, h);
  });
  ipcMain.handle("setup:state", () => setupWork);
  ipcMain.handle("setup:recheck", () => runSetup(false));
  ipcMain.handle("setup:apply", () => runSetup(true));
  ipcMain.handle("setup:copy", (_e, text) => clipboard.writeText(String(text)));
  ipcMain.handle("setup:openLink", (_e, url) => {
    if (/^https:\/\//.test(url)) shell.openExternal(url);
  });
  // Hide when nothing is playing (Settings → Behavior). Shown again without
  // taking focus, so music starting never pulls focus from what you are doing.
  // Widget width (Settings → Layout): the window follows the widget's width.
  ipcMain.handle("win:setWidth", (_e, width) => {
    if (!win) return;
    const w = Math.max(280, Math.min(600, Math.round(width))) + STAGE_PAD * 2;
    widgetOuterW = w;
    const [cw, ch] = win.getSize();
    if (cw === w) return;
    setWindowSize(win, w, ch);
  });
  ipcMain.handle("win:setHidden", (_e, hidden) => {
    if (!win) return;
    if (hidden) win.hide();
    else if (!win.isVisible()) win.showInactive();
  });
  ipcMain.handle("fonts:list", () => listFonts());
  ipcMain.handle("settings:resize", (_e, height) => {
    if (!settingsWin) return;
    const h = Math.max(120, Math.min(1000, Math.ceil(height)));
    if (Math.abs(settingsWin.getSize()[1] - h) < 2) return;
    setWindowSize(settingsWin, SETTINGS_W, h);
  });

  // The Spicetify extension pushes the Spotify client's own token in here, with
  // its real expiry. Same source spicy-lyrics reads; see tokenBridge.cjs.
  startTokenBridge((token, expiresAt, track) => {
    if (track?.id) {
      const changed = track.id !== bridgedTrack?.id;
      const nextChanged = JSON.stringify(track.next ?? null) !== JSON.stringify(bridgedTrack?.next ?? null);
      bridgedTrack = { ...track, at: Date.now() };
      if (changed || nextChanged) preloadNext(track.next);
      // The queue can change without the track changing (the user queued
      // something), so the "Next" hint is refreshed on its own.
      if (nextChanged && !changed) {
        for (const w of BrowserWindow.getAllWindows()) w.webContents.send("bridge:track");
      }
      if (changed) {
        console.log(`[bridge] now playing ${track.name} (${track.id})`);
        // Written out so the current id can be inspected without a debugger;
        // the lookup itself reads the in-memory copy.
        try {
          fs.writeFileSync(
            path.join(app.getPath("userData"), "bridged-track.json"),
            JSON.stringify(bridgedTrack),
          );
        } catch {
          /* diagnostics only — never worth failing a token push over */
        }
        for (const w of BrowserWindow.getAllWindows()) {
          w.webContents.send("bridge:track");
        }
        // The renderer usually asks for lyrics on SMTC's change, before this id
        // arrives, and finds nothing to key on. If lyrics are already on hand,
        // have it look again now instead of waiting for the next track.
        if (spicyFromClient.has(track.id) || lyricsCache.get(track.id)) {
          for (const w of BrowserWindow.getAllWindows()) {
            w.webContents.send("spicy:lyricsArrived");
          }
        }
      }
    }
    const before = spotify.isSignedIn();
    const res = spotify.setManualToken(token, expiresAt);
    if (res.ok && !before) {
      console.log("[bridge] token accepted — Spicy Lyrics is reachable again");
      // Nothing is watching for this, so nudge the renderer to look again.
      for (const w of BrowserWindow.getAllWindows()) {
        w.webContents.send("spotify:tokenArrived");
      }
    } else if (!res.ok) {
      console.error(`[bridge] token rejected: ${res.error}`);
    }
    closeQuickOpenWhenDone();
    return res;
  },
  // Lyrics Spicy Lyrics already had cached, forwarded from inside Spotify.
  (capture) => {
    const track = capture?.track;
    const lyrics = capture?.lyrics;
    if (!track?.id || !lyrics) return { ok: false, error: "no track id or lyrics" };

    rememberSpicyLyrics(track.id, lyrics);
    console.log(
      `[bridge] ${lyrics.Type ?? "?"} lyrics for ${track.name ?? "?"} (${track.id})`,
    );
    // The renderer may be showing this very track on a coarser source, so tell
    // it to look again rather than leave the upgrade until the next track.
    for (const w of BrowserWindow.getAllWindows()) {
      w.webContents.send("spicy:lyricsArrived");
    }
    return { ok: true };
  },
  {
    pending: () => (pendingLookup ? [{ title: pendingLookup.title, artist: pendingLookup.artist }] : []),
    resolve: resolveLookup,
  });
  ipcMain.handle("spotify:login", async () => {
    try {
      await spotify.login();
      return { ok: true };
    } catch (e) {
      return { ok: false, error: e.message };
    }
  });
  ipcMain.handle("spotify:signOut", () => {
    const res = spotify.signOut();
    broadcastTokenChange();
    return res;
  });

  // The bridge supplies the exact Spotify id; cached line lyrics are refreshed
  // when their request version is missing or older than the current one.
  ipcMain.handle("spicy:lyrics", async (_e, req) => {
    const trackId = spotifyIdFor(req);
    if (!trackId || !tokenFresh()) requestFromSpotify(req);
    return resolveSpicyLyrics(trackId, spicyFromClient.get(trackId), {
      spicy,
      cache: lyricsCache,
      getAccessToken: () => spotify.getAccessToken(),
      apiKey: spicyKey.get(),
      enabled: SPICY_ENABLED,
    });
  });

  // The user's own Spicy Lyrics key (spicyKey.cjs). Every change is broadcast,
  // so Setup, Settings and the widget's lyrics follow along.
  const spicyKeyChanged = (result) => {
    if (result?.ok !== false) {
      for (const w of BrowserWindow.getAllWindows()) w.webContents.send("spicyKey:changed");
    }
    return result;
  };
  ipcMain.handle("spicyKey:status", () => spicyKey.status());
  ipcMain.handle("spicyKey:connect", (e) =>
    spicyKey.connect(BrowserWindow.fromWebContents(e.sender)).then(spicyKeyChanged));
  ipcMain.handle("spicyKey:set", (_e, raw) => spicyKey.set(raw).then(spicyKeyChanged));
  ipcMain.handle("spicyKey:clear", () => spicyKeyChanged(spicyKey.clear()));
  ipcMain.handle("win:dragStart", () => startDrag());
  ipcMain.handle("win:dragEnd", () => stopDrag());

  ipcMain.handle("win:resize", (_e, height, hovered) => {
    if (!win) return;
    wantedHeight = Math.max(80, Math.min(1400, Math.ceil(height)));
    wantedHovered = !!hovered;
    applyWindowHeight(wantedHeight, wantedHovered);
  });
  ipcMain.handle("win:setAlwaysOnTop", (_e, on) => {
    win?.setAlwaysOnTop(!!on, "screen-saver");
    settingsWin?.setAlwaysOnTop(!!on, "screen-saver");
  });
  // Settings → Behavior → Uninstall: the bridge out of Spicetify (restarts
  // Spotify), then, once this process has exited, the app and its data folder
  // (settings, Spicy key and sign-in, lyrics cache). Only the installed app has
  // an uninstaller; the dev build says so instead.
  ipcMain.handle("app:uninstall", async () => {
    const uninstaller = path.join(path.dirname(process.execPath), "Uninstall player.exe");
    if (!fs.existsSync(uninstaller)) {
      return { ok: false, error: "Only the installed app can uninstall itself." };
    }
    try {
      for (const a of await spicetifySetup.remove()) console.log("[uninstall]", a);
    } catch (e) {
      console.error("[uninstall] bridge:", e.message);
    }
    const quote = (p) => `'${p.replace(/'/g, "''")}'`;
    const cleanup = [
      `Wait-Process -Id ${process.pid} -ErrorAction SilentlyContinue`,
      // the app's helper processes let go of the data folder a moment later
      "Start-Sleep -Seconds 2",
      `Get-Process player -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq ${quote(process.execPath)} } | Stop-Process -Force`,
      `Start-Process -FilePath ${quote(uninstaller)} -ArgumentList '/S' -Wait`,
      `Remove-Item -LiteralPath ${quote(app.getPath("userData"))} -Recurse -Force -ErrorAction SilentlyContinue`,
    ].join("; ");
    // A child of this app, detached or not, is killed when the app exits, so
    // the cleanup is started through WMI instead: Windows owns that process,
    // and it outlives us.
    const encoded = Buffer.from(cleanup, "utf16le").toString("base64");
    const launcher = [
      "$si = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0 }",
      `$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell.exe -NoProfile -WindowStyle Hidden -EncodedCommand ${encoded}'; ProcessStartupInformation = $si }`,
      "if ($r.ReturnValue -ne 0) { exit 1 }",
    ].join("; ");
    try {
      await new Promise((resolve, reject) =>
        execFile("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", launcher], { windowsHide: true }, (err) =>
          err ? reject(err) : resolve(),
        ),
      );
    } catch (e) {
      console.error("[uninstall] could not start the cleanup:", e.message);
      return { ok: false, error: "Could not start the uninstaller." };
    }
    app.quit();
    return { ok: true };
  });
  ipcMain.handle("win:close", () => app.quit());
  ipcMain.handle("win:minimize", () => win?.minimize());
});

app.on("before-quit", () => {
  app.isQuittingApp = true;
  quickOpen.close();
  clearTimeout(psRestartTimer);
  ps?.stdin.end();
  ps?.kill();
});

app.on("window-all-closed", () => app.quit());
