/**
 * How finely a source knows the timing. Mirrors TIER in electron/main.cjs.
 *
 * The ladder decides everything: a coarser source is only used once no finer
 * one exists, whichever provider it came from, and `none` is reported as no
 * lyrics rather than shown untimed.
 */
export const TIER = { none: 0, line: 1, word: 2, syllable: 3 } as const;
export type Tier = (typeof TIER)[keyof typeof TIER];

export interface LyricWord {
  t: number;
  text: string;
  /** when this word stops being sung. Real value wherever the source has one. */
  end?: number;
  /** syllable-level unit that glues to the next without a separating space */
  joinNext?: boolean;
}

export interface LyricLine {
  t: number;
  text: string;
  words?: LyricWord[];
  /** when the line stops being sung, when the source tells us */
  end?: number;
  /** synthetic instrumental-gap marker rather than a real line */
  gap?: boolean;
  /** gap markers fade out shortly before the next line lands */
  hideAt?: number;
  /**
   * Backup vocal — rendered smaller and dimmer directly beneath the lead line
   * it belongs to. Its window usually overlaps the lead's, so both lines can be
   * current at once.
   */
  bg?: boolean;
  /** duet marker: this line is sung by the other voice, so it aligns right */
  opposite?: boolean;
}

function parseMmSs(min: string, sec: string): number {
  return parseInt(min) * 60 + parseFloat(sec);
}

// ── Credits ───────────────────────────────────────────────────
// NetEase ships songwriter credits as real timestamped lines at the head of the
// lrc track ("作词 : name"), so they parse as ordinary lyrics and end up being
// sung by the animator. spicy-lyrics never has this problem: its payload
// carries a structured SongWriters array, which it renders in a separate
// .Credits element rather than as a line. We reproduce that by lifting the
// credit lines out of the lyrics and handing them back separately.

/** `Role : names` — the shape NetEase and most Asian providers use. */
const CREDIT_ROLE =
  /^\s*(?:作词|作曲|编曲|填词|制作人|制作|监制|出品人|出品|发行|策划|统筹|企划|录音师|录音室|录音|混音|母带后期处理工程师|母带|和声|吉他|贝斯|鼓|键盘|钢琴|弦乐|词|曲|演唱|OP|SP|Lyricist|Composer|Arranger|Producer|Mixing|Mastering|Recording|Vocals?|Guitar|Bass|Drums|Piano|Keyboards?|Strings|Remix)\s*[:：]/i;

/** `Written by name` — the shape LrcLib entries and Western uploads use. */
const CREDIT_BY =
  /^\s*(?:written|lyrics|composed|music|arranged|produced|mixed|mastered|performed|recorded)\s+by\b/i;

/**
 * `短标签 : 值` — any short Han label followed by a colon.
 *
 * The explicit list above can only name roles someone thought of. NetEase adds
 * them freely and compounds them, so `混音` is listed but `混音助理` and
 * `混音工程师` are not, and neither are `器乐` or `音频工程师` — all of which
 * were being sung as lyrics at the end of a song.
 *
 * Kept narrow on purpose: the label must be Han, at most eight characters, and
 * carry a value after the colon. Combined with only ever scanning the two ends
 * of a song, a lyric that merely contains a colon is never at risk.
 */
const CREDIT_CJK_LABEL = /^\s*[一-鿿]{1,8}\s*[:：]\s*\S/;

/**
 * `names, role` — NetEase's English credit rows, role last:
 *
 *   作曲 : Kanye West/Keith Harris/William Adams
 *   Estelle -- American Boy (Ft. Kanye West)
 *   will.i.am, Estelle, John Legend, …, songwriters
 *   will.i.am, producer
 *
 * Catching the last two is what lets the header scan reach past the title row
 * between them and the first credit, so all four come off.
 */
const CREDIT_ROLE_LAST =
  /,\s*(?:songwriters?|writers?|lyricists?|composers?|producers?|co-producers?|arrangers?|engineers?|mixers?)\s*$/i;

