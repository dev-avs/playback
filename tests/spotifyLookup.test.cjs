const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { pickTrack, createIdCache } = require("../electron/spotifyLookup.cjs");

const cand = (id, name, artists, durationMs) => ({ id, name, artists, durationMs });

test("picks the candidate whose title and artist match", () => {
  const id = pickTrack({ title: "Boys Don't Cry", artist: "The Cure" }, [
    cand("a", "Boys Don't Cry", ["Some Cover Band"], 157000),
    cand("b", "Boys Don't Cry", ["The Cure"], 157053),
  ]);
  assert.equal(id, "b");
});

test("prefers the duration closest to what is playing", () => {
  const id = pickTrack({ title: "c u next tuesday", artist: "Kesha", duration: 225.3 }, [
    cand("animal", "c u next tuesday", ["Kesha"], 232440),
    cand("cannibal", "c u next tuesday", ["Kesha"], 225346),
  ]);
  assert.equal(id, "cannibal");
});

test("tolerates version suffixes and featured artists", () => {
  const id = pickTrack({ title: "Sunflower (Spider-Man: Into the Spider-Verse)", artist: "Post Malone & Swae Lee" }, [
    cand("x", "Sunflower - Spider-Man: Into the Spider-Verse", ["Post Malone", "Swae Lee"], 158040),
  ]);
  assert.equal(id, "x");
});

test("returns null when nothing matches the artist", () => {
  const id = pickTrack({ title: "Boys Don't Cry", artist: "The Cure" }, [
    cand("a", "Boys Don't Cry", ["Rihanna"], 157000),
  ]);
  assert.equal(id, null);
});

test("rejects a match whose duration is far off", () => {
  const id = pickTrack({ title: "Intro", artist: "The xx", duration: 128 }, [
    cand("a", "Intro", ["The xx"], 400000),
  ]);
  assert.equal(id, null);
});

test("id cache remembers hits and misses across reloads", (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "player-idcache-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const file = path.join(dir, "ids.json");

  const a = createIdCache(file);
  a.set({ title: "Boys Don't Cry", artist: "The Cure" }, "b");
  a.set({ title: "Unknown", artist: "Nobody" }, null);

  const b = createIdCache(file);
  assert.equal(b.get({ title: "boys don't cry", artist: "the cure" }).id, "b");
  assert.equal(b.get({ title: "Unknown", artist: "Nobody" }).id, null);
  assert.equal(b.get({ title: "Never", artist: "Seen" }), undefined);
});

test("a remembered miss expires so the song is looked up again later", (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "player-idcache-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  let now = 1_000_000;
  const cache = createIdCache(path.join(dir, "ids.json"), () => now);
  cache.set({ title: "Unknown", artist: "Nobody" }, null);
  now += 8 * 24 * 3600_000;
  assert.equal(cache.get({ title: "Unknown", artist: "Nobody" }), undefined);
});
