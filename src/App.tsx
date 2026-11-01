import { flushSync } from "react-dom";
import { useState, useEffect, useRef, useCallback, useMemo, Fragment } from "react";
import "./App.css";
import {
  loadLyrics,
  markParentheticalBackingVocals,
  parseLrc,
  TIER,
  type LyricLine,
} from "./lyrics";
import {
  alignRomanizedToWords,
  detectLyricLang,
  hasHanScript,
  linesNeedRomanization,
  romanizeLines,
} from "./romanize";
import { loadSpicyLyrics, type SpicyAttribution } from "./spicy";
import { BAR_COUNT, useAudioBars } from "./useAudioBars";
import {
  collectLine,
  letterWindows,
  setAnimationStyle,
  type AnimationStyle,
  resetLine,
  stepLine,
  type WordUnit,
  setLyricMotion,
} from "./lyricAnimator";
import { useNowPlaying, appLabel } from "./useNowPlaying";
import {
  DEFAULTS,
  KEYS,
  PROVIDERS,
  fontStack,
  providerOf,
  songKey,
  tierDots,
  usePersisted,
  write,
  type Provider,
  MOTIONS,
  highlightVars,
  GLASS_TINTS,
  SWEEP_ANGLES,
  COVER_SHAPES,
  LYRIC_WEIGHTS,
  cjkStack,
  LOGO_ART,
} from "./settings";

const SPACER = 150;
const LYRIC_ANCHOR = 6;
// Playback's logo stands in for a missing cover (settings.ts)
const ART_FALLBACK = LOGO_ART;

// stable identity so the "lyrics for another track" case does not re-render
const EMPTY_LINES: LyricLine[] = [];
const EMPTY_CREDITS: string[] = [];

/**
 * Surfaces that handle their own pointer and must never begin a window drag.
 * Replaces the old `-webkit-app-region: no-drag` rules — the drag is ours now,
 * so the exclusions have to be ours too.
 */
const NO_DRAG_SELECTOR =
  "button, input, a, .scrubber, .lyrics-panel, .vol-pill, .art-wrap, .token-entry, [data-nodrag]";


// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;

/**
 * Insert ellipsis markers into instrumental stretches.
 *
 * Mirrors spicy-lyrics' interlude detection: gaps ≥ getLyricsBetweenShow() (3s)
 * at the start or between lines earn a musical-line with three dots. The dot
 * timings use a padding offset so they don't snap exactly on the boundary.
 */
const MIN_GAP = 3; // getLyricsBetweenShow() — interlude threshold
const PRE_HIDDEN_DOT_MS = 500; // matches spicy-lyrics' preHiddenDotLineMs
const INTERLUDE_PADDING = -(PRE_HIDDEN_DOT_MS + 50) / 1000; // -0.55s

/**
 * Three dots with back-to-back windows, matching Syllable.ts's musical-line:
 * each dot ends where the next begins, and the last one stops a little early so
 * the whole marker has time to clear before the next line lands.
 */
function makeGap(from: number, to: number): LyricLine {
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
function lineEndOf(line: LyricLine, next: LyricLine, estimated: boolean): number {
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
function withGaps(lines: LyricLine[], estimated: boolean): LyricLine[] {
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
function sealEnds(lines: LyricLine[], estimated: boolean): LyricLine[] {
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
const LYRIC_LEAD = 0.05;

/** Depth cue: each line of distance from the playhead adds this much bloom. */
const BLUR_STEP = 0.55; // px
const BLUR_MAX = 2.2; // px

interface ActiveLines {
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
/** How long a panning title rests at the start, and (twice over) at the end. */
const PAN_REST_MS = 1500;

/**
 * A title or artist too long for its row slides to show the rest, like Spicy
 * Lyrics' title marquee: it rests, slides until the last letter is at the
 * edge, rests, and slides back. At a steady speed (Settings → Text → Long
 * title speed), so a little overflow is over quickly and a lot takes longer;
 * text that fits stays still. Measured by a ResizeObserver (it also fires once
 * on observe), so a resized widget or a new song re-measures by itself.
 *
 * Once there and back when a song starts, then it rests at the start; while
 * the widget is hovered it keeps going, and when the pointer leaves it finishes
 * the pass it is on and stops at the start rather than snapping back.
 */
function PanText({
  className,
  text,
  hovered,
  speed,
}: {
  className: string;
  text: string;
  hovered: boolean;
  /** px per second */
  speed: number;
}) {
  const outer = useRef<HTMLSpanElement>(null);
  const inner = useRef<HTMLSpanElement>(null);
  const anim = useRef<Animation | null>(null);
  const hoveredRef = useRef(hovered);
  const [pan, setPan] = useState(0);

  useEffect(() => {
    hoveredRef.current = hovered;
  }, [hovered]);

  useEffect(() => {
    const o = outer.current;
    const i = inner.current;
    if (!o || !i) return;
    const ro = new ResizeObserver(() => setPan(Math.max(0, Math.ceil(i.scrollWidth - o.clientWidth))));
    ro.observe(o);
    ro.observe(i);
    return () => ro.disconnect();
  }, [text]);

  // One pass there and back: rest, slide, rest twice as long, slide back, rest.
  useEffect(() => {
    const i = inner.current;
    if (!i || pan <= 0) return;
    const travel = (pan / Math.max(1, speed)) * 1000;
    const pass = 4 * PAN_REST_MS + 2 * travel;
    const at = (ms: number) => ms / pass;
    const start = "translateX(0)";
    const end = `translateX(-${pan}px)`;
    const a = i.animate(
      [
        { transform: start, offset: 0 },
        { transform: start, offset: at(PAN_REST_MS) },
        { transform: end, offset: at(PAN_REST_MS + travel) },
        { transform: end, offset: at(3 * PAN_REST_MS + travel) },
        { transform: start, offset: at(3 * PAN_REST_MS + 2 * travel) },
        { transform: start, offset: 1 },
      ],
      { duration: pass, iterations: hoveredRef.current ? Infinity : 1 },
    );
    anim.current = a;
    return () => {
      a.cancel();
      anim.current = null;
    };
  }, [pan, speed, text]);

  useEffect(() => {
    const a = anim.current;
    if (!a?.effect) return;
    if (hovered) {
      a.effect.updateTiming({ iterations: Infinity });
      if (a.playState === "finished") a.play();
    } else {
      // finish the pass it is on: each pass ends back at the start
      a.effect.updateTiming({ iterations: (a.effect.getComputedTiming().currentIteration ?? 0) + 1 });
    }
  }, [hovered]);

  return (
    <span ref={outer} className={`${className} pan`}>
      {/* keyed on the text: a new song gets its own element and its own run */}
      <span key={text} ref={inner} className="pan-inner">
        {text}
      </span>
    </span>
  );
}

function resolveActive(lines: LyricLine[], t: number): ActiveLines {
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
function isGapHiding(line: LyricLine | undefined, t: number): boolean {
  return !!line?.gap && line.hideAt != null && t >= line.hideAt;
}

/** Writes text only when it differs, so an unchanged label costs no layout. */
function setText(el: HTMLElement | null, text: string) {
  if (el && el.textContent !== text) el.textContent = text;
}

function fmt(s: number, remaining = false): string {
  if (!isFinite(s)) return remaining ? "-0:00" : "0:00";
  const abs = Math.abs(s);
  const m = Math.floor(abs / 60);
  const sec = Math.floor(abs % 60);
  const str = `${m}:${sec.toString().padStart(2, "0")}`;
  return remaining ? `-${str}` : str;
}

/* Rendered a touch larger than the 13px it replaced: this glyph fills 17 of
   the 24 units where the old outline one spanned nearly the whole box, so the
   bigger container is what keeps its *drawn* size unchanged beside repeat. */
/** The provider, and for community syncs the people who made them, linked. */
function LyricsAttribution({ credit }: { credit: SpicyAttribution }) {
  const person = (p: { name: string; url?: string }) =>
    p.url ? (
      <button
        className="lyrics-attribution-link"
        data-nodrag
        onClick={(e) => {
          e.stopPropagation();
          window.player?.openLink?.(p.url!);
        }}
      >
        {p.name}
      </button>
    ) : (
      p.name
    );
  return (
    <div className="lyrics-attribution">
      <span>Lyrics from {credit.provider}</span>
      {credit.uploader && <span>Uploaded by {person(credit.uploader)}</span>}
      {credit.maker && <span>Made by {person(credit.maker)}</span>}
    </div>
  );
}

const IconShuffle = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
    <path transform="matrix(0.0106 0 0 -0.0106 1.757 19.6007)" d="M1385 1303V868C1385 832 1408 809 1442 809C1460 809 1475 816 1488 827L1751 1038C1780 1063 1781 1106 1752 1131L1487 1346C1474 1356 1459 1361 1442 1361C1407 1361 1385 1339 1385 1303ZM1434 1010C1472 1010 1503 1041 1503 1079C1503 1118 1472 1149 1434 1149H1263C1146 1149 1074 1114 988 1014L574 528C515 457 471 434 400 434H239C197 434 165 404 165 365C165 325 195 296 239 296H398C515 296 587 331 673 430L1088 916C1146 986 1190 1010 1267 1010ZM1386 135C1386 100 1408 78 1443 78C1460 78 1475 83 1488 93L1753 308C1782 333 1781 376 1752 401L1489 612C1476 623 1461 630 1443 630C1409 630 1386 607 1386 571ZM1435 429H1268C1191 429 1148 453 1089 523L674 1009C588 1109 516 1143 399 1143H240C196 1143 166 1114 166 1074C166 1035 198 1005 240 1005H402C472 1005 516 982 575 911L989 425C1075 325 1147 290 1264 290H1435C1473 290 1504 321 1504 360C1504 398 1473 429 1435 429Z" />
  </svg>
);
/**
 * Transport and volume glyphs, from the SF Symbols exports in scripts/icons/.
 *
 * Those arrive with a per-file viewBox many hundreds of units across and a
 * `scale(1, -1)` flip, and no two share a scale — dropping them in as-is would
 * render them at wildly different sizes. Each path is therefore left byte for
 * byte as exported and placed by a matrix that does the flip and normalises it:
 * measured tight bounding box, fitted into a common optical box (17 wide by 14
 * tall) preserving aspect, then centred on 12,12 of the 24-unit viewBox the rest
 * of the icons here already use.
 *
 * Height is what is held constant, so widths differ by glyph the way they
 * should — the skip icons are broader than play because they carry a bar as
 * well as a triangle. Regenerate with `node scripts/icons/gen-icons.mjs` if those change.
 */
const IconPrev = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
    <path transform="matrix(0.0178 0 0 -0.0178 1.7405 24.8601)" d="M128 393V1051C128 1093 151 1115 192 1115H303C344 1115 367 1094 367 1051V750C374 766 387 780 408 792L900 1081C918 1092 936 1098 955 1098C993 1098 1024 1070 1024 1017V424C1024 370 993 343 955 343C936 343 918 349 900 360L408 649C387 661 374 675 367 691V393C367 350 344 329 303 329H192C151 329 128 350 128 393Z" />
  </svg>
);
const IconPause = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
    <path transform="matrix(0.0137 0 0 -0.0137 3.4315 21.8904)" d="M256 211C194 211 165 245 165 294V1150C165 1199 194 1233 256 1233H387C450 1233 478 1199 478 1150V294C478 245 450 211 387 211ZM865 211C803 211 774 245 774 294V1150C774 1199 803 1233 865 1233H996C1058 1233 1086 1199 1086 1150V294C1086 245 1058 211 996 211Z" />
  </svg>
);
const IconPlay = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
    <path transform="matrix(0.0167 0 0 -0.0167 3.0221 24.0156)" d="M164 374C164 305 212 281 279 319L869 658C928 691 929 748 869 783L279 1123C213 1161 164 1138 164 1069Z" />
  </svg>
);
const IconNext = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
    <path transform="matrix(0.0138 0 0 -0.0138 1.8282 21.9714)" d="M1307 293C1307 241 1279 215 1229 215H1087C1036 215 1009 241 1009 293V682C1001 664 986 649 963 636L316 255C294 242 272 234 249 234C203 234 165 267 165 333V1107C165 1172 203 1205 249 1205C272 1205 294 1198 316 1184L963 804C986 791 1001 775 1009 758V1151C1009 1204 1036 1228 1087 1228H1229C1279 1228 1307 1203 1307 1151Z" />
  </svg>
);
const IconRepeat = () => (
  <svg
    width="13"
    height="13"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2.2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polyline points="17 1 21 5 17 9" />
    <path d="M3 11V9a4 4 0 0 1 4-4h14" />
    <polyline points="7 23 3 19 7 15" />
    <path d="M21 13v2a4 4 0 0 1-4 4H3" />
  </svg>
);
// The speaker sits left of centre to leave room for its sound waves, which
// follow the level: an x when muted, then one, two or three waves at up to
// 30%, up to 60% and above. Without a level (nothing playing) it shows full.
const VOLUME_WAVES = [
  "M14.9 9.5A3.5 3.5 0 0 1 14.9 14.5",
  "M17 7.4A6.5 6.5 0 0 1 17 16.6",
  "M19.1 5.3A9.5 9.5 0 0 1 19.1 18.7",
];
const IconVolume = ({ level }: { level?: number }) => {
  const pct = level == null ? 100 : Math.round(level * 100);
  const waves = pct <= 0 ? 0 : pct <= 30 ? 1 : pct <= 60 ? 2 : 3;
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
      <path transform="matrix(0.0079 0 0 -0.0079 0.6 17.7222)" d="M1383 -161C1449 -161 1496 -114 1496 -49V1485C1496 1550 1449 1603 1381 1603C1335 1603 1303 1583 1252 1534L826 1134C819 1128 811 1125 801 1125H514C379 1125 305 1050 305 906V534C305 389 379 315 514 315H801C811 315 819 312 826 306L1252 -98C1298 -142 1337 -161 1383 -161Z" />
      <g fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round">
        {pct <= 0 ? (
          <path d="M15.5 9L21.5 15M21.5 9L15.5 15" />
        ) : (
          VOLUME_WAVES.slice(0, waves).map((d) => <path key={d} d={d} />)
        )}
      </g>
    </svg>
  );
};
// Fullscreen controls: outward arrows to open, inward to close, and the layout
// switches. Same 24-unit box and stroke as IconLyrics.
const strokeIcon = (d: string) => () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d={d} />
  </svg>
);
// 14 units tall, centred on 12: the speaker's optical box, so the two sit level
const IconExpand = strokeIcon("M14 5h5v5M10 19H5v-5M19 5l-6 6M5 19l6-6");
const IconCollapse = strokeIcon("M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7");
const IconSwap = strokeIcon("M7 7h13M16 3l4 4-4 4M17 17H4M8 13l-4 4 4 4");
const IconDominant = strokeIcon("M4 4h4v4H4zM11 5h9M11 8h6M4 13h16M4 17h16M4 21h11");
const IconCover = strokeIcon("M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-5L5 21");