function isCreditLine(text: string): boolean {
  const t = text.trim();
  if (!t) return false;
  return CREDIT_ROLE.test(t) || CREDIT_BY.test(t) || CREDIT_CJK_LABEL.test(t) || CREDIT_ROLE_LAST.test(t);
}

/**
 * Mark wholly-parenthesised lines as backing vocals.
 *
 * Only the syllable sources tag backing vocals structurally. Line-timed
 * providers convey the same thing by convention — a line that is entirely
 * inside brackets is the answering voice — so honouring that convention is the
 * only way these render as backing vocals on anything but a spicy payload.
 *
 * Deliberately strict: the brackets must wrap the *whole* line, and the opening
 * one must not close before the end, so a line that merely contains an aside
 * is left alone.
 *
 * Consecutive backing lines are allowed — spicy's own payload carries Background
 * as an array, so one lead can genuinely have several. What is required is that
 * a lead has appeared above, which is what stops a song opening on a bracketed
 * line from producing backing vocals with nothing to sit under.
 */
export function markParentheticalBackingVocals(lines: LyricLine[]): LyricLine[] {
  let seenLead = false;
  const out: LyricLine[] = [];

  for (const line of lines) {
    if (line.gap || line.bg) {
      out.push(line);
      continue;
    }

    const t = line.text.trim();
    const wrapped =
      (t.startsWith("(") && t.endsWith(")")) || (t.startsWith("[") && t.endsWith("]"));
    // Reject "(a) b (c)" — the opening bracket has to survive to the end.
    const closer = t[0] === "(" ? ")" : "]";
    const wrapsWholeLine =
      wrapped && t.length >= 3 && t.indexOf(closer) === t.length - 1;

    if (wrapsWholeLine) {
      out.push(seenLead ? { ...line, bg: true } : line);
      continue;
    }

    seenLead = true;
    const split = splitTrailingAside(line);
    if (!split) {
      out.push(line);
      continue;
    }
    out.push(split.lead, split.bg);
  }

  return out;
}

/** `lead words (answering voice)` — the aside has to close the line. */
const TRAILING_ASIDE = /^(.*\S)\s*\(([^()]{2,})\)$/;

/**
 * Where in `words` the trailing aside begins.
 *
 * Found by shape rather than by counting tokens: the estimator splits on
 * whitespace, but `yrc` and spicy payloads carry syllables, so a token count
 * taken from the text would cut in the wrong place. A run that opens with "("
 * and closes the line with ")" is the aside wherever the token boundaries fell.
 * Returns -1 when no run qualifies, which sends the caller down the
 * timing-free path rather than mangling the words.
 */
function asideWordIndex(words: LyricWord[]): number {
  for (let i = 1; i < words.length; i++) {
    if (!words[i].text.startsWith("(")) continue;
    const tail = words.slice(i).map((w) => w.text).join("");
    if (tail.endsWith(")")) return i;
  }
  return -1;
}

function stripBrackets(words: LyricWord[]): LyricWord[] {
  return words
    .map((w, i) => {
      let text = w.text;
      if (i === 0) text = text.replace(/^\(+/, "");
      if (i === words.length - 1) text = text.replace(/\)+$/, "");
      return { ...w, text };
    })
    .filter((w) => w.text);
}

/**
 * Split `lead (aside)` into a lead line and a backing-vocal line beneath it.
 *
 * Only the syllable sources tag backing vocals structurally, and only a
 * line-timed provider that puts the aside on its *own* line is caught by the
 * wholly-parenthesised rule above. LrcLib and NetEase overwhelmingly write the
 * answering voice inline instead, which printed the whole thing as one lead
 * line — "But think of you? (But think of you?)" — where the reference shows a
 * dimmed second line. Splitting here is what makes those two sources render the
 * same shape as a spicy payload's Background[] group.
 *
 * Timings come from the aside's own words when the line has them, so the
 * backing line lights up when it is actually sung rather than with the lead.
 * Without words both lines share the lead's window, which is the honest answer
 * for plain LRC.
 */
