// On-disk lyrics cache.
//
// Why this exists: a cached entry needs neither a Spotify token nor Spotify
// itself. The token is minted by the Spotify client and lasts about an hour, so
// without this the player can only show syllable lyrics while Spotify is open
// and recently awake. With it, anything played once keeps working offline.
//
// Kept deliberately dumb: one gzipped file per Spotify track id, last-access
// tracked by the file's own mtime, and a byte budget enforced by deleting the
// least recently used files. No index file to corrupt, no expiry timer, and
// nothing that needs migrating — if the directory is deleted the player simply
// refetches.
//
// Payloads are stored in whichever shape they arrived in: `packed` straight
// from the API (the renderer owns the unpacking, see src/spicy.ts) or `lyrics`
// already unpacked from the Spicy Lyrics client's own cache.

const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

/**
 * Byte budget. Nothing is evicted until it is exceeded — there is no expiry.
 *
 * 10 GB is effectively unlimited for a person: a syllable payload is ~35 KB
 * raw and ~5 KB gzipped, so this is room for hundreds of thousands of tracks
 * against a lifetime library of maybe twenty thousand. It exists so a runaway
 * cannot fill the disk, not as a working limit.
 */
const MAX_BYTES = 10 * 1024 * 1024 * 1024;

/**
 * Refetch an entry older than this even when it is a cache hit, so upstream
 * corrections eventually land. 0 disables it, which is the default: lyrics are
 * served from disk forever until evicted.
 */
const REFRESH_AFTER_MS = 0;

const CACHE_VERSION = 1;

let dir = null;
let totalBytes = 0;
// Overridable so the eviction path can actually be exercised by a test — at the
// real budget it would never run, and untested code is where the bugs live.
let maxBytes = MAX_BYTES;

/** Spotify ids are base62. Anything else is not one, and must never reach the filesystem. */
function safeId(trackId) {
  return typeof trackId === "string" && /^[A-Za-z0-9]{1,64}$/.test(trackId);
}

function fileFor(trackId) {
  return path.join(dir, `${trackId}.json.gz`);
}

/**
 * Point the cache at a directory and total up what is already there.
 *
 * The scan is one stat per file — a few tens of milliseconds even at ten
 * thousand entries — and it happens once, so the running total afterwards is
 * just arithmetic.
 */
function init(userDataPath, budget = MAX_BYTES) {
  dir = path.join(userDataPath, "lyrics-cache");
  maxBytes = budget;
  try {
    fs.mkdirSync(dir, { recursive: true });
    totalBytes = 0;
    let count = 0;
    for (const name of fs.readdirSync(dir)) {
      if (!name.endsWith(".json.gz")) continue;
      try {
        totalBytes += fs.statSync(path.join(dir, name)).size;
        count++;
      } catch {
        /* vanished between readdir and stat — nothing to count */
      }
    }
    console.log(
      `[lyrics-cache] ${count} entries, ${(totalBytes / 1024 / 1024).toFixed(1)} MB` +
        ` of ${(maxBytes / 1024 / 1024 / 1024).toFixed(0)} GB`,
    );
  } catch (e) {
    console.error("[lyrics-cache] unusable, continuing without it:", e.message);
    dir = null;
  }
}

/**
 * Delete least-recently-used entries until the total is back under budget.
 *
 * Only runs when the budget is actually exceeded, which at these sizes means
 * essentially never — so it reads the directory fresh rather than keeping a
 * sorted structure permanently in memory for a path that will not be taken.
 */
function evict() {
  if (totalBytes <= maxBytes || !dir) return;

  const entries = [];
  for (const name of fs.readdirSync(dir)) {
    if (!name.endsWith(".json.gz")) continue;
    try {
      const st = fs.statSync(path.join(dir, name));
      entries.push({ name, size: st.size, at: st.mtimeMs });
    } catch {
      /* skip */
    }
  }
  entries.sort((a, b) => a.at - b.at); // oldest access first

  let removed = 0;
  for (const e of entries) {
    if (totalBytes <= maxBytes) break;
    try {
      fs.unlinkSync(path.join(dir, e.name));
      totalBytes -= e.size;
      removed++;
    } catch {
      /* already gone */
    }
  }
  if (removed) console.log(`[lyrics-cache] evicted ${removed} least-recently-used entries`);
}

/**
 * A cached payload, or null.
 *
 * Reading counts as use: the mtime is bumped so the eviction order is genuinely
 * least-recently-*used* rather than least-recently-written. A failure to bump it
 * is not worth failing the read over.
 */
function get(trackId) {
  if (!dir || !safeId(trackId)) return null;
  const file = fileFor(trackId);
  let raw;
  try {
    raw = fs.readFileSync(file);
  } catch {
    return null; // ordinary miss
  }

  let entry;
  try {
    entry = JSON.parse(zlib.gunzipSync(raw).toString("utf8"));
  } catch (e) {
    // Truncated by a crash mid-write, or written by a future version. Drop it
    // rather than let one bad file wedge the track forever.
    console.error(`[lyrics-cache] discarding unreadable entry ${trackId}: ${e.message}`);
    try {
      const size = fs.statSync(file).size;
      fs.unlinkSync(file);
      // Clamped: a file that appeared without going through put() was never
      // added to the total, and the running figure must not go negative. init()
      // rescans on every start, so any drift is corrected there.
      totalBytes = Math.max(0, totalBytes - size);
    } catch {
      /* already gone */
    }
    return null;
  }

  if (!entry || entry.v !== CACHE_VERSION || !entry.kind) return null;
  if (REFRESH_AFTER_MS > 0 && Date.now() - (entry.at ?? 0) > REFRESH_AFTER_MS) return null;

  const now = new Date();
  try {
    fs.utimesSync(file, now, now);
  } catch {
    /* mtime bump is best-effort */
  }

  return { kind: entry.kind, data: entry.data, clientVersion: entry.clientVersion ?? null };
}

/**
 * Store a payload. `kind` is "packed" (straight from the API) or "lyrics"
 * (already unpacked, from the Spicy Lyrics client's cache).
 *
 * Written to a temporary name and renamed into place, so a crash mid-write
 * leaves the previous entry intact rather than a half-file the next read has to
 * detect and throw away.
 */
function put(trackId, kind, data, clientVersion = null) {
  if (!dir || !safeId(trackId) || data == null) return;
  const file = fileFor(trackId);
  const tmp = `${file}.${process.pid}.tmp`;

  try {
    const body = zlib.gzipSync(
      Buffer.from(JSON.stringify({ v: CACHE_VERSION, kind, at: Date.now(), data, clientVersion }), "utf8"),
    );

    let previous = 0;
    try {
      previous = fs.statSync(file).size;
    } catch {
      /* new entry */
    }

    fs.writeFileSync(tmp, body);
    fs.renameSync(tmp, file);
    totalBytes += body.length - previous;
    evict();
  } catch (e) {
    console.error(`[lyrics-cache] could not store ${trackId}: ${e.message}`);
    try {
      fs.unlinkSync(tmp);
    } catch {
      /* nothing to clean */
    }
  }
}

function stats() {
  if (!dir) return { entries: 0, bytes: 0, maxBytes };
  let entries = 0;
  try {
    for (const name of fs.readdirSync(dir)) if (name.endsWith(".json.gz")) entries++;
  } catch {
    /* unreadable */
  }
  return { entries, bytes: totalBytes, maxBytes };
}

module.exports = { init, get, put, stats, MAX_BYTES };
