// The pure line-state rules of the widget (lyric gaps, which lines are active,
// time labels, per-letter splitting), moved verbatim out of App.tsx so they run
// without React or the DOM: scripts/golden/cases/line_state.mts runs them for
// the Rust port (crates/playback-logic/src/line_state.rs). The functions under
// "extracted from App.tsx" at the end were inline expressions there; App.tsx now
// calls them.
import { markParentheticalBackingVocals, type LyricLine } from "./lyrics/lyrics";

/** the next-song popup shows for this many seconds before the song ends */
export const NEXT_POPUP_SECS = 10;

/**
 * Insert ellipsis markers into instrumental stretches.
 *
 * Mirrors spicy-lyrics' interlude detection: gaps ≥ getLyricsBetweenShow() (3s)
 * at the start or between lines earn a musical-line with three dots. The dot
 * timings use a padding offset so they don't snap exactly on the boundary.
 */
export const MIN_GAP = 3; // getLyricsBetweenShow() — interlude threshold
export const PRE_HIDDEN_DOT_MS = 500; // matches spicy-lyrics' preHiddenDotLineMs
export const INTERLUDE_PADDING = -(PRE_HIDDEN_DOT_MS + 50) / 1000; // -0.55s

/**
 * Three dots with back-to-back windows, matching Syllable.ts's musical-line:
 * each dot ends where the next begins, and the last one stops a little early so
 * the whole marker has time to clear before the next line lands.
 */
export function makeGap(from: number, to: number): LyricLine {
  const totalTime = to - from;
  const baseDotTime = totalTime / 3;
  const dotPadding = INTERLUDE_PADDING / 3;
  const d1 = Math.max(from, from + baseDotTime + dotPadding);
  const d2 = Math.max(d1, from + baseDotTime * 2 + dotPadding * 2);
  const d3 = Math.max(d2, from + totalTime + INTERLUDE_PADDING);

  return {
    t: from,
    end: to,
    gap: true,
    hideAt: Math.max(from, to - PRE_HIDDEN_DOT_MS / 1000),
    text: "• • •",
    words: [
      { t: from, end: d1, text: "•" },
      { t: d1, end: d2, text: "•" },
      { t: d2, end: d3, text: "•" },
    ],
  };
}

/**
 * Where a line stops being sung.
 *
 * Only ever answered from something the source actually said. spicy-lyrics
 * reads EndTime off the line object directly and its whole interlude test rests
 * on that number, so the one thing we must not do is supply a number of our own:
 * a guessed end that lands early reads as an instrumental, and dots appear in
 * the middle of a verse that is merely sung slowly.
 *
 * `estimated` says the words on this line were calculated from the line itself
 * (see estimateWordTimings), which makes reading an end back off them circular.
 */
export function lineEndOf(line: LyricLine, next: LyricLine, estimated: boolean): number {
  if (line.end != null) return line.end;
  if (!estimated && line.words?.length) {
    const lastWord = line.words[line.words.length - 1];
    return Math.min(lastWord.end ?? lastWord.t + 1.2, next.t);
  }
  // Nothing in the source says when this line stops, so it runs until the next
  // one starts — which is also what spicy does to a line whose gap falls under
  // the threshold. A source that only knows starts cannot imply an interlude.
  return next.t;
}

/**
 * Insert ellipsis markers into instrumental stretches.
 *
 * Mirrors spicy-lyrics' interlude detection: gaps >= getLyricsBetweenShow() (3s)
 * at the start or between lines earn a musical-line with three dots.
 *
 * Backup-vocal lines are invisible to this pass. They live inside their lead's
 * window, so counting them as lines would both hide real interludes and drop a
 * marker between a lead and its own backing vocal.
 */
