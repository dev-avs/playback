// Spotify track ids for songs playing in other apps (Apple Music, browsers…).
//
// Spicy Lyrics is keyed by Spotify track id. When Spotify itself is playing,
// the Spicetify extension reads the id straight out of the client. For any
// other player there is no id, so the extension searches Spotify's own catalog
// for the title and artist (see spotifyQuickOpen.cjs for how a hidden Spotify
// is started to do it) and returns the candidates. This picks among them and
// remembers the answer, so each song is only ever looked up once.

const fs = require("fs");

// Older misses are tried again: Spotify may have added the song since.
const MISS_TTL_MS = 7 * 24 * 3600_000;
// Album, single and remaster cuts of one song sit within a few seconds of each
// other; anything further off is a different recording.
const MAX_DURATION_DIFF_MS = 10_000;

/** Letters and digits only, with "(feat. …)", "[Remastered]" and " - Live" cut. */
function norm(s) {
  return String(s ?? "")
    .toLowerCase()
    .replace(/\s+-\s+.*$/, "")
    .replace(/\s*[([].*$/, "")
    .replace(/[^\p{L}\p{N}]+/gu, "");
}

function titleMatches(a, b) {
  const x = norm(a);
  const y = norm(b);
  return !!x && !!y && (x === y || x.includes(y) || y.includes(x));
}

// The playing app reports every artist in one string ("Post Malone & Swae
// Lee"), Spotify lists them apart, so any one of them appearing is enough.
function artistMatches(playing, artists) {
  const p = norm(playing);
  if (!p) return false;
  return artists.some((a) => {
    const n = norm(a);
    return !!n && (p.includes(n) || n.includes(p));
  });
}

/**
 * @param req { title, artist, duration? } duration in seconds, as SMTC reports it
 * @param candidates [{ id, name, artists: string[], durationMs? }]
 * @returns the best matching id, or null
 */
function pickTrack(req, candidates) {
  const wantMs = req.duration > 0 ? req.duration * 1000 : null;
  let best = null;
  let bestDiff = Infinity;
  for (const c of candidates ?? []) {
    if (!c?.id || !titleMatches(c.name, req.title)) continue;
    if (!artistMatches(req.artist, c.artists ?? [])) continue;
    if (wantMs == null || !(c.durationMs > 0)) {
      if (!best) best = c.id; // search order is Spotify's relevance order
      continue;
    }
    const diff = Math.abs(c.durationMs - wantMs);
    if (diff <= MAX_DURATION_DIFF_MS && diff < bestDiff) {
      best = c.id;
      bestDiff = diff;
    }
  }
  return best;
}

const keyOf = (req) =>
  `${String(req.artist ?? "").toLowerCase().trim()}\u0000${String(req.title ?? "").toLowerCase().trim()}`;

/**
 * Title/artist → Spotify id, persisted. A miss is stored as id null so a song
 * Spotify does not have is not searched for on every play.
 */
function createIdCache(file, now = Date.now) {
  let entries = {};
  try {
    entries = JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    entries = {};
  }
  return {
    /** { id } when known (id null = known miss), undefined when never looked up */
    get(req) {
      const e = entries[keyOf(req)];
      if (!e) return undefined;
      if (e.id == null && now() - e.at > MISS_TTL_MS) return undefined;
      return { id: e.id };
    },
    set(req, id) {
      entries[keyOf(req)] = { id: id ?? null, at: now() };
      try {
        fs.writeFileSync(file, JSON.stringify(entries));
      } catch (e) {
        console.error("[lookup] could not persist ids:", e.message);
      }
    },
  };
}

module.exports = { pickTrack, createIdCache };
