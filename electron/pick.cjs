// Choosing which NetEase search result is actually the track being played.
//
// The old rule was "first result whose name contains the title, else the first
// result" — artist and duration were never consulted. NetEase's catalogue for
// non-Chinese pop is mostly covers, remixes, sped-up edits and karaoke
// instrumentals, so that rule routinely returned a different recording of the
// same song. Lyrics from a different recording cannot line up: a 183-second
// cover of a 269-second track drifts further apart with every line, which is
// what "the lyrics are out of sync" looks like from the outside.
//
// So a candidate now has to earn its place, and when none does we return null
// and let LrcLib's line-timed lyrics for the right recording win instead.

/** Recordings whose timings cannot match the track even when the title does. */
const KARAOKE = /karaoke|instrumental|伴奏|inst\.|off vocal/i;
const REWORKED = /remix|edit|sped ?up|slowed|nightcore|mashup|version/i;
const COVER = /cover|翻自|翻唱/i;

/** Duration is in seconds; NetEase reports milliseconds. */
function scoreSong(song, { artist, track, duration }) {
  const name = (song?.name ?? "").toLowerCase();
  const wanted = (track ?? "").toLowerCase();
  const wantedArtist = (artist ?? "").toLowerCase();
  let score = 0;

  if (!name) return -Infinity;
  if (name === wanted) score += 40;
  else if (name.includes(wanted) || wanted.includes(name)) score += 20;
  else return -Infinity; // not this song at all

  const artists = (song.artists ?? []).map((a) => (a.name ?? "").toLowerCase());
  // Exact is worth far more than partial: NetEase re-uploads impersonate the
  // real act by appending punctuation ("周杰伦-", "周杰伦."), which is a
  // substring match every time. Only an exact credit clears the bar on its own.
  if (artists.some((a) => a === wantedArtist)) score += 30;
  else if (artists.some((a) => a.includes(wantedArtist) || wantedArtist.includes(a))) {
    score += 10;
  }

  // Length is the best signal that two entries are the same performance, but it
  // ranks rather than vetoes. Releases legitimately differ — album vs single,
  // radio edit, or SMTC reporting a slightly different runtime — and an earlier
  // draft of this let a 31-second difference discard the only NetEase entry
  // carrying word-by-word timing for the track. A correctly credited title now
  // survives a length disagreement; a partial credit does not.
  if (duration && song.duration) {
    const delta = Math.abs(song.duration / 1000 - duration);
    if (delta <= 2) score += 30;
    else if (delta <= 5) score += 15;
    else if (delta <= 15) score += 0;
    else if (delta <= 30) score -= 15;
    else score -= 25;
  }

  // A karaoke track has no lead vocal, so its lyric track is absent or wrong.
  if (KARAOKE.test(name)) score -= 60;
  // Reworks keep the words and move every timestamp.
  else if (REWORKED.test(name)) score -= 30;
  // A cover sings the same words, so it is usable when the length agrees.
  else if (COVER.test(name)) score -= 10;

  return score;
}

/** Minimum score worth trusting — below this we would rather have no NetEase hit. */
const MIN_SCORE = 40;

function pickNeteaseSong(songs, want) {
  if (!Array.isArray(songs) || !songs.length) return null;
  let best = null;
  let bestScore = -Infinity;
  for (const song of songs) {
    const score = scoreSong(song, want);
    if (score > bestScore) {
      bestScore = score;
      best = song;
    }
  }
  return bestScore >= MIN_SCORE ? best : null;
}

module.exports = { pickNeteaseSong, scoreSong, MIN_SCORE };