export function withGaps(lines: LyricLine[], estimated: boolean): LyricLine[] {
  if (!lines.length) return lines;

  const out: LyricLine[] = [];

  // Intro gap — spicy-lyrics checks data.StartTime >= getLyricsBetweenShow()
  if (lines[0].t >= MIN_GAP) {
    out.push(makeGap(0, lines[0].t));
  }

  // A gap is discovered at its opening lead line but must be emitted *after*
  // that lead's backing vocals, so it is held until the next lead arrives.
  let pendingGap: LyricLine | null = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.bg && pendingGap) {
      out.push(pendingGap);
      pendingGap = null;
    }
    out.push(line);
    if (line.bg) continue;

    let j = i + 1;
    while (j < lines.length && lines[j].bg) j++;
    const next = lines[j];
    if (!next) continue;

    // Spicy-lyrics threshold: arr[index+1].StartTime - line.EndTime >= getLyricsBetweenShow()
    const end = lineEndOf(line, next, estimated);
    if (next.t - end >= MIN_GAP) pendingGap = makeGap(end, next.t);
  }

  if (pendingGap) out.push(pendingGap);
  return out;
}

/**
 * Guarantee every line has an end time.
 *
 * Sources that only mark line starts (plain LRC) leave `end` undefined, and an
 * undefined end makes every "is this line Active?" test fail outright — which
 * pushed all of them down the "most recent line that started" fallback, so a
 * line stayed lit long past the point it was actually sung.
 */
export function sealEnds(lines: LyricLine[], estimated: boolean): LyricLine[] {
  return lines.map((line, i) => {
    if (line.end != null) return line;
    let j = i + 1;
    while (j < lines.length && lines[j].bg) j++;
    const next = lines[j] ?? { t: line.t + 4, text: "" };
    return { ...line, end: lineEndOf(line, next, estimated) };
  });
}

/**
 * Forward lead, compensating for the gap between the reported position and what
 * actually reaches the speakers. spicy-lyrics ships PROGRESS_POSITION_OFFSET =
 * 100 ms for this; ours stays at half that on purpose.
 *
 * Two reasons it should not be copied straight across. SMTC reports Spotify's
 * decoded position, which already sits later in the pipeline than the clock
 * spicy reads. And the fill sweep no longer runs through a spring, so the lag
 * this used to be partly cancelling is gone — raising the lead now would land
 * the sweep ahead of the beat rather than on it.
 *
 * The user's [ / ] nudge stacks on top.
 */
export const LYRIC_LEAD = 0.05;

/** Depth cue: each line of distance from the playhead adds this much bloom. */
export const BLUR_STEP = 0.55; // px
export const BLUR_MAX = 2.2; // px


export interface ActiveLines {
  /** every line that is Active right now, in document order */
  active: number[];
  /** the one line the scroller and the depth cue key off; -1 before playback */
  anchor: number;
}

/**
 * Which lines are current right now.
 *
 * spicy-lyrics asks every line "are you Active?" instead of searching for a
 * single winner, which is what lets a lead line and its backing vocal light up
 * together. The anchor prefers the lead: a backing vocal should never pull the
 * view onto itself.
 */

export function resolveActive(lines: LyricLine[], t: number): ActiveLines {
  const active: number[] = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.end != null && t >= line.t && t < line.end) active.push(i);
  }
  if (active.length) {
    const lead = active.find((i) => !lines[i].bg);
    return { active, anchor: lead ?? active[0] };
  }
  // Nothing Active — hold the view on the most recent line that started, so it
  // does not snap between lines or after the last one.
  for (let i = lines.length - 1; i >= 0; i--) {
    if (t >= lines[i].t) return { active, anchor: lines[i].bg ? Math.max(0, i - 1) : i };
  }
  return { active, anchor: -1 };
}

/** Interlude dots clear out shortly before the next line lands. */
export function isGapHiding(line: LyricLine | undefined, t: number): boolean {
  return !!line?.gap && line.hideAt != null && t >= line.hideAt;
}


export function fmt(s: number, remaining = false): string {
  if (!isFinite(s)) return remaining ? "-0:00" : "0:00";
  const abs = Math.abs(s);
  const m = Math.floor(abs / 60);
  const sec = Math.floor(abs % 60);
  const str = `${m}:${sec.toString().padStart(2, "0")}`;
  return remaining ? `-${str}` : str;
}