function splitTrailingAside(line: LyricLine): { lead: LyricLine; bg: LyricLine } | null {
  const m = line.text.trim().match(TRAILING_ASIDE);
  if (!m) return null;

  const leadText = m[1].trim();
  const asideText = m[2].trim();
  // A lead of punctuation alone is not a lead; leave the line as it was.
  if (!leadText || !asideText || !/[\p{L}\p{N}]/u.test(leadText)) return null;

  const words = line.words ?? [];
  const cut = words.length ? asideWordIndex(words) : -1;
  // The aside sits inside a word rather than between two, so there is no clean
  // cut. Splitting anyway would mean dropping this line's word timings, and
  // real per-word sync is worth more than the shape, so leave the line alone.
  if (words.length && cut < 1) return null;

  const leadWords = cut > 0 ? words.slice(0, cut) : [];
  const bgWords = cut > 0 ? stripBrackets(words.slice(cut)) : [];

  const lead: LyricLine = {
    ...line,
    text: leadText,
    ...(leadWords.length
      ? { words: leadWords, end: leadWords[leadWords.length - 1].end ?? line.end }
      : { words: undefined }),
  };

  const bg: LyricLine = {
    ...line,
    text: asideText,
    bg: true,
    t: bgWords.length ? bgWords[0].t : line.t,
    end: bgWords.length ? (bgWords[bgWords.length - 1].end ?? line.end) : line.end,
    ...(bgWords.length ? { words: bgWords } : { words: undefined }),
  };

  return { lead, bg };
}

/**
 * How far into the song a credit line can still count as part of the header.
 *
 * Bounded so a lyric that merely happens to contain "词：" halfway through a
 * song can never swallow everything above it.
 */
const HEADER_SCAN = 10;

/**
 * How many lines at the end may be taken as the closing credits block.
 *
 * A safety valve rather than a real limit: the run has to be contiguous from
 * the last line, so this only matters for a song whose actual final verse is a
 * long stack of colon lines. Better to leave a few credits on screen than to
 * eat the ending.
 */
const TRAILER_SCAN = 15;

/**
 * Non-credit lines the header may contain before it is judged to have ended —
 * enough for NetEase's title and artist rows, not enough to run into the song.
 */
const MAX_HEADER_GAP = 2;

/**
 * Split the metadata header off the front of a parsed lyric list.
 *
 * Providers put credits at the very top, but NetEase precedes them with lines
 * that are not credit-shaped at all — the track title and the artist's name in
 * its own script:
 *
 *   Ditto - NewJeans
 *   뉴진스
 *   词：Ylva Dimberg/…
 *   曲：250/Ylva Dimberg
 *   编曲：250
 *
 * Stopping at the first line that is not a credit meant the run ended at line 0
 * and the whole block was sung as lyrics. So the scan instead finds the *last*
 * credit line within the header window and drops everything above it too —
 * anchored on a real credit, so a song with no credits still loses nothing.
 *
 * Only the credit-shaped lines are returned for display; the title and artist
 * are dropped, since the widget is already showing both.
 */
export function extractCredits(lines: LyricLine[]): {
  lines: LyricLine[];
  credits: string[];
} {
  // Trailing block first. NetEase appends the engineering credits *after* the
  // final lyric — 器乐, 音频工程师, 混音助理, 混音工程师 — which were otherwise sung
  // as the closing lines. Taken as a contiguous run backwards from the end, so
  // a credit is only stripped when everything below it is one too.
  //
  // Order matters: doing this before the header scan is what stops a song with
  // only a line or two of lyrics from having its closing credits mistaken for
  // header metadata and the whole song discarded with them.
  let end = lines.length;
  const floor = Math.max(0, lines.length - TRAILER_SCAN);
  while (end > floor && isCreditLine(lines[end - 1].text)) end--;

  // Then the header, tolerating the couple of non-credit lines NetEase puts
  // above the credits (the track title and the artist's native-script name) but
  // stopping once real lyrics start. A flat "last credit in the first N lines"
  // was wrong: on a short song N reaches the closing credits, and the whole
  // song is treated as header and thrown away.
  let start = 0;
  let lastHeader = -1;
  let run = 0;
  for (let i = 0; i < Math.min(end, HEADER_SCAN); i++) {
    if (isCreditLine(lines[i].text)) {
      lastHeader = i;
      run = 0;
    } else if (++run > MAX_HEADER_GAP) {
      break;
    }
  }
  if (lastHeader >= 0) start = Math.min(lastHeader + 1, end);

  if (start === 0 && end === lines.length) return { lines, credits: [] };

  return {
    lines: lines.slice(start, end),
    credits: [...lines.slice(0, start), ...lines.slice(end)]
      .filter((l) => isCreditLine(l.text))
      .map((l) => l.text.trim()),
  };
}