const IconLyrics = () => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <rect x="3" y="3" width="18" height="18" rx="3" />
    <line x1="7" y1="8" x2="17" y2="8" />
    <line x1="7" y1="12" x2="13" y2="12" />
    <line x1="7" y1="16" x2="15" y2="16" />
  </svg>
);

// Scripts that join or reorder their glyphs cannot be split into per-letter
// boxes without destroying the shaping, so those words stay whole.
// (escapes, not literals — the literal ranges include combining marks)
const UNSPLITTABLE = new RegExp(
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

function splitForWave(text: string): string[] | null {
  if (text.length < 2 || UNSPLITTABLE.test(text)) return null;
  const chars = segmenter
    ? Array.from(segmenter.segment(text), (s) => s.segment)
    : Array.from(text);
  return chars.length > 1 ? chars : null;
}

/**
 * Renders a line as animatable units.
 *
 * Every unit carries a real start and end, which is the whole point: a syllable
 * is Active only between its own two timestamps, not "until the next one
 * starts". Inferring the end from the following syllable made one stay lit
 * across any pause after it, which is what made the highlight look late.
 *
 * Two tiers, exactly as in spicy-lyrics:
 *   .word      — an ordinary syllable, animated as one box
 *   .wordGroup — a syllable held long enough to earn a letter wave; it still
 *                animates as a box, and its .letter children ripple inside it
 *
 * Interlude markers get .dot units instead, which have their own curves.
 */
function Units({
  line,
  sweep = false,
  animation,
}: {
  line: LyricLine;
  sweep?: boolean;
  /** Settings → Highlight → Word animation: which syllables wave letter by letter */
  animation: AnimationStyle;
}) {
  // A line with no word timings at all gets swept too — otherwise it would
  // render as static text with no animation whatsoever, which is what happened
  // to Chinese lines falling back to a source-supplied romanization.
  if ((sweep || !line.words?.length) && !line.gap) {
    // The whole line is one unit, swept top to bottom over its own window —
    // spicy's look for line-synced lyrics. Splitting it into per-word units and
    // wiping them left to right implied the source knew where each word fell,
    // which for a guessed timing it does not; a single vertical wipe says only
    // "this line, now", which is exactly what a line timestamp knows.
    // See the .word[data-sweep] rule in App.css.
    return (
      <span
        className="word"
        data-sweep="1"
        data-s={line.t.toFixed(3)}
        data-e={(line.end ?? line.t).toFixed(3)}
      >
        {line.text}
      </span>
    );
  }

  if (!line.words) return <>{line.text}</>;
  const words = line.words;

  if (line.gap) {
    return (
      <span className="dotGroup">
        {words.map((w, i) => (
          <span
            className="dot"
            key={i}
            data-s={w.t.toFixed(3)}
            data-e={(w.end ?? w.t).toFixed(3)}
          >
            {w.text}
          </span>
        ))}
      </span>
    );
  }

  return (
    <>
      {words.map((w, i) => {
        const next = words[i + 1];
        const start = w.t;
        // real end where the source gave one, else the next syllable, else the line
        const end = w.end ?? next?.t ?? line.end ?? start + 0.5;
        // the separating space sits outside the unit, so a scaled syllable
        // never drags its whitespace around with it
        const sep = i < words.length - 1 && !w.joinNext ? " " : null;
        // glued syllables scale toward their joint, see .partOfWord in App.css
        const part = w.joinNext ? " partOfWord" : "";

        // Syllables long enough for the chosen style wave letter by letter
        // (letterWindows); the rest, and scripts that cannot be split, pop as
        // a single unit.
        const letters = splitForWave(w.text);
        const win = letters ? letterWindows(start, end, letters.length, animation) : null;

        if (!letters || !win) {
          return (
            <Fragment key={i}>
              <span className={`word${part}`} data-s={start.toFixed(3)} data-e={end.toFixed(3)}>
                {w.text}
              </span>
              {sep}
            </Fragment>
          );
        }

        // The group shares its letters' shortened window, so the wave locks
        // into idle before the syllable's raw end and the next syllable's rise
        // reads as a continuation rather than a fresh start.
        return (
          <Fragment key={i}>
            <span
              className={`wordGroup${part}`}
              data-s={start.toFixed(3)}
              data-e={win.groupEnd.toFixed(3)}
            >
              {letters.map((ch, li) => (
                <span
                  className="letter"
                  key={li}
                  data-s={(start + li * win.per).toFixed(3)}
                  data-e={(start + (li + 1) * win.per).toFixed(3)}
                >
                  {ch}
                </span>
              ))}
            </span>
            {sep}
          </Fragment>
        );
      })}
    </>
  );
}

export default function App() {
  const { np, connected, getPosition, seek, togglePlay, setVolume } = useNowPlaying();

  const [elapsed, setElapsed] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [hovered, setHovered] = useState(false);

  // ── settings — edited in the settings window (right-click the widget) ──
  const [lyricSize] = usePersisted(KEYS.lyricSize, DEFAULTS.lyricSize);
  const [panSpeed] = usePersisted(KEYS.panSpeed, DEFAULTS.panSpeed);
  const [animationStyle] = usePersisted(KEYS.animationStyle, DEFAULTS.animationStyle);
  // the animator reads it every frame; the letter split reads it as a prop
  useEffect(() => setAnimationStyle(animationStyle), [animationStyle]);
  const [lyricFont] = usePersisted(KEYS.lyricFont, DEFAULTS.lyricFont);
  const [alwaysOnTop] = usePersisted(KEYS.alwaysOnTop, DEFAULTS.alwaysOnTop);
  const [blurLines] = usePersisted(KEYS.blurLines, DEFAULTS.blurLines);
  const [lyricsOnLaunch] = usePersisted(KEYS.lyricsOnLaunch, DEFAULTS.lyricsOnLaunch);
  const [autoSync] = usePersisted(KEYS.autoSync, DEFAULTS.autoSync);
  // click the right-hand time to flip between time left and the song's length
  const [showTotal, setShowTotal] = usePersisted(KEYS.showTotalTime, DEFAULTS.showTotalTime);
  const showTotalRef = useRef(showTotal);
  const [bgAlpha] = usePersisted(KEYS.bgAlpha, DEFAULTS.bgAlpha);
  const [lyricAlign] = usePersisted(KEYS.lyricAlign, DEFAULTS.lyricAlign);
  const [highlight] = usePersisted(KEYS.highlight, DEFAULTS.highlight);
  const [corners] = usePersisted(KEYS.corners, DEFAULTS.corners);
  const [showNext] = usePersisted(KEYS.showNext, DEFAULTS.showNext);
  const [showSource] = usePersisted(KEYS.showSource, DEFAULTS.showSource);
  const [showVisualizer] = usePersisted(KEYS.showVisualizer, DEFAULTS.showVisualizer);
  const [lyricMotion] = usePersisted(KEYS.lyricMotion, DEFAULTS.lyricMotion);
  const [hideWhenIdle] = usePersisted(KEYS.hideWhenIdle, DEFAULTS.hideWhenIdle);
  const [showChangelog] = usePersisted(KEYS.showChangelog, DEFAULTS.showChangelog);
  const [lyricWeight] = usePersisted(KEYS.lyricWeight, DEFAULTS.lyricWeight);
  const [lineSpacing] = usePersisted(KEYS.lineSpacing, DEFAULTS.lineSpacing);
  const [clickToJump] = usePersisted(KEYS.clickToJump, DEFAULTS.clickToJump);
  const [showLineTimes] = usePersisted(KEYS.showLineTimes, DEFAULTS.showLineTimes);
  const [scrollSpeed] = usePersisted(KEYS.scrollSpeed, DEFAULTS.scrollSpeed);
  const [coverBlur] = usePersisted(KEYS.coverBlur, DEFAULTS.coverBlur);
  const [coverShape] = usePersisted(KEYS.coverShape, DEFAULTS.coverShape);
  const [spinCover] = usePersisted(KEYS.spinCover, DEFAULTS.spinCover);
  const [allCaps] = usePersisted(KEYS.allCaps, DEFAULTS.allCaps);
  const [fontJa] = usePersisted(KEYS.fontJa, DEFAULTS.fontJa);
  const [fontKo] = usePersisted(KEYS.fontKo, DEFAULTS.fontKo);
  const [fontZh] = usePersisted(KEYS.fontZh, DEFAULTS.fontZh);
  const [lineHeight] = usePersisted(KEYS.lineHeight, DEFAULTS.lineHeight);
  const [letterSpacing] = usePersisted(KEYS.letterSpacing, DEFAULTS.letterSpacing);
  const [italicLyrics] = usePersisted(KEYS.italicLyrics, DEFAULTS.italicLyrics);
  const [titleScale] = usePersisted(KEYS.titleScale, DEFAULTS.titleScale);
  const [upcomingBrightness] = usePersisted(KEYS.upcomingBrightness, DEFAULTS.upcomingBrightness);
  const [sungBrightness] = usePersisted(KEYS.sungBrightness, DEFAULTS.sungBrightness);
  const [highlightStrength] = usePersisted(KEYS.highlightStrength, DEFAULTS.highlightStrength);
  const [glowStrength] = usePersisted(KEYS.glowStrength, DEFAULTS.glowStrength);
  const [sweepDirection] = usePersisted(KEYS.sweepDirection, DEFAULTS.sweepDirection);
  const [sweepSoftness] = usePersisted(KEYS.sweepSoftness, DEFAULTS.sweepSoftness);
  const [followSpeed] = usePersisted(KEYS.followSpeed, DEFAULTS.followSpeed);
  const [hideGaps] = usePersisted(KEYS.hideGaps, DEFAULTS.hideGaps);
  const [hideBackingVocals] = usePersisted(KEYS.hideBackingVocals, DEFAULTS.hideBackingVocals);
  const [hideCredits] = usePersisted(KEYS.hideCredits, DEFAULTS.hideCredits);
  const [panelHeight] = usePersisted(KEYS.panelHeight, DEFAULTS.panelHeight);

  // ── Fullscreen ──
  // The widget's window covers its screen (main.cjs setFullscreen) and the
  // widget lays itself out across it: cover and controls on one side, the
  // lyrics on the other, at the screen's height. Swap and hide cover are
  // remembered; hiding the lyrics is the ordinary lyrics switch.
  const [fullscreen, setFullscreenState] = useState(false);
  const [fsLayout, setFsLayout] = usePersisted(KEYS.fsLayout, DEFAULTS.fsLayout);
  const [fsSwap, setFsSwap] = usePersisted(KEYS.fsSwap, DEFAULTS.fsSwap);
  const [fsHideCover, setFsHideCover] = usePersisted(KEYS.fsHideCover, DEFAULTS.fsHideCover);
  const [fsShowLyrics] = usePersisted(KEYS.fsShowLyrics, DEFAULTS.fsShowLyrics);
  const [fsAnimatedBg] = usePersisted(KEYS.fsAnimatedBg, DEFAULTS.fsAnimatedBg);
  const [fsLyricScale] = usePersisted(KEYS.fsLyricScale, DEFAULTS.fsLyricScale);
  const [fsPinControls] = usePersisted(KEYS.fsPinControls, DEFAULTS.fsPinControls);
  const fsDominant = fullscreen && fsLayout === "lyrics";
  const [viewH, setViewH] = useState(() => window.innerHeight);
  useEffect(() => {
    const onResize = () => setViewH(window.innerHeight);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  // Fullscreen lyrics scale with the screen, so a handful of lines show
  // whatever its size (Settings → Fullscreen → Lyrics size).
  const fsLyricSize = Math.round(viewH * 0.045 * (fsLyricScale / 100));
  // the lyrics panel's height: the setting, or the screen's in fullscreen
  const lyricsH = fullscreen ? viewH : panelHeight;
  // where the line being sung sits: just under the top in the widget, a third
  // of the way down fullscreen (higher under the lyrics-dominant header)
  const anchorRef = useRef(LYRIC_ANCHOR);
  // The opening lines sit near the top until the song reaches the anchor;
  // holding them at the anchor too left an empty "forehead" above them.
  const anchorTopRef = useRef(LYRIC_ANCHOR);
  useEffect(() => {
    anchorRef.current = !fullscreen ? LYRIC_ANCHOR : Math.round(viewH * (fsDominant ? 0.2 : 0.32));
    anchorTopRef.current = !fullscreen ? LYRIC_ANCHOR : Math.round(viewH * (fsDominant ? 0.03 : 0.1));
  }, [fullscreen, viewH, fsDominant]);
  const [widgetWidth] = usePersisted(KEYS.widgetWidth, DEFAULTS.widgetWidth);
  const [showCover] = usePersisted(KEYS.showCover, DEFAULTS.showCover);
  const [showArtist] = usePersisted(KEYS.showArtist, DEFAULTS.showArtist);
  const [showTimes] = usePersisted(KEYS.showTimes, DEFAULTS.showTimes);
  const [showProgress] = usePersisted(KEYS.showProgress, DEFAULTS.showProgress);
  const [showShuffleRepeat] = usePersisted(KEYS.showShuffleRepeat, DEFAULTS.showShuffleRepeat);
  const [showVolume] = usePersisted(KEYS.showVolume, DEFAULTS.showVolume);
  const [pinControls] = usePersisted(KEYS.pinControls, DEFAULTS.pinControls);
  const [restOpacity] = usePersisted(KEYS.restOpacity, DEFAULTS.restOpacity);
  const [barThickness] = usePersisted(KEYS.barThickness, DEFAULTS.barThickness);
  const [tintBar] = usePersisted(KEYS.tintBar, DEFAULTS.tintBar);
  const [glassTint] = usePersisted(KEYS.glassTint, DEFAULTS.glassTint);
  const [coverSaturation] = usePersisted(KEYS.coverSaturation, DEFAULTS.coverSaturation);
  const [coverBrightness] = usePersisted(KEYS.coverBrightness, DEFAULTS.coverBrightness);
  const [edgeHighlight] = usePersisted(KEYS.edgeHighlight, DEFAULTS.edgeHighlight);
  const [shadowStrength] = usePersisted(KEYS.shadowStrength, DEFAULTS.shadowStrength);
  const [albumOnLaunch] = usePersisted(KEYS.albumOnLaunch, DEFAULTS.albumOnLaunch);
  const [bubbleOpacity] = usePersisted(KEYS.bubbleOpacity, DEFAULTS.bubbleOpacity);
  const [albumLyricSize] = usePersisted(KEYS.albumLyricSize, DEFAULTS.albumLyricSize);
  const [rainbowSpeed] = usePersisted(KEYS.rainbowSpeed, DEFAULTS.rainbowSpeed);
  const [spinSpeed] = usePersisted(KEYS.spinSpeed, DEFAULTS.spinSpeed);
  const [partyBackground] = usePersisted(KEYS.partyBackground, DEFAULTS.partyBackground);
  const [floatingCover] = usePersisted(KEYS.floatingCover, DEFAULTS.floatingCover);
  // read inside the wheel handler, which is bound once per panel
  const scrollSpeedRef = useRef(scrollSpeed);
  useEffect(() => {
    scrollSpeedRef.current = scrollSpeed;
  }, [scrollSpeed]);
  // Per-language faces: the :lang() rules in App.css read these variables.
  useEffect(() => {
    const root = document.documentElement.style;
    root.setProperty("--cjk-ja", cjkStack("ja", fontJa));
    root.setProperty("--cjk-ko", cjkStack("ko", fontKo));
    root.setProperty("--cjk-zh", cjkStack("zh", fontZh));
  }, [fontJa, fontKo, fontZh]);
  useEffect(() => setLyricMotion(MOTIONS[lyricMotion] ?? 1), [lyricMotion]);
  // Asked once per launch with the setting as it stood then; the main process
  // compares this version with the one it recorded last time.
  const changelogAtLaunch = useRef(showChangelog);
  useEffect(() => {
    window.player?.checkChangelog?.(changelogAtLaunch.current);
  }, []);
  // the glass's own alpha, and a 0–1 scale for the cover blur and shadow that
  // is full from the default upward; see .widget
  const bgStyle = {
    "--bg-a": bgAlpha / 100,
    "--bg-k": Math.min(1, bgAlpha / DEFAULTS.bgAlpha),
    "--widget-radius": `${corners}px`,
    "--cover-blur": `${coverBlur}px`,
    "--cover-radius": COVER_SHAPES[coverShape] ?? COVER_SHAPES.rounded,
    "--lyric-weight": LYRIC_WEIGHTS[lyricWeight] ?? 700,
    "--tint": (GLASS_TINTS[glassTint] ?? GLASS_TINTS.dark).rgb,
    "--cover-sat": coverSaturation / 100,
    "--cover-bright": coverBrightness / 100,
    "--edge-k": edgeHighlight ? 1 : 0,
    "--shadow-k": shadowStrength / 100,
    "--rest-opacity": restOpacity / 100,
    "--widget-w": `${widgetWidth}px`,
    "--panel-h": `${lyricsH}px`,
    "--fs-lyric": `${fsLyricSize}px`,
    "--bar-h": `${barThickness}px`,
    "--bubble-a": bubbleOpacity / 100,
    "--album-lyric-size": `${albumLyricSize}px`,
    "--meta-k": titleScale,
    "--rainbow-s": `${rainbowSpeed}s`,
    "--spin-s": `${spinSpeed}s`,
    // set on the widget so the lyrics, the progress bar and the Rainbow cycle share it
    ...highlightVars(highlight),
  } as React.CSSProperties;

  // The window is sized to the widget; tell it when the width setting moves.
  useEffect(() => {
    window.player?.setWidgetWidth?.(widgetWidth);
  }, [widgetWidth]);

  const [lyricsOpen, setLyricsOpen] = useState(lyricsOnLaunch);
  const [lyricScroll, setLyricScroll] = useState(0);
  const [artMode, setArtMode] = useState(albumOnLaunch);
  const [artCompact, setArtCompact] = useState(false);
  // Lyrics are stored together with the track they belong to, so a track
  // change invalidates them by derivation instead of by a reset effect.
  const [lyricsFor, setLyricsFor] = useState<{
    key: string;
    lines: LyricLine[];
    status: "loading" | "none" | "ok";
    /** false when the timings are fabricated rather than from the source */
    synced?: boolean;
    /** songwriter credits, shown below the lyrics rather than sung as lines */
    credits?: string[];
    /** who to credit for Spicy Lyrics API answers; always on screen with them */
    attribution?: SpicyAttribution | null;
    /** true when the per-word timings were guessed, not supplied by the source */
    wordsEstimated?: boolean;
    source?: string;
    /** finest timing the source carries — see TIER */
    tier?: number;
  }>({ key: "", lines: [], status: "loading" });
  const lyricsRef = useRef<LyricLine[]>([]);
  // Bumped to force the lyrics lookup to run again for the track already
  // playing — a token or a cached payload can arrive long after the track
  // changed, and the effect itself only keys on the track.
  const [reloadNonce, setReloadNonce] = useState(0);
  const [outOfSync, setOutOfSync] = useState(false);
  // bumped per wheel step, so the auto sync countdown restarts while scrolling
  const [wheelNonce, setWheelNonce] = useState(0);
  const [volOpen, setVolOpen] = useState(false);
  const [volDragging, setVolDragging] = useState(false);
  // main.tsx seeds this from the launch default before the first render
  const [romanized, setRomanized] = usePersisted(KEYS.romanized, false);
  const [spotifyAuth, setSpotifyAuth] = useState<{
    configured: boolean;
    signedIn: boolean;
    tokenExpiresAt: number | null;
  } | null>(null);
  // Manual nudge for sources whose timings sit slightly off this master.
  // Positive = show lyrics earlier.
  const [offset, setOffset] = usePersisted(KEYS.lyricOffset, DEFAULTS.lyricOffset);
  const [songOffsets, setSongOffsets] = usePersisted(KEYS.songOffsets, DEFAULTS.songOffsets);
  const [preferredProvider] = usePersisted(KEYS.lyricProvider, DEFAULTS.lyricProvider);
  const [songProviders, setSongProviders] = usePersisted(KEYS.songProviders, DEFAULTS.songProviders);
  const [offsetToast, setOffsetToast] = useState<"global" | "song" | null>(null);
  const offsetRef = useRef(0);

  const [romaLines, setRomaLines] = useState<{ key: string; lines: LyricLine[] }>({
    key: "",
    lines: [],
  });

  // read inside the ResizeObserver, which is created once
  const hoveredRef = useRef(false);

  // capture only while the visualiser is actually on screen
  const { barRefs, live: barsLive } = useAudioBars(artMode && np.playing && showVisualizer);

  // Hide when nothing is playing, after a moment: a track change empties the
  // title for an instant, and the app starts before the first SMTC state.
  const idleHidden = hideWhenIdle && !np.title;
  useEffect(() => {
    if (!idleHidden) {
      window.player?.setWidgetHidden?.(false);
      return;
    }
    const id = setTimeout(() => window.player?.setWidgetHidden?.(true), 1500);
    return () => clearTimeout(id);
  }, [idleHidden]);

  const volume = np.volume ?? 1;
  const hasVolume = np.volume !== null;
  const duration = np.duration > 0 ? np.duration : 0;
  const art = np.art ?? ART_FALLBACK;

  const currentIdxRef = useRef(0);
  const scrubRef = useRef<HTMLInputElement>(null);
  const draggingRef = useRef(false);
  const timeElRef = useRef<HTMLSpanElement>(null);
  const timeRemRef = useRef<HTMLSpanElement>(null);
  const lyricsPanelRef = useRef<HTMLDivElement>(null);
  const lyricsInnerRef = useRef<HTMLDivElement>(null);
  // Serialised set of Active line indices — the animator owns everything below
  // line level, so this is the only thing the clock re-renders React for.
  const activeKeyRef = useRef("");
  const gapHidingRef = useRef(false);
  const durationRef = useRef(0);

  useEffect(() => {
    durationRef.current = duration;
  }, [duration]);

  useEffect(() => {
    hoveredRef.current = hovered;
  }, [hovered]);

  useEffect(() => {
    draggingRef.current = dragging;
  }, [dragging]);

  useEffect(() => {
    window.player?.setAlwaysOnTop?.(alwaysOnTop);
  }, [alwaysOnTop]);

  // This song's own offset rides on top of the global one.
  const thisSong = songKey(np.artist, np.title);
  const songOffset = songOffsets[thisSong] ?? 0;
  const songKeyRef = useRef(thisSong);

  useEffect(() => {
    offsetRef.current = offset + songOffset;
  }, [offset, songOffset]);

  useEffect(() => {
    songKeyRef.current = thisSong;
    // the settings window shows and edits this song's offset
    write(KEYS.track, np.title ? { key: thisSong, title: np.title, artist: np.artist } : null);
  }, [thisSong, np.title, np.artist]);

  // [ and ] nudge the lyric timing by 100 ms; \ resets it. With Shift they
  // nudge this song's own offset instead. Matched on e.code so Shift (which
  // turns [ into { on most layouts) does not change which key it is.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const step =
        e.code === "BracketRight" ? 0.1 : e.code === "BracketLeft" ? -0.1 : e.code === "Backslash" ? 0 : null;
      if (step === null) return;
      e.preventDefault();
      const nudge = (prev: number) =>
        step === 0 ? 0 : Math.max(-5, Math.min(5, Math.round((prev + step) * 10) / 10));
      if (e.shiftKey) {
        const key = songKeyRef.current;
        setSongOffsets((prev) => {
          const next = { ...prev, [key]: nudge(prev[key] ?? 0) };
          if (next[key] === 0) delete next[key];
          return next;
        });
        setOffsetToast("song");
      } else {
        setOffset(nudge);
        setOffsetToast("global");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOffset, setSongOffsets]);

  useEffect(() => {
    if (!offsetToast) return;
    const id = setTimeout(() => setOffsetToast(null), 1400);
    return () => clearTimeout(id);
  }, [offsetToast, offset, songOffset]);

  // The volume pill is rendered outside the art-mode branch, so it has to be
  // dismissed whenever the mode changes or it is left stranded on screen.
  // Fullscreen controls show while the mouse moves and fade after it rests
  // (with the cursor). The pointer never leaves a fullscreen window, so hover
  // alone would keep them up for good.
  const FS_IDLE_MS = 2500;
  const [fsActive, setFsActive] = useState(false);
  const fsIdleTimer = useRef(0);
  const wakeFs = useCallback(() => {
    setFsActive(true);
    clearTimeout(fsIdleTimer.current);
    fsIdleTimer.current = window.setTimeout(() => setFsActive(false), FS_IDLE_MS);
  }, []);
  useEffect(() => {
    if (!fullscreen) return;
    window.addEventListener("mousemove", wakeFs);
    window.addEventListener("pointerdown", wakeFs);
    return () => {
      window.removeEventListener("mousemove", wakeFs);
      window.removeEventListener("pointerdown", wakeFs);
      clearTimeout(fsIdleTimer.current);
    };
  }, [fullscreen, wakeFs]);

  // A fullscreen layout change (swap, cover, lyrics, layout), animated: the
  // browser morphs each named block (App.css, view-transition-name) from where
  // it was to where it goes, and fades what appears or leaves.
  const morph = useCallback(
    (update: () => void) => {
      if (!fullscreen || !document.startViewTransition) return update();
      document.startViewTransition(() => flushSync(update));
    },
    [fullscreen],
  );

  // Into and out of fullscreen. The lyrics show on the way in (and go back to
  // how they were on the way out); album mode has no place there.
  const lyricsBeforeFs = useRef<boolean | null>(null);
  const fullscreenRef = useRef(false);
  const toggleFullscreen = useCallback((on?: boolean) => {
    const next = on ?? !fullscreenRef.current;
    if (next === fullscreenRef.current) return;
    fullscreenRef.current = next;
    if (next) {
      lyricsBeforeFs.current = lyricsOpen;
      setLyricsOpen(fsShowLyrics);
      setArtMode(false);
    } else if (lyricsBeforeFs.current != null) {
      setLyricsOpen(lyricsBeforeFs.current);
      lyricsBeforeFs.current = null;
    }
    setFullscreenState(next);
    if (next) wakeFs();
    window.player?.setFullscreen?.(next);
  }, [lyricsOpen, fsShowLyrics, wakeFs]);
  // Ctrl+Alt+F from anywhere (main.cjs), Esc to leave
  const toggleRef = useRef(toggleFullscreen);
  useEffect(() => {
    toggleRef.current = toggleFullscreen;
  }, [toggleFullscreen]);
  useEffect(() => window.player?.onFullscreenToggle?.(() => toggleRef.current()), []);
  useEffect(() => {
    if (!fullscreen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") toggleRef.current(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen]);

  const setArtModeSafely = useCallback((on: boolean) => {
    setArtMode(on);
    if (!on) setArtCompact(false);
    setVolOpen(false);
  }, []);

  // ── Window dragging ──
  // Anything that owns the pointer itself must not start a drag.
  const startWindowDrag = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    // [data-drag] wins over the exclusions, so a deliberate drag handle can sit
    // inside an otherwise-excluded surface (art mode's strip lives in .art-wrap).
    if (!target.closest("[data-drag]") && target.closest(NO_DRAG_SELECTOR)) return;

    const el = e.currentTarget;
    // The window follows the cursor, so the pointer should stay over the widget
    // for the whole gesture — but a fast flick can outrun it, and without
    // capture the release would then be delivered to whatever is underneath and
    // the window would stay glued to the cursor.
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      /* capture is best-effort */
    }

    window.player?.dragStart();

    const end = () => {
      window.player?.dragEnd();
      el.removeEventListener("pointerup", end);
      el.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
      try {
        el.releasePointerCapture(e.pointerId);
      } catch {
        /* already released */
      }
    };
    el.addEventListener("pointerup", end);
    el.addEventListener("pointercancel", end);
    // Alt-tab or a system dialog mid-drag would otherwise never release it.
    window.addEventListener("blur", end);
  }, []);

  // Paints the slider fill and both time labels. Used by the clock when idle
  // and by the input itself while scrubbing — never by both at once.
  // Called every frame, but the times change once a second and the fill by a
  // fraction of a pixel, and every write costs the page a style or layout pass.
  // So only what differs is written; the fill is kept to 0.01% (~0.03px).
  const paintScrub = useCallback((v: number) => {
    const dur = durationRef.current;
    const el = scrubRef.current;
    if (el) {
      const pos = dur > 0 ? `${((v / dur) * 100).toFixed(2)}%` : "0%";
      if (el.style.getPropertyValue("--pos") !== pos) el.style.setProperty("--pos", pos);
    }
    setText(timeElRef.current, fmt(v));
    if (dur > 0) setText(timeRemRef.current, showTotalRef.current ? fmt(dur) : fmt(dur - v, true));
  }, []);

  // Repaint the moment it is flipped, not on the next tick (none comes when paused).
  useEffect(() => {
    showTotalRef.current = showTotal;
    paintScrub(getPosition());
  }, [showTotal, paintScrub, getPosition]);
  const toggleTotal = () => setShowTotal((t) => !t);

  // Hover drives a layout change, which resizes the window, which can briefly
  // put the pointer outside the element it is hovering — that feedback loop is
  // what made the widget flicker when the cursor sat on its bottom edge. The
  // hover region is the whole window (see .stage) and un-hovering is delayed,
  // so a transient leave is swallowed instead of collapsing the widget.
  const hoverOff = useRef<number | null>(null);

  const enterHover = useCallback(() => {
    if (hoverOff.current !== null) {
      clearTimeout(hoverOff.current);
      hoverOff.current = null;
    }
    setHovered(true);
  }, []);

  const leaveHover = useCallback(() => {
    if (hoverOff.current !== null) clearTimeout(hoverOff.current);
    hoverOff.current = window.setTimeout(() => {
      hoverOff.current = null;
      setHovered(false);
      setVolOpen(false);
    }, 180);
  }, []);

  useEffect(
    () => () => {
      if (hoverOff.current !== null) clearTimeout(hoverOff.current);
    },
    [],
  );

  // A pointer released outside the slider still ends the drag.
  useEffect(() => {
    if (!volDragging) return;
    const end = () => setVolDragging(false);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [volDragging]);

  // Only trust lyrics that were loaded for the track now playing.
  const lyricsStatus = lyricsFor.key === np.trackKey ? lyricsFor.status : "loading";
  const sourceLyrics = lyricsFor.key === np.trackKey ? lyricsFor.lines : EMPTY_LINES;
  const credits =
    lyricsFor.key === np.trackKey ? (lyricsFor.credits ?? EMPTY_CREDITS) : EMPTY_CREDITS;
  const attribution = lyricsFor.key === np.trackKey ? (lyricsFor.attribution ?? null) : null;
  // No real word sync from this source — sweep the line rather than pretend to
  // follow words we only guessed at.
  const lineSweep = lyricsFor.key === np.trackKey && lyricsFor.wordsEstimated === true;

  // Offer romanization only when the lyrics actually contain a non-Latin script.
  const canRomanize = useMemo(
    () => linesNeedRomanization(sourceLyrics),
    [sourceLyrics],
  );

  // Prefer the source's own romanized track (it can handle kanji); fall back to
  // transliterating what we have, which covers kana and hangul.
  const dynamicLyrics = useMemo(() => {
    let base = sourceLyrics;
    if (romanized && canRomanize) {
      // Transliterating in place keeps every timestamp, so word sync survives.
      // The source's own romanized track does not — NetEase ships it as plain
      // line-timed LRC — so it is only worth taking when we genuinely cannot do
      // the job ourselves, i.e. when Han characters are left over. Korean and
      // kana romanize algorithmically, and preferring the source track for them
      // was silently throwing away per-word timing for no gain.
      const inline = romanizeLines(sourceLyrics);
      const sourceHasWordSync = sourceLyrics.some((l) => l.words?.length);
      const inlineIsComplete = !inline.some((l) => hasHanScript(l.text));
      const sourceTrack =
        romaLines.key === np.trackKey && romaLines.lines.length ? romaLines.lines : null;

      if (sourceHasWordSync && inlineIsComplete) {
        base = inline;
      } else if (sourceTrack) {
        // Spicy's own transliteration is already per syllable, so it is taken
        // as-is. NetEase's romalrc is line-timed, and swapping it in wholesale
        // used to discard the source's word timings — lay its text over those
        // windows instead. See alignRomanizedToWords.
        base = sourceTrack.some((l) => l.words?.length)
          ? sourceTrack
          : alignRomanizedToWords(sourceLyrics, sourceTrack);
      } else {
        base = inline;
      }
    }

    // gaps only make sense against real timings
    if (lyricsFor.synced === false) return base;
    // sealEnds first: interlude detection and the Active test both need an end
    // on every line, and only the syllable sources ship one for every line. The
    // bracket pass runs before gaps so a backing vocal is never mistaken for a
    // lead when measuring the space between lines.
    return withGaps(
      markParentheticalBackingVocals(sealEnds(base, lineSweep)),
      lineSweep,
    );
  }, [
    romanized,
    canRomanize,
    sourceLyrics,
    romaLines,
    np.trackKey,
    lyricsFor.synced,
    lineSweep,
  ]);

  useEffect(() => {
    lyricsRef.current = dynamicLyrics;
  }, [dynamicLyrics]);

  // Tag whatever is actually on screen: once romanized, the lines are Latin and
  // must not keep pulling a CJK face.
  const lyricLang = useMemo(() => detectLyricLang(dynamicLyrics), [dynamicLyrics]);

  // ── which provider to ask ──────────────────────────────────
  // This song's pick beats the preferred provider; "auto" is the timing ladder.
  const choice: Provider = songProviders[thisSong] ?? preferredProvider;

  // One lookup per provider per track, shared by the lookup effect and the
  // bottom-bar cycle so switching back and forth never refetches. Spicy is
  // keyed on reloadNonce too: its lyrics can arrive after the first ask.
  type ProviderResult =
    | Awaited<ReturnType<typeof loadSpicyLyrics>>
    | Awaited<ReturnType<typeof loadLyrics>>;
  const providerCache = useRef<{ track: string; map: Map<string, Promise<ProviderResult>> }>({
    track: "",
    map: new Map(),
  });
  const reloadNonceRef = useRef(reloadNonce);
  useEffect(() => {
    reloadNonceRef.current = reloadNonce;
  }, [reloadNonce]);
  const askProvider = useCallback(
    (p: "spicy" | "netease" | "lrcmux" | "lrclib" | "chain"): Promise<ProviderResult> => {
      const cache = providerCache.current;
      if (cache.track !== np.trackKey) {
        cache.track = np.trackKey;
        cache.map = new Map();
      }
      const id = p === "spicy" ? `spicy|${reloadNonceRef.current}` : p;
      let hit = cache.map.get(id);
      if (!hit) {
        const req = { artist: np.artist, title: np.title, album: np.album, duration: np.duration };
        hit = (
          p === "spicy"
            ? loadSpicyLyrics(req)
            : loadLyrics({ ...req, only: p === "chain" ? undefined : p })
        ).catch((e) => {
          console.error(`[lyrics] ${p} failed`, e);
          return null;
        });
        cache.map.set(id, hit);
      }
      return hit;
    },
    [np.trackKey, np.artist, np.title, np.album, np.duration],
  );

  // ── fetch lyrics whenever the track (or the provider choice) changes ──
  useEffect(() => {
    currentIdxRef.current = 0;
    activeKeyRef.current = "";
    gapHidingRef.current = false;

    const key = np.trackKey;
    if (!key || !np.title) return;

    let cancelled = false;

    // Timing quality decides, not provider order:
    //   syllable > word > line > untimed (reported as no lyrics)
    // Spicy is asked first because it is the only syllable source for most
    // catalogue, and a syllable hit ends the search. Anything coarser has to
    // beat what the LrcLib/NetEase chain offers, since an unauthenticated spicy
    // request answers `Static` and a `Line` payload is no better than LrcLib.
    (async () => {
      const usable = (r: ProviderResult) => !!r?.lines.length && r.tier > TIER.none;

      // Not named use* — eslint reads that prefix as a React hook, and this is
      // an ordinary closure called from inside an async callback.
      const applySpicy = (r: NonNullable<Awaited<ReturnType<typeof loadSpicyLyrics>>>) => {
        console.info(`[lyrics] spicy — ${r.lines.length} lines, tier=${r.tier}`);
        setLyricsFor({
          key,
          lines: r.lines,
          status: "ok",
          synced: r.synced,
          credits: r.credits,
          attribution: r.attribution,
          source: "spicy",
          tier: r.tier,
        });
        // Spicy's own transliteration is per syllable, so it keeps the timing
        // the romanize toggle would otherwise lose.
        if (r.romanized) setRomaLines({ key, lines: r.romanized });
      };
      const applyChain = (r: NonNullable<Awaited<ReturnType<typeof loadLyrics>>>) => {
        console.info(
          `[lyrics] ${r.source} — ${r.lines.length} lines, tier=${r.tier}${
            r.wordsEstimated ? ", word timings calculated from the line" : ""
          }`,
        );
        setLyricsFor({
          key,
          lines: r.lines,
          status: "ok",
          synced: r.synced,
          credits: r.credits,
          wordsEstimated: r.wordsEstimated,
          source: r.source,
          tier: r.tier,
        });
      };

      // A picked provider goes first. If it has nothing timed for this song,
      // fall through to the automatic choice rather than show nothing.
      if (choice !== "auto") {
        const picked = await askProvider(choice);
        if (cancelled) return;
        if (usable(picked)) {
          if (choice === "spicy") applySpicy(picked as NonNullable<Awaited<ReturnType<typeof loadSpicyLyrics>>>);
          else applyChain(picked as NonNullable<Awaited<ReturnType<typeof loadLyrics>>>);
          return;
        }
      }

      const spicyRes = (await askProvider("spicy")) as Awaited<ReturnType<typeof loadSpicyLyrics>>;
      if (cancelled) return;
      if (spicyRes?.lines.length && spicyRes.tier >= TIER.syllable) {
        applySpicy(spicyRes);
        return;
      }

      const res = (await askProvider("chain")) as Awaited<ReturnType<typeof loadLyrics>>;
      if (cancelled) return;

      const spicyTier = spicyRes?.lines.length ? spicyRes.tier : TIER.none;
      const chainTier = res?.lines.length ? res.tier : TIER.none;

      // Ties go to spicy: it is asked first, and its lines carry structural
      // backing vocals that the LRC sources only imply with brackets.
      if (spicyRes?.lines.length && spicyTier >= chainTier && spicyTier > TIER.none) {
        applySpicy(spicyRes);
        return;
      }

      if (res?.lines.length) {
        applyChain(res);
        return;
      }

      // Nothing timed anywhere. Untimed words cannot be followed, so the track
      // is reported as having no lyrics rather than shown as static text.
      console.info("[lyrics] none found");
      setLyricsFor({ key, lines: [], status: "none" });
    })();

    return () => {
      cancelled = true;
    };
    // np.trackKey identifies the track; the other fields are read-through.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [np.trackKey, reloadNonce, choice]);

  /**
   * Bottom-bar click: move to the next provider, in PROVIDERS order, that has
   * timed lyrics for this song, and remember it for this song.
   */
  const cycleProvider = useCallback(async () => {
    const current = providerOf(lyricsFor.source);
    const start = PROVIDERS.indexOf(current as (typeof PROVIDERS)[number]);
    for (let i = 1; i <= PROVIDERS.length; i++) {
      const p = PROVIDERS[(start + i + PROVIDERS.length) % PROVIDERS.length];
      if (p === current) break;
      const r = await askProvider(p);
      if (r?.lines.length && r.tier > TIER.none) {
        setSongProviders((prev) => ({ ...prev, [thisSong]: p }));
        return;
      }
    }
    console.info("[lyrics] no other provider has timed lyrics for this song");
  }, [askProvider, lyricsFor.source, setSongProviders, thisSong]);

  // Pull the source's romanized lyrics the first time they are asked for on a
  // given track. Failure is fine — we fall back to transliterating locally.
  useEffect(() => {
    if (!romanized || !canRomanize) return;
    if (romaLines.key === np.trackKey) return;
    if (!np.title || !window.player) return;

    let cancelled = false;
    const key = np.trackKey;
    window.player
      .fetchRomanized({ artist: np.artist, title: np.title })
      .then((lrc) => {
        if (cancelled) return;
        setRomaLines({ key, lines: lrc ? parseLrc(lrc) : [] });
      })
      .catch(() => {
        if (!cancelled) setRomaLines({ key, lines: [] });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [romanized, canRomanize, np.trackKey]);

  useEffect(() => {
    window.player
      ?.spotifyStatus()
      .then(setSpotifyAuth)
      .catch(() => {});
  }, []);

  // The Spicetify extension can hand us a token at any moment — on its first
  // push, or after Spotify refreshes one. Re-run the lookup when that happens
  // so the current track upgrades to syllable timing without a track change.
  useEffect(() => {
    return window.player?.onTokenArrived?.(() => {
      window.player?.spotifyStatus().then(setSpotifyAuth).catch(() => {});
      setReloadNonce((n) => n + 1);
    });
  }, []);

  // The Spicetify extension forwards lyrics Spicy Lyrics had cached, which
  // usually lands a moment after the track change — the track on screen is
  // often already showing a coarser source by then, so look again.
  useEffect(() => {
    return window.player?.onSpicyLyrics?.(() => setReloadNonce((n) => n + 1));
  }, []);

  // A new Spicy Lyrics key may have lyrics the bridge could not get.
  useEffect(() => {
    return window.player?.onSpicyKeyChanged?.(() => setReloadNonce((n) => n + 1));
  }, []);

  // Spotify's next track, from the Spicetify extension. Asked again whenever
  // the bridge reports a new track or a changed queue.
  const [nextFor, setNextFor] = useState<{
    key: string;
    next: { name: string; artist: string; image: string | null } | null;
  }>({ key: "", next: null });
  const [nextNonce, setNextNonce] = useState(0);
  useEffect(() => window.player?.onBridgedTrack?.(() => setNextNonce((n) => n + 1)), []);
  useEffect(() => {
    if (!np.title) return;
    let cancelled = false;
    const key = np.trackKey;
    window.player
      ?.getNext?.({ title: np.title })
      .then((next) => {
        if (!cancelled) setNextFor({ key, next });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // np.title is constant for a given trackKey
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [np.trackKey, nextNonce]);
  const nextUp = nextFor.key === np.trackKey ? nextFor.next : null;

  // The source on screen, for the bottom bar and the settings window.
  const lyricSource =
    lyricsFor.key === np.trackKey && lyricsFor.status === "ok" && lyricsFor.source
      ? { provider: providerOf(lyricsFor.source), tier: lyricsFor.tier ?? TIER.none }
      : null;
  useEffect(
    () => write(KEYS.lyricSource, lyricSource),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [lyricSource?.provider, lyricSource?.tier],
  );

  // The settings window cannot see either of these, so hand them over.
  useEffect(() => write(KEYS.canRomanize, canRomanize), [canRomanize]);
  // Only a real cover: on a song change the art is briefly null while the new
  // one loads, and handing that on left the settings window's blur black.
  // Nothing playing clears it, so they go black and white too.
  useEffect(() => {
    if (np.art) write(KEYS.art, np.art);
    else if (!np.title) write(KEYS.art, null);
  }, [np.art, np.title]);

  const signInToSpotify = useCallback(async () => {
    const res = await window.player?.spotifyLogin();
    if (res?.ok) {
      setSpotifyAuth(await window.player!.spotifyStatus());
      // re-run the lyrics lookup now that Spicy Lyrics is reachable
      setReloadNonce((n) => n + 1);
    } else if (res?.error) {
      console.error("[spotify] sign-in failed:", res.error);
    }
  }, []);

  // ── hovered lyric line ──
  // Marked with .hover by hand rather than CSS :hover. The lyrics move by
  // transform, and the browser only re-checks :hover when the pointer moves, so
  // a line kept its hover look after sliding out from under a still pointer.
  // While the pointer is over the panel, the line under it is found again every
  // frame (the clock below calls this). A re-render rewrites className, so a
  // missing class on the same line is put back too.
  const hoverPoint = useRef<{ x: number; y: number } | null>(null);
  const hoveredLine = useRef<HTMLElement | null>(null);
  const updateLineHover = useCallback(() => {
    const pt = hoverPoint.current;
    const under = pt ? document.elementFromPoint(pt.x, pt.y) : null;
    const line = (under?.closest(".lyric-line") as HTMLElement | null) ?? null;
    if (line === hoveredLine.current && (!line || line.classList.contains("hover"))) return;
    hoveredLine.current?.classList.remove("hover");
    hoveredLine.current = line;
    if (!line) return;
    line.classList.add("hover");
    // the outline's cut-out for the time label is sized to the label
    const ts = line.querySelector<HTMLElement>(".lyric-ts");
    if (ts) line.style.setProperty("--ts-w", `${ts.getBoundingClientRect().width}px`);
  }, []);
  const updateLineHoverRef = useRef(updateLineHover);

  // ── smooth clock — DOM-direct for scrubber/times, React only on line change ──
  useEffect(() => {
    let raf = 0;
    let lastFrame = performance.now();
    // One entry per line currently rendered as current. A lead line and its
    // backing vocal overlap in time, so this is genuinely a list.
    let anims: { node: HTMLElement; words: WordUnit[] }[] = [];
    // The lyrics the units were read from. Toggling romanization (or a source
    // upgrading mid-line) keeps the same line node but swaps its letters, so the
    // node check alone left the animator driving stale spans — overlapping text.
    let animLyrics = lyricsRef.current;

    const tick = () => {
      updateLineHoverRef.current();
      const now = performance.now();
      const dt = (now - lastFrame) / 1000;
      lastFrame = now;

      const next = getPosition();

      // While scrubbing the pointer owns the slider — repainting it here is
      // what made seeking fight itself, 60 times a second.
      if (!draggingRef.current) {
        // The value only serves clicks and arrow keys (the fill is --pos), so
        // it is resynced when it has drifted, not rewritten every frame.
        const scrub = scrubRef.current;
        if (scrub && Math.abs(Number(scrub.value) - next) >= 0.1) scrub.value = String(next);
        paintScrub(next);
      }

      // Lyric timing gets the manual nudge; the scrubber and clock above do
      // not, so the displayed position still matches the player exactly.
      const lyricTime = next + offsetRef.current + LYRIC_LEAD;

      const { active, anchor } = resolveActive(lyricsRef.current, lyricTime);

      // Re-render only when the *set* of current lines changes, the line the
      // view holds on changes, or a gap marker crosses into its fade-out.
      // Word-level changes no longer need React at all — the animator owns
      // every visual property per frame. The anchor counts on its own: on a
      // song change the old song's position briefly anchors the new lyrics on
      // their last line, and once the clock resets nothing is being sung either
      // way, so the set alone kept the view at the bottom until the first line.
      const key = active.join(",");
      const hiding = isGapHiding(lyricsRef.current[anchor], lyricTime);
      if (key !== activeKeyRef.current || anchor !== currentIdxRef.current || hiding !== gapHidingRef.current) {
        activeKeyRef.current = key;
        gapHidingRef.current = hiding;
        currentIdxRef.current = anchor;
        setElapsed(lyricTime);
      }

      // Re-read the units whenever the set of current line elements changes,
      // then drive their springs from the same clock the highlight uses.
      // Find them by class rather than through shared refs: React commits refs
      // in tree order, so when the active line moves the outgoing line's detach
      // can null out the ref the incoming line just set — which left the
      // animator stepping the wrong line's units, or none.
      const nodes = lyricsInnerRef.current
        ? Array.from(
            lyricsInnerRef.current.querySelectorAll<HTMLElement>(".lyric-line.current"),
          )
        : [];

      const unchanged =
        animLyrics === lyricsRef.current &&
        nodes.length === anims.length &&
        nodes.every((n, i) => n === anims[i].node);
      if (!unchanged) {
        for (const a of anims) resetLine(a.words);
        anims = nodes.map((node) => ({ node, words: collectLine(node) }));
        animLyrics = lyricsRef.current;
      }
      for (const a of anims) stepLine(a.words, lyricTime, dt);

      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [getPosition, paintScrub]);

  // ── keep the frameless window sized to the widget ──────────
  // A callback ref, not an effect: the widget element is swapped out when the
  // idle placeholder gives way to the real player, and an effect with an empty
  // dep list would keep observing the detached node.
  const resizeObserver = useRef<ResizeObserver | null>(null);
  const widgetRef = useCallback((el: HTMLDivElement | null) => {
    resizeObserver.current?.disconnect();
    resizeObserver.current = null;
    if (!el || !window.player) return;
    const ro = new ResizeObserver(() => {
      window.player?.resizeTo(
        el.getBoundingClientRect().height + STAGE_PAD * 2,
        hoveredRef.current,
      );
    });
    // border-box: the height sent is the border box, and a padding-only change
    // (album mode drops the padding) leaves the content box as it was
    ro.observe(el, { box: "border-box" });
    resizeObserver.current = ro;
  }, []);

  // ── scrubber ──────────────────────────────────────
  // Same shape as the volume control: a native range input owns the
  // interaction, so pointer capture, click-to-position and keyboard all come
  // for free instead of being reimplemented on mousemove.
  const onScrubChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    paintScrub(Number(e.target.value));
  };

  // Commit on release, wherever the pointer happens to be.
  useEffect(() => {
    if (!dragging) return;
    const end = () => {
      setDragging(false);
      const v = Number(scrubRef.current?.value ?? 0);
      seek(v);
      setElapsed(v);
    };
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragging, seek]);

  // ── lyrics scroll (transform-based, fully controlled) ──
  const clamp = useCallback((val: number) => {
    const inner = lyricsInnerRef.current;
    if (!inner) return val;
    // The spacers let the opening and closing lines reach the same position,
    // just below the panel's top edge, and no further: scrolling the spacers
    // themselves into view only shows an empty panel.
    const rows = inner.querySelectorAll<HTMLElement>(".lyric-line, .lyrics-credits, .lyrics-attribution");
    if (!rows.length) return 0;
    const min = rows[0].offsetTop - anchorTopRef.current;
    const max = rows[rows.length - 1].offsetTop - anchorRef.current;
    if (max <= min) return min;
    return Math.max(min, Math.min(max, val));
  }, []);

  // wheel on panel → update lyricScroll, preventDefault stops native scroll entirely.
  // Re-attached whenever the panel element is replaced: it is absent while
  // nothing plays and unmounted in album mode, and a listener left on the old
  // node made the wheel do nothing at all.
  const panelMounted = !artMode && !!np.title;
  useEffect(() => {
    const el = lyricsPanelRef.current;
    if (!el) return;
    // Each wheel step glides rather than jumps: .wheel gives the transform a
    // short ease-out, and a step arriving mid-glide retargets from wherever the
    // glide has got to, so a flick reads as one motion. Steps are summed per
    // frame, so a touchpad's stream of small deltas costs one render a frame.
    let pending = 0;
    let raf = 0;
    let idle = 0;
    let gliding: HTMLElement | null = null;
    const wheelMax = () => {
      const rows = lyricsInnerRef.current?.querySelectorAll<HTMLElement>(".lyric-line, .lyrics-credits, .lyrics-attribution");
      const last = rows?.[rows.length - 1];
      return last ? last.offsetTop + last.offsetHeight - lyricsH * 0.6 : 0;
    };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const inner = lyricsInnerRef.current;
      if (inner) {
        gliding = inner;
        inner.classList.add("wheel");
        // dropped once the wheel rests, so following the song keeps its own ease
        clearTimeout(idle);
        idle = window.setTimeout(() => inner.classList.remove("wheel"), 250);
      }
      // line and page modes (some mice, some settings) report in other units
      pending +=
        (e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * lyricsH : e.deltaY) *
        scrollSpeedRef.current;
      if (!raf) {
        raf = requestAnimationFrame(() => {
          raf = 0;
          const d = pending;
          pending = 0;
          setLyricScroll((prev) => {
            const next = clamp(prev + d);
            // The wheel stops once the end of the lyrics reaches the middle of
            // the panel; scrolling on past that only showed an empty panel.
            // Following the song still carries the last line to the top, so
            // never pull back from a position it already reached.
            return d > 0 ? Math.min(next, Math.max(prev, wheelMax())) : next;
          });
          setWheelNonce((n) => n + 1);
        });
      }
      setOutOfSync(true);
    };
    const onScroll = () => {
      if (el.scrollTop !== 0) {
        el.scrollTop = 0;
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    el.addEventListener("scroll", onScroll);
    return () => {
      el.removeEventListener("wheel", onWheel);
      el.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(raf);
      clearTimeout(idle);
      gliding?.classList.remove("wheel");
    };
  }, [clamp, lyricsOpen, panelMounted, lyricsH]);

  const scrollToLine = useCallback(
    (el: HTMLElement | null) => {
      if (!el) return;
      const target = el.offsetTop - anchorRef.current;
      setLyricScroll(clamp(target));
    },
    [clamp],
  );

  // Every line Active right now (lead + its backing vocal), plus the one the
  // scroller follows. Matches TimeSetter: Active when startTime <= t < endTime.
  const { active: activeIdxs, anchor: currentIdx } = useMemo(
    () => resolveActive(dynamicLyrics, elapsed),
    [dynamicLyrics, elapsed],
  );
  const activeSet = useMemo(() => new Set(activeIdxs), [activeIdxs]);

  /**
   * The song's opening lyric, shown at full strength until it is sung.
   *
   * A song that opens on an instrumental has nothing on screen but a dots
   * marker and a list dimmed to unsung opacity, so there is nothing to read
   * while waiting. Lifting just the first line gives the opening words to read
   * along to, and it settles back to the ordinary states the moment it starts.
   */
  const firstLyricIdx = useMemo(
    () => dynamicLyrics.findIndex((l) => !l.gap && !l.bg),
    [dynamicLyrics],
  );

  // A backing vocal shares its lead's depth, so the pair defocuses together
  // rather than the bg line reading as one step further away.
  const leadOf = useMemo(() => {
    const map = new Array<number>(dynamicLyrics.length);
    let lead = 0;
    for (let i = 0; i < dynamicLyrics.length; i++) {
      if (!dynamicLyrics[i].bg) lead = i;
      map[i] = lead;
    }
    return map;
  }, [dynamicLyrics]);

  // Unsynced sources (plain text) have no real timings — we spread them evenly
  // just to order them. Following that with a highlight makes the lyrics look
  // badly out of time, so leave them static instead of faking it.
  const isSynced = lyricsFor.synced !== false;



  const syncToCurrentLine = useCallback(() => {
    const lines = lyricsInnerRef.current?.querySelectorAll<HTMLElement>(".lyric-line");
    const index = currentIdx >= 0 ? currentIdx : firstLyricIdx;
    scrollToLine(lines?.[index] ?? null);
    setOutOfSync(false);
  }, [currentIdx, firstLyricIdx, scrollToLine]);

  // The sync and the scrolled-away state, read through refs so each effect
  // below runs on its own triggers rather than on every line change.
  const syncRef = useRef(syncToCurrentLine);
  useEffect(() => {
    syncRef.current = syncToCurrentLine;
  }, [syncToCurrentLine]);
  const outOfSyncRef = useRef(outOfSync);
  useEffect(() => {
    outOfSyncRef.current = outOfSync;
  }, [outOfSync]);

  // Reposition after loading/replacing lyrics and reopening the panel too, even
  // when scrolled away: the lyrics that were scrolled are gone. Resolve from the
  // committed DOM instead of sharing a ref across changing rows.
  useEffect(() => {
    if (!isSynced || !lyricsOpen || artMode) return;
    syncRef.current();
  }, [isSynced, lyricsOpen, artMode, dynamicLyrics, lyricSize, lyricFont, fullscreen, viewH, fsLayout, fsLyricScale]);

  // Follow the song line by line, but not while the user has scrolled away:
  // the view stays where they put it until Auto sync or the Sync button. It
  // used to follow every line change regardless, which snapped a scroll back
  // as soon as the next line started.
  useEffect(() => {
    if (!isSynced || !lyricsOpen || artMode || outOfSyncRef.current) return;
    syncRef.current();
  }, [currentIdx, firstLyricIdx, isSynced, lyricsOpen, artMode]);

  // After a wheel scroll, follow the song again once the wheel has rested for
  // the chosen delay (Settings → Auto sync; 0 = never). The sync is read
  // through a ref so a line change mid-countdown does not restart it. Paused,
  // the view stays put; on resume it follows the song again straight away.
  useEffect(() => {
    if (!outOfSync || !autoSync || !isSynced || !np.playing) return;
    const id = setTimeout(() => {
      syncRef.current();
    }, autoSync * 1000);
    return () => clearTimeout(id);
  }, [outOfSync, wheelNonce, autoSync, isSynced, np.playing]);

  const wasPlayingRef = useRef(np.playing);
  useEffect(() => {
    const resumed = np.playing && !wasPlayingRef.current;
    wasPlayingRef.current = np.playing;
    if (resumed && autoSync && isSynced && outOfSyncRef.current) syncRef.current();
  }, [np.playing, autoSync, isSynced]);

  const seekToLine = (t: number) => {
    if (!clickToJump || !np.caps.seek) return;
    // t is lyric time; the player runs offset behind it (see lyricTime)
    seek(Math.max(0, t - offsetRef.current));
    setElapsed(t);
  };

  const playing = np.playing;
  const platform = appLabel(np.appId);
  const hasTrack = !!np.title;
  // Nothing to show and nothing to do about it, so the panel stays shut and Aa
  // is greyed. lyricsOpen itself is left alone: the next song with lyrics opens
  // the way the user left it.
  const noLyrics = lyricsStatus === "none" && !(spotifyAuth && !spotifyAuth.signedIn);
  const showLyrics = lyricsOpen && !noLyrics;

  if (!hasTrack) {
    // Nothing playing: Playback's own card, the logo's middle line sweeping
    // like a lyric being sung, over the same black and white glass as the
    // installer. While Windows media is still connecting it says so instead.
    return (
      <div className="stage" onMouseEnter={enterHover} onMouseLeave={leaveHover}>
        <div
          ref={widgetRef}
          className={[
            "widget",
            "widget-empty",
            hovered && "hovered",
            !connected && window.player && "connecting",
          ]
            .filter(Boolean)
            .join(" ")}
          style={bgStyle}
          onPointerDown={startWindowDrag}
          onContextMenu={(e) => {
            e.preventDefault();
            window.player?.toggleSettings?.();
          }}
        >
          <div className="widget-bg widget-bg-mono" />
          <div className="np-empty">
            <svg className="np-mark" viewBox="0 0 64 64" aria-hidden="true">
              <rect width="64" height="64" rx="15" fill="#0b0b0d" />
              <rect x=".5" y=".5" width="63" height="63" rx="14.5" fill="none" stroke="#fff" strokeOpacity=".14" />
              <g strokeLinecap="round" stroke="#fff">
                <line x1="17" y1="18" x2="40" y2="18" strokeOpacity=".28" strokeWidth="5" />
                <line x1="13" y1="32" x2="51" y2="32" strokeOpacity=".28" strokeWidth="9" />
                <line className="np-sung" x1="13" y1="32" x2="51" y2="32" strokeWidth="9" pathLength={100} />
                <line x1="17" y1="46" x2="34" y2="46" strokeOpacity=".28" strokeWidth="5" />
              </g>
            </svg>
            <div className="np-empty-text">
              <span className="np-empty-title">
                {!connected && window.player ? "Connecting…" : "Nothing playing"}
              </span>
              <span className="np-empty-sub">Play something in Spotify, your browser or any app.</span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    // hover is tracked on the stage, not the widget: the stage fills the
    // window, so growing the widget can never move the pointer out of it
    <div
      className={`stage${fullscreen ? " fullscreen" : ""}`}
      onMouseEnter={enterHover}
      onMouseLeave={leaveHover}
    >
      <div
        ref={widgetRef}
        className={[
          "widget",
          fullscreen && "fullscreen",
          fullscreen && fsSwap && "fs-swap",
          fullscreen && fsHideCover && "fs-no-cover",
          fsDominant && "fs-dominant",
          // pinned controls behave as if the mouse never rests
          fullscreen && (fsActive || fsPinControls) && "fs-active",
          fullscreen && fsAnimatedBg && "fs-animated",
          hovered && "hovered",
          showLyrics && "lyrics-open",
          artMode && "art-mode",
          dragging && "dragging",
          spinCover && "spin-cover",
          playing && "is-playing",
          allCaps && "caps-lyrics",
          !showCover && "no-cover",
          !showArtist && "no-artist",
          !showTimes && "no-times",
          !showProgress && "no-progress",
          !showShuffleRepeat && "no-shufrep",
          !showVolume && "no-volume",
          pinControls && "controls-pinned",
          tintBar && "tint-bar",
          partyBackground && "party",
          floatingCover && "float-cover",
          highlight === "rainbow" && "rainbow",
        ]
          .filter(Boolean)
          .join(" ")}
        style={bgStyle}
        onPointerDown={startWindowDrag}
        onContextMenu={(e) => {
          e.preventDefault();
          window.player?.toggleSettings?.();
        }}
      >
        {fullscreen && fsAnimatedBg && np.art ? (
          // the cover three times over, blurred and turning at different
          // speeds, so the colour drifts rather than sitting still
          <div className="widget-bg fs-anim-bg">
            {[0, 1, 2].map((i) => (
              <i key={i} style={{ backgroundImage: `url(${art})` }} />
            ))}
          </div>
        ) : np.art ? (
          <div className="widget-bg" style={{ backgroundImage: `url(${art})` }} />
        ) : (
          <div className="widget-bg widget-bg-mono" />
        )}
        <div className="top">
          <div
            className="art-wrap"
            onClick={() => {
              if (fullscreen) return; // no album mode in fullscreen
              if (artMode) setArtCompact((o) => !o);
              else setArtModeSafely(true);
            }}
            style={{ cursor: fullscreen ? "default" : "pointer" }}
          >
            <img
              className="art"
              src={art}
              alt="album art"
              // without this the OS starts an image drag instead of moving
              // the window
              draggable={false}
              onError={(e) => {
                e.currentTarget.src = ART_FALLBACK;
              }}
            />
            {/* In art mode the cover fills the widget, so there is no bare
                surface left to drag the window by. This strip gives one back,
                inset past the back button. */}
            {artMode && (
              <div
                className="art-drag-strip"
                data-drag
                // .art-wrap toggles compact mode on click; the strip is a drag
                // handle, so its clicks must not reach it.
                onClick={(e) => e.stopPropagation()}
              />
            )}
            {artMode && (
              <button
                className="art-overlay-btn art-btn-back"
                onClick={(e) => {
                  e.stopPropagation();
                  setArtModeSafely(false);
                }}
                aria-label="Back"
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <polyline points="15 18 9 12 15 6" />
                </svg>
              </button>
            )}
            {/* Only while a sung line is current: an instrumental break's
                marker is not something to read, and before the first line
                there is nothing to show, so the pill stays away for both. */}
            {artMode && showLyrics && dynamicLyrics[currentIdx] && !dynamicLyrics[currentIdx].gap && (
              <div
                className="art-lyric-overlay"
                lang={lyricLang}
                style={{ bottom: hovered ? 110 : artCompact ? 80 : 10 }}
              >
                {/* Album mode shows the line as a whole — no per-word
                    highlight, no letter wave. Keyed so each new line
                    remounts and fades in. */}
                <span className="art-lyric-text" key={currentIdx}>
                  {dynamicLyrics[currentIdx].text}
                </span>
              </div>
            )}
            {artMode && (
              <div
                className={`art-mini-bar${artCompact || hovered ? " visible" : ""}`}
                onClick={(e) => e.stopPropagation()}
              >
                {/* compact: title + artist + waveform + scrubber — click-toggled */}
                <div
                  className={`art-mini-compact-section${artCompact || hovered ? " visible" : ""}`}
                >
                  <div className="art-mini-row">
                    <div className="art-mini-info">
                      <PanText className="art-mini-title" text={np.title} hovered={hovered} speed={panSpeed} />
                      <PanText className="art-mini-artist" text={np.artist} hovered={hovered} speed={panSpeed} />
                    </div>
                    <div
                      className={[
                        "art-mini-bars",
                        !showVisualizer && "off",
                        playing && "playing",
                        barsLive && "live",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                    >
                      {Array.from({ length: BAR_COUNT }, (_, i) => (
                        <span
                          key={i}
                          ref={(el) => {
                            barRefs.current[i] = el;
                          }}
                        />
                      ))}
                    </div>
                  </div>
                  <div className="art-mini-progress">
                    <span className="art-mini-time" ref={timeElRef}>
                      {fmt(elapsed)}
                    </span>
                    <input
                      type="range"
                      className="scrub scrub-mini"
                      ref={scrubRef}
                      min={0}
                      max={duration || 1}
                      step={0.05}
                      defaultValue={0}
                      disabled={!np.caps.seek || !duration}
                      onPointerDown={(e) => {
                        e.stopPropagation();
                        setDragging(true);
                      }}
                      onChange={onScrubChange}
                      aria-label="Seek"
                      style={
                        {
                          "--pos": duration ? `${(elapsed / duration) * 100}%` : "0%",
                        } as React.CSSProperties
                      }
                    />
                    <span
                      className="art-mini-time art-mini-rem time-flip"
                      ref={timeRemRef}
                      data-nodrag
                      title={showTotal ? "Song length · click for time left" : "Time left · click for song length"}
                      onClick={toggleTotal}
                    >
                      {showTotal ? fmt(duration) : fmt(duration - elapsed, true)}
                    </span>
                  </div>
                </div>
                {/* controls: always on hover */}
                <div
                  className={`art-mini-controls${hovered ? " visible" : ""}`}
                  onClick={(e) => e.stopPropagation()}
                >
                  <div className={`art-vol-btn-wrap${volOpen ? " vol-open" : ""}`}>
                    <span className="ctrl-vol-pct">{Math.round(volume * 100)}%</span>
                    <button
                      className="art-mini-btn"
                      aria-label="Volume"
                      disabled={!hasVolume}
                      onClick={(e) => {
                        e.stopPropagation();
                        setVolOpen((o) => !o);
                      }}
                    >
                      <IconVolume level={volume} />
                    </button>
                  </div>
                  <div className="art-mini-spacer" />
                  <div className="art-mini-transport">
                    <button
                      className={`art-mini-btn${np.shuffle ? " active" : ""}`}
                      aria-label="Shuffle"
                      disabled={!np.caps.shuffle}
                      onClick={() => window.player?.shuffle()}
                    >
                      <IconShuffle />
                    </button>
                    <button
                      className="art-mini-btn"
                      aria-label="Previous"
                      disabled={!np.caps.prev}
                      onClick={() => window.player?.prev()}
                    >
                      <IconPrev />
                    </button>
                    <button
                      className="art-mini-btn art-mini-play"
                      aria-label={playing ? "Pause" : "Play"}
                      onClick={togglePlay}
                    >
                      {playing ? <IconPause /> : <IconPlay />}
                    </button>
                    <button
                      className="art-mini-btn"
                      aria-label="Next"
                      disabled={!np.caps.next}
                      onClick={() => window.player?.next()}
                    >
                      <IconNext />
                    </button>
                    <button
                      className={`art-mini-btn${np.repeat !== "None" ? " active" : ""}`}
                      aria-label="Repeat"
                      disabled={!np.caps.repeat}
                      onClick={() => window.player?.repeat()}
                    >
                      <IconRepeat />
                    </button>
                  </div>
                  <div className="art-mini-spacer" />
                  <button
                    className="art-mini-btn"
                    aria-label="Lyrics"
                    disabled={noLyrics}
                    onClick={(e) => {
                      e.stopPropagation();
                      setLyricsOpen((o) => !o);
                    }}
                  >
                    <IconLyrics />
                  </button>
                </div>
              </div>
            )}
          </div>
          {!artMode && (
            <div className="info">
              <PanText className="title" text={np.title} hovered={hovered} speed={panSpeed} />
              <PanText className="artist" text={np.artist} hovered={hovered} speed={panSpeed} />
            </div>
          )}
          {!artMode && showNext && nextUp && (
            <button
              className="next-up"
              title={`Next: ${nextUp.name} — ${nextUp.artist} (click to skip)`}
              disabled={!np.caps.next}
              onClick={() => window.player?.next()}
            >
              <span className="next-up-text">
                <span className="next-up-label">NEXT</span>
                <span className="next-up-name">{nextUp.name}</span>
              </span>
              {nextUp.image && <img className="next-up-art" src={nextUp.image} alt="" draggable={false} />}
            </button>
          )}
        </div>

        {/* lyrics panel */}
        {!artMode && (
          <div
            className="lyrics-panel"
            ref={lyricsPanelRef}
            onPointerMove={(e) => {
              hoverPoint.current = { x: e.clientX, y: e.clientY };
              updateLineHover();
            }}
            onPointerLeave={() => {
              hoverPoint.current = null;
              updateLineHover();
            }}
          >
            {lyricsStatus !== "ok" ? (
              <div className="lyrics-empty">
                {lyricsStatus === "loading" ? (
                  // skeleton rather than a spinner: it occupies the shape the
                  // lyrics will take, so the panel doesn't jump when they land
                  <div className="lyrics-skeleton" aria-label="Searching for lyrics">
                    {[0, 1, 2, 3].map((i) => (
                      <span key={i} style={{ ["--i" as string]: i } as React.CSSProperties} />
                    ))}
                  </div>
                ) : spotifyAuth && !spotifyAuth.signedIn ? (
                  <div className="token-entry">
                    <button
                      className="spotify-signin"
                      onClick={() => window.player?.openSettings?.()}
                    >
                      Paste Spotify token for better lyrics
                    </button>
                    {spotifyAuth.configured && (
                      <button className="token-alt" onClick={signInToSpotify}>
                        or sign in with Spotify
                      </button>
                    )}
                  </div>
                ) : null}
              </div>
            ) : (
              <div
                className={[
                  "lyrics-inner",
                  lyricAlign === "center" && "centered",
                  !showLineTimes && "no-ts",
                  hideGaps && "no-gaps",
                  hideBackingVocals && "no-bg",
                  hideCredits && "no-credits",
                ]
                  .filter(Boolean)
                  .join(" ")}
                lang={lyricLang}
                ref={lyricsInnerRef}
                style={
                  {
                    "--row-gap": `${lineSpacing}px`,
                    "--sweep-angle": SWEEP_ANGLES[sweepDirection] ?? "90deg",
                    "--sweep-soft": `${sweepSoftness}%`,
                    "--follow-ms": `${followSpeed}ms`,
                    "--lyric-lh": lineHeight,
                    // unset at their defaults, so the browser's own "normal" applies
                    ...(letterSpacing ? { "--lyric-tracking": `${letterSpacing}em` } : {}),
                    ...(italicLyrics ? { "--lyric-style": "italic" } : {}),
                    "--upcoming-k": upcomingBrightness,
                    "--sung-k": sungBrightness,
                    "--lit-k": highlightStrength,
                    "--glow-k": glowStrength,
                    transform: `translateY(${-lyricScroll}px)`,
                    "--lyric-size": `${fullscreen ? fsLyricSize : lyricSize}px`,
                    ...(fontStack(lyricFont) ? { "--latin-stack": fontStack(lyricFont) } : {}),
                  } as React.CSSProperties
                }
              >
                <div style={{ height: SPACER }} />
                {dynamicLyrics.map((line, i) => {
                  const isCurrent = isSynced && activeSet.has(i);
                  // Sung, in spicy's sense: past its own end, not merely above
                  // the playhead. A backing vocal can end before its lead does.
                  const isPast =
                    isSynced && !isCurrent && line.end != null && elapsed >= line.end;
                  // The opening lyric, up until the moment it is sung.
                  const isIntroPreview =
                    isSynced && i === firstLyricIdx && !isCurrent && elapsed < line.t;
                  // depth cue — bloom grows with distance from the playhead.
                  // The intro preview is exempt: defocusing the one line that is
                  // meant to be readable would undo the point of lifting it.
                  const blur =
                    blurLines && isSynced && currentIdx >= 0 && !isIntroPreview
                      ? Math.min(BLUR_MAX, Math.abs(leadOf[i] - leadOf[currentIdx]) * BLUR_STEP)
                      : 0;
                  return (
                    <div
                      key={i}
                      className={[
                        "lyric-line",
                        line.gap && "lyric-gap",
                        line.bg && "bg-line",
                        line.opposite && "opposite",
                        // clear the dots just before the next line arrives
                        isCurrent && isGapHiding(line, elapsed) && "gap-hiding",
                        isCurrent && "current",
                        isPast && "past",
                        isIntroPreview && "intro-preview",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      style={
                        { "--BlurAmount": `${blur.toFixed(2)}px` } as React.CSSProperties
                      }
                      onClick={() => seekToLine(line.t)}
                    >
                      {/* Only current lines need per-syllable spans; rendering
                          them for every line re-flowed the whole list on each
                          change. Non-current lines are styled whole, exactly as
                          spicy does with its .Sung / .NotSung line rules.

                          Gap markers are the exception: their dotGroup is
                          always rendered, because it is scaled between 0 and 1
                          to animate in and out (see .lyric-gap .dotGroup). It
                          has to exist before the line goes current for that
                          transition to have somewhere to start — printing the
                          text instead left every interlude in the song showing
                          a permanent "• • •". */}
                      {isCurrent || line.gap ? (
                        <Units line={line} sweep={lineSweep} animation={animationStyle} />
                      ) : (
                        line.text
                      )}
                      <span className="lyric-ts">• {fmt(line.t)}</span>
                    </div>
                  );
                })}
                {/* Credits belong after the song, not inside it — matching
                    spicy's .Credits element. They carry no timing, are never
                    animated, and never become the current line. */}
                {!!credits.length && (
                  <div className="lyrics-credits">
                    {credits.map((c, i) => (
                      <span key={i}>{c}</span>
                    ))}
                  </div>
                )}
                {/* Required by the Spicy Lyrics API's terms wherever its
                    lyrics show, so "hide credits" does not hide it. */}
                {attribution && <LyricsAttribution credit={attribution} />}
                {/* Enough trailing room to align even a one-line song near the top. */}
                <div style={{ height: lyricsH }} />
              </div>
            )}
          </div>
        )}

        {!artMode && showLyrics && outOfSync && (
          <button
            className="sync-btn"
            onClick={syncToCurrentLine}
            aria-label="Sync"
          >
            Sync
          </button>
        )}

        {!artMode && (
          <>
            <div className="scrubber">
              <input
                type="range"
                className="scrub"
                ref={scrubRef}
                min={0}
                max={duration || 1}
                step={0.05}
                defaultValue={0}
                disabled={!np.caps.seek || !duration}
                onPointerDown={() => setDragging(true)}
                onChange={onScrubChange}
                aria-label="Seek"
                style={
                  {
                    "--pos": duration ? `${(elapsed / duration) * 100}%` : "0%",
                  } as React.CSSProperties
                }
              />
            </div>

            <div className="times">
              <span className="time-el" ref={timeElRef}>
                {fmt(elapsed)}
              </span>
              <span className={`platform${showSource ? "" : " hidden"}`}>
                {platform}
                {lyricSource && (
                  <button
                    className="lyric-source"
                    title={`Lyrics from ${lyricSource.provider} — click to try another provider for this song`}
                    onClick={cycleProvider}
                  >
                    · {lyricSource.provider}
                    <span className="tier-dots">{tierDots(lyricSource.tier)}</span>
                  </button>
                )}
              </span>
              <span
                className="time-rem time-flip"
                ref={timeRemRef}
                data-nodrag
                title={showTotal ? "Song length · click for time left" : "Time left · click for song length"}
                onClick={toggleTotal}
              >
                {showTotal ? fmt(duration) : fmt(duration - elapsed, true)}
              </span>
            </div>

            <div className="controls">
              <div
                className={`ctrl-vol-btn-wrap ctrl-edge${volOpen ? " vol-open" : ""}`}
              >
                <span className="ctrl-vol-pct">{Math.round(volume * 100)}%</span>
                <button
                  className="ctrl-btn"
                  aria-label="Volume"
                  disabled={!hasVolume}
                  onClick={() => setVolOpen((o) => !o)}
                >
                  <IconVolume level={volume} />
                </button>
                {!fullscreen && (
                  <button
                    className="ctrl-btn ctrl-fs"
                    aria-label="Fullscreen"
                    title="Fullscreen (Ctrl+Alt+F)"
                    onClick={() => toggleFullscreen(true)}
                  >
                    <IconExpand />
                  </button>
                )}
              </div>
              <div className="ctrl-center">
                <button
                  className={`ctrl-btn${np.shuffle ? " active" : ""}`}
                  aria-label="Shuffle"
                  disabled={!np.caps.shuffle}
                  onClick={() => window.player?.shuffle()}
                >
                  <IconShuffle />
                </button>
                <button
                  className="ctrl-btn"
                  aria-label="Previous"
                  disabled={!np.caps.prev}
                  onClick={() => window.player?.prev()}
                >
                  <IconPrev />
                </button>
                <button
                  className="ctrl-btn ctrl-play"
                  aria-label={playing ? "Pause" : "Play"}
                  onClick={togglePlay}
                >
                  {playing ? <IconPause /> : <IconPlay />}
                </button>
                <button
                  className="ctrl-btn"
                  aria-label="Next"
                  disabled={!np.caps.next}
                  onClick={() => window.player?.next()}
                >
                  <IconNext />
                </button>
                <button
                  className={`ctrl-btn${np.repeat !== "None" ? " active" : ""}`}
                  aria-label="Repeat"
                  disabled={!np.caps.repeat}
                  onClick={() => window.player?.repeat()}
                >
                  <IconRepeat />
                </button>
              </div>
              <div className="ctrl-right">
                {/* only when the lyrics have a non-Latin script to romanize;
                    the same setting as the Romanize toggle in Settings */}
                {canRomanize && (
                  <button
                    className={`ctrl-btn ctrl-aa${romanized ? " active" : ""}`}
                    aria-label="Romanize"
                    title="Romanize"
                    onClick={() => setRomanized((r) => !r)}
                  >
                    あ
                  </button>
                )}
                <button
                  className={`ctrl-btn ctrl-aa${showLyrics ? " active" : ""}`}
                  aria-label="Lyrics"
                  title="Lyrics"
                  disabled={noLyrics}
                  // animated in fullscreen, like the button at the top right
                  onClick={() => morph(() => setLyricsOpen((o) => !o))}
                >
                  Aa
                </button>
              </div>
            </div>
          </>
        )}

        {fullscreen && (
          <div className="fs-bar" data-nodrag>
            <button
              className={`ctrl-btn${fsDominant ? " active" : ""}`}
              aria-label={fsDominant ? "Split layout" : "Lyrics dominant"}
              title={fsDominant ? "Split layout" : "Lyrics dominant"}
              onClick={() => morph(() => setFsLayout((v) => (v === "lyrics" ? "split" : "lyrics")))}
            >
              <IconDominant />
            </button>
            <button
              className={`ctrl-btn${fsSwap ? " active" : ""}`}
              aria-label="Swap sides"
              title="Swap sides"
              onClick={() => morph(() => setFsSwap((v) => !v))}
            >
              <IconSwap />
            </button>
            <button
              className={`ctrl-btn${showLyrics ? " active" : ""}`}
              aria-label={showLyrics ? "Hide lyrics" : "Show lyrics"}
              title={showLyrics ? "Hide lyrics" : "Show lyrics"}
              disabled={noLyrics}
              onClick={() => morph(() => setLyricsOpen((o) => !o))}
            >
              <IconLyrics />
            </button>
            <button
              className={`ctrl-btn${fsHideCover ? "" : " active"}`}
              aria-label={fsHideCover ? "Show cover" : "Hide cover"}
              title={fsHideCover ? "Show cover" : "Hide cover"}
              onClick={() => morph(() => setFsHideCover((v) => !v))}
            >
              <IconCover />
            </button>
            <button
              className="ctrl-btn"
              aria-label="Exit fullscreen"
              title="Exit fullscreen (Esc)"
              onClick={() => toggleFullscreen(false)}
            >
              <IconCollapse />
            </button>
          </div>
        )}

        {offsetToast && (
          <div className="offset-toast">
            {(() => {
              const v = offsetToast === "song" ? songOffset : offset;
              const label = offsetToast === "song" ? "This song" : "Lyrics";
              return `${label} ${v === 0 ? "in sync" : `${v > 0 ? "+" : ""}${v.toFixed(1)}s`}`;
            })()}
          </div>
        )}

        {volOpen && (
          <div className={`vol-pill${volDragging ? " vol-dragging" : ""}`}>
            <div className="vol-pill-bg" style={{ backgroundImage: `url(${art})` }} />
            <IconVolume level={volume} />
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={volume}
              onChange={(e) => setVolume(Number(e.target.value))}
              onPointerDown={() => setVolDragging(true)}
              className="vol-slider"
              aria-label="Volume"
              // the filled portion is drawn on the track pseudo-element, which
              // lets the input itself be tall enough to actually grab
              style={{ "--vol": `${volume * 100}%` } as React.CSSProperties}
            />
            <span className="vol-pct">{Math.round(volume * 100)}</span>
          </div>
        )}
      </div>
    </div>
  );
}