// Scripts that join or reorder their glyphs cannot be split into per-letter
// boxes without destroying the shaping, so those words stay whole.
// (escapes, not literals — the literal ranges include combining marks)
export const UNSPLITTABLE = new RegExp(
  "[" +
    "\\u0590-\\u05FF" + // Hebrew
    "\\u0600-\\u06FF\\u0750-\\u077F\\u08A0-\\u08FF" + // Arabic
    "\\u0900-\\u097F\\u0980-\\u09FF" + // Devanagari, Bengali
    "\\u0B80-\\u0BFF" + // Tamil
    "\\u0E00-\\u0E7F" + // Thai
    "\\u1780-\\u17FF" + // Khmer
    "]",
);

// Grapheme clusters, so an emoji or a combining accent stays one unit.
const segmenter =
  typeof Intl !== "undefined" && "Segmenter" in Intl
    ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
    : null;

export function splitForWave(text: string): string[] | null {
  if (text.length < 2 || UNSPLITTABLE.test(text)) return null;
  const chars = segmenter
    ? Array.from(segmenter.segment(text), (s) => s.segment)
    : Array.from(text);
  return chars.length > 1 ? chars : null;
}

// ── extracted from App.tsx (were inline expressions; App.tsx calls these) ──

/** The clock the lyrics follow: the player position plus the user's nudge and the lead (App.tsx tick). */
export function lyricTimeOf(position: number, offset: number): number {
  return position + offset + LYRIC_LEAD;
}

/** Whether the next-song popup is due (App.tsx tick). */
export function isNearEnd(dur: number, next: number): boolean {
  return dur > NEXT_POPUP_SECS * 2 && dur - next <= NEXT_POPUP_SECS && next > 0;
}

/** The tail of the dynamicLyrics memo: gaps only make sense against real timings. */
export function finishLines(base: LyricLine[], synced: boolean | undefined, lineSweep: boolean): LyricLine[] {
  if (synced === false) return base;
  return withGaps(markParentheticalBackingVocals(sealEnds(base, lineSweep)), lineSweep);
}

/** The first line that is neither an interlude marker nor a backing vocal (-1 if none). */
export function firstLyricIndex(lines: LyricLine[]): number {
  return lines.findIndex((l) => !l.gap && !l.bg);
}

/**
 * A backing vocal shares its lead's depth, so the pair defocuses together
 * rather than the bg line reading as one step further away.
 */
export function leadIndexes(lines: LyricLine[]): number[] {
  const map = new Array<number>(lines.length);
  let lead = 0;
  for (let i = 0; i < lines.length; i++) {
    if (!lines[i].bg) lead = i;
    map[i] = lead;
  }
  return map;
}

/** What the render map decides for line `i`: current, past (sung), intro preview. */
export function lineFlags(
  lines: LyricLine[],
  i: number,
  isSynced: boolean,
  activeSet: Set<number>,
  elapsed: number,
  firstLyricIdx: number,
): { isCurrent: boolean; isPast: boolean; isIntroPreview: boolean } {
  const line = lines[i];
  const isCurrent = isSynced && activeSet.has(i);
  // Sung, in spicy's sense: past its own end, not merely above
  // the playhead. A backing vocal can end before its lead does.
  const isPast = isSynced && !isCurrent && line.end != null && elapsed >= line.end;
  // The opening lyric, up until the moment it is sung.
  const isIntroPreview = isSynced && i === firstLyricIdx && !isCurrent && elapsed < line.t;
  return { isCurrent, isPast, isIntroPreview };
}

/**
 * Depth cue — bloom grows with distance from the playhead.
 * The intro preview is exempt: defocusing the one line that is
 * meant to be readable would undo the point of lifting it.
 */
export function lineBlur(
  blurLines: boolean,
  isSynced: boolean,
  currentIdx: number,
  isIntroPreview: boolean,
  leadOf: number[],
  i: number,
): number {
  return blurLines && isSynced && currentIdx >= 0 && !isIntroPreview
    ? Math.min(BLUR_MAX, Math.abs(leadOf[i] - leadOf[currentIdx]) * BLUR_STEP)
    : 0;
}