// ── LRC parser ────────────────────────────────────────────────
// Handles both standard LRC and enhanced word-level LRC:
//   [mm:ss.xx] line text
//   [mm:ss.xx] <mm:ss.xx>word1 <mm:ss.xx>word2 ...
export function parseLrc(lrc: string): LyricLine[] {
  const lines: LyricLine[] = [];
  // A timestamped line with no text is how LRC marks the moment singing stops —
  // the end of a verse, before an instrumental. It is the only real EndTime an
  // LRC file carries, and spicy's interlude test is built entirely on EndTime
  // (`next.StartTime - line.EndTime >= 3s`). Discarding these was what forced
  // us to guess an end back, and a guessed end invents interludes inside any
  // verse sung more slowly than the song's average.
  const endMarks: number[] = [];

  for (const raw of lrc.split("\n")) {
    const lineM = raw.match(/\[(\d+):(\d+(?:\.\d+)?)\](.*)/);
    if (!lineM) continue;
    const t = parseMmSs(lineM[1], lineM[2]);
    const body = lineM[3].trim();
    if (!body) {
      endMarks.push(t);
      continue;
    }

    // word-level: <mm:ss.xx>word tokens
    const wordTokens = [...body.matchAll(/<(\d+):(\d+(?:\.\d+)?)>([^<]*)/g)];
    if (wordTokens.length) {
      const words: LyricWord[] = wordTokens
        .map((m) => ({ t: parseMmSs(m[1], m[2]), text: m[3].trim() }))
        .filter((w) => w.text);
      // Enhanced LRC only marks starts, so a word runs until the next begins.
      for (let i = 0; i < words.length - 1; i++) words[i].end = words[i + 1].t;
      const text = words.map((w) => w.text).join(" ");
      lines.push({ t, text, words });
    } else {
      // strip any stray <...> tags (malformed enhanced LRC)
      const text = body.replace(/<[^>]+>/g, "").trim();
      if (text) lines.push({ t, text });
    }
  }

  // Word timings are left absent unless the file actually carried them as
  // <mm:ss.xx> tags. Spreading words evenly across a line only looked
  // word-synced — the highlight drifted within every line, since words are not
  // sung at a constant rate. Without them the renderer highlights whole lines,
  // which is honest about what the source knows.
  lines.sort((a, b) => a.t - b.t);
  endMarks.sort((a, b) => a - b);

  // A marker seals the line it follows, and only while it still falls before
  // the next line starts: a marker further down the file belongs to a later
  // line, and one sitting above the first lyric (files often open with one)
  // belongs to no line at all. Lines left unsealed genuinely have no end in
  // this source, and are treated as such rather than given a fabricated one.
  for (let i = 0; i < lines.length; i++) {
    const next = lines[i + 1];
    const mark = endMarks.find((m) => m > lines[i].t && (!next || m <= next.t));
    if (mark != null) lines[i].end = mark;
  }

  return lines;
}

/**
 * Scripts that need no spaces between words.
 *
 * Kana, Han and Hangul are written continuously, so a token boundary in the
 * source is already a real unit — usually one syllable. Latin text is the
 * opposite: NetEase splits a single word across several tokens, which is why
 * they have to be fused.
 */
const SELF_DELIMITING =
  /[぀-ゟ゠-ヿ㐀-䶿一-鿿豈-﫿가-힯]/;

/**
 * NetEase's word-by-word karaoke format.
 *
 *   [lineStartMs,lineDurationMs](wordStartMs,wordDurationMs,0)text(...)text
 *
 * Any leading JSON metadata lines are skipped by the shape of the line regex.
 * Latin tokens that don't end in whitespace are fragments of the following word,
 * so they're fused — our highlighting is word-level, not syllable-level.
 *
 * That fusing rule must not be applied to Korean, Chinese or Japanese. Those
 * lines contain no spaces at all, so "fuse until whitespace" swallowed every
 * token into a single word per line and discarded the real per-syllable timings
 * underneath — leaving one linear sweep across the line, which reads as the
 * whole line being out of sync with the singing.
 */
export function parseYrc(yrc: string): LyricLine[] {
  const lines: LyricLine[] = [];

  for (const raw of yrc.split("\n")) {
    const head = raw.match(/^\[(\d+),(\d+)\](.*)$/);
    if (!head) continue;

    const start = parseInt(head[1]) / 1000;
    const duration = parseInt(head[2]) / 1000;
    const tokens = [...head[3].matchAll(/\((\d+),(\d+),\d+\)([^(]*)/g)];
    if (!tokens.length) continue;

    const words: LyricWord[] = [];
    let buffer = "";
    let bufferStart = 0;
    let bufferEnd = 0;

    for (const [, tStart, tDur, text] of tokens) {
      if (!buffer) bufferStart = parseInt(tStart) / 1000;
      // a fused word ends when its final fragment ends
      bufferEnd = (parseInt(tStart) + parseInt(tDur)) / 1000;
      buffer += text;

      const trimmed = buffer.trim();
      // trailing whitespace marks a word boundary in space-delimited scripts;
      // in kana/Han/Hangul every token is already its own unit
      const spaced = /\s$/.test(text);
      const selfDelimited =
        !!trimmed && SELF_DELIMITING.test(trimmed[trimmed.length - 1]);

      if (spaced || selfDelimited) {
        if (trimmed) {
          words.push({
            t: bufferStart,
            end: bufferEnd,
            text: trimmed,
            // No space separated these, so the renderer must not insert one —
            // otherwise the line would be printed one character apart.
            joinNext: !spaced,
          });
        }
        buffer = "";
      }
    }
    if (buffer.trim()) {
      words.push({ t: bufferStart, end: bufferEnd, text: buffer.trim() });
    }
    if (!words.length) continue;

    // Rebuild the display text honouring joinNext, so unspaced scripts read as
    // continuous text rather than as spaced-out characters.
    let text = "";
    for (let i = 0; i < words.length; i++) {
      text += words[i].text;
      if (i < words.length - 1 && !words[i].joinNext) text += " ";
    }

    lines.push({ t: start, end: start + duration, text, words });
  }

  return lines.sort((a, b) => a.t - b.t);
}

export interface LoadedLyrics {
  lines: LyricLine[];
  synced: boolean;
  /** true when word timings were estimated rather than supplied by the source */
  wordsEstimated?: boolean;
  /** songwriter/production credits, lifted out of the lines — see extractCredits */
  credits?: string[];
  /** finest timing this result carries — see TIER */
  tier: Tier;
  source: string;
}

/**
 * Give line-timed lyrics estimated word timings.
 *
 * Plain LRC marks only where each line starts, so without this there is nothing
 * to animate word by word. Weighting each word by its length is much closer than
 * the uniform split I used before — "I" and "everything" plainly don't take the
 * same time to sing — but it is still an estimate, and the flag says so.
 *
 * Skipped where it would be nonsense: single-word lines, and spans so long the
 * line is clearly followed by an instrumental rather than sung throughout.
 */
function charsIn(text: string): number {
  return text.replace(/\s+/g, "").length;
}

/**
 * Seconds per character, estimated from the song itself.
 *
 * A line is almost never sung right up to the moment the next one starts —
 * there is usually a breath or a bar of music after it. Stretching a line's
 * words all the way to the next line therefore makes every word land late,
 * worst at the end of the line.
 *
 * The densest lines are the ones sung with little trailing pause, so a low
 * percentile of "gap per character" approximates the true singing rate. Taking
 * it per song adapts to ballads and fast rap alike.
 */
function estimateRate(lines: LyricLine[]): number {
  const samples: number[] = [];
  for (let i = 0; i < lines.length - 1; i++) {
    const chars = charsIn(lines[i].text);
    const gap = lines[i + 1].t - lines[i].t;
    if (chars >= 4 && gap > 0.4 && gap <= 8) samples.push(gap / chars);
  }
  if (!samples.length) return 0.11; // ~9 characters a second
  samples.sort((a, b) => a - b);
  return samples[Math.floor(samples.length * 0.3)];
}

export function estimateWordTimings(lines: LyricLine[]): LyricLine[] {
  const MAX_SUNG_SPAN = 12; // seconds
  const rate = estimateRate(lines);

  return lines.map((line, i) => {
    if (line.words?.length) return line;

    const tokens = line.text.split(/\s+/).filter(Boolean);
    if (tokens.length < 2) return line;

    const nextStart = lines[i + 1]?.t;
    const toNext = (nextStart ?? line.t + 4) - line.t;
    // How long this line is actually sung — never longer than the room
    // available before the next line.
    const sung = Math.min(Math.max(charsIn(line.text) * rate, 0.5), toNext);
    const span = Math.min(line.end != null ? line.end - line.t : sung, MAX_SUNG_SPAN);
    if (span <= 0.2) return line;

    // +1 per word so single letters still occupy a slot
    const weights = tokens.map((w) => w.length + 1);
    const total = weights.reduce((a, b) => a + b, 0);

    let cursor = line.t;
    const words: LyricWord[] = tokens.map((text, wi) => {
      const dur = (weights[wi] / total) * span;
      const word = { t: cursor, end: cursor + dur, text };
      cursor += dur;
      return word;
    });

    return { ...line, words };
  });

  // Also set end on lines that have words but no real end time.
  return lines.map((line) => {
    if (line.end != null || !line.words?.length) return line;
    const lastWord = line.words[line.words.length - 1];
    return { ...line, end: lastWord.t + 0.5 };
  });
}

/**
 * Fetch lyrics for the current track through the Electron main process, which
 * is not subject to browser CORS (LrcLib is the only source that would work
 * from the renderer directly).
 */
export async function loadLyrics(req: {
  artist: string;
  title: string;
  album?: string;
  duration?: number;
  geniusKey?: string;
  /** ask just this provider ("netease" | "lrclib") instead of the whole chain */
  only?: string;
}): Promise<LoadedLyrics | null> {
  const bridge = window.player;
  if (!bridge) return null;

  const res = await bridge.fetchLyrics(req);
  if (!res) return null;

  // Best available first: syllable timing, then word, then line.
  if (res.wordLyrics) {
    const { lines, credits } = extractCredits(parseYrc(res.wordLyrics));
    if (lines.length) {
      return {
        lines,
        credits,
        synced: true,
        tier: TIER.syllable,
        source: `${res.source}/yrc`,
      };
    }
  }
  if (res.syncedLyrics) {
    // Credits come off before anything else: they would otherwise skew the
    // per-song singing-rate estimate, since a credit line is followed by a gap
    // that has nothing to do with how fast the song is sung.
    const { lines, credits } = extractCredits(parseLrc(res.syncedLyrics));
    if (lines.length) {
      // Enhanced LRC already carries words with real end times. Plain LRC gets
      // estimates so the word animation still has something to follow.
      const hasWords = lines.some((l) => l.words?.length);
      return hasWords
        ? { lines, credits, synced: true, tier: TIER.word, source: res.source }
        : {
            // Line-timed is the floor we still animate: word windows are
            // calculated from the line so there is something to follow, and
            // wordsEstimated tells the renderer they were calculated, not sung.
            lines: estimateWordTimings(lines),
            credits,
            synced: true,
            wordsEstimated: true,
            tier: TIER.line,
            source: res.source,
          };
    }
  }
  // Untimed lyrics are deliberately not used — they cannot be followed, and
  // faking timings for them is what made the highlight drift.
  return null;
}
