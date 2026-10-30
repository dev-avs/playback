import { SLObjPack } from "./vendor/objpack";
import { TIER, type LyricLine, type LyricWord, type Tier } from "./lyrics";

// Shapes as used by spicy-lyrics' own applyers (Synced/Syllable.ts,
// Synced/Line.ts). Times are already in seconds — their ConvertTime()
// multiplies by 1000 for the animator, so the wire format matches our own unit.
interface SpicySyllable {
  Text: string;
  TransliteratedText?: string;
  StartTime: number;
  EndTime: number;
  IsPartOfWord?: boolean;
}

interface SpicyVocalGroup {
  StartTime?: number;
  EndTime?: number;
  Syllables?: SpicySyllable[];
}

interface SpicySyllableLine {
  Lead?: SpicyVocalGroup;
  /**
   * Backup vocals for this line. Their windows normally sit *inside* the lead's,
   * so the renderer has to allow more than one line to be current at a time.
   */
  Background?: SpicyVocalGroup[];
  /** duet flag — spicy right-aligns these lines (`OppositeAligned`) */
  OppositeAligned?: boolean;
}

interface SpicyTextLine {
  Text?: string;
  TransliteratedText?: string;
  StartTime?: number;
  EndTime?: number;
  OppositeAligned?: boolean;
}

interface SpicyLyrics {
  Type?: "Syllable" | "Line" | "Static" | string;
  Content?: unknown[];
  StartTime?: number;
  /** structured credits — never part of the lyric content */
  SongWriters?: string[];
  /** which catalogue answered; only the public API (v1) sends it */
  source?: "spicy_lyrics" | "apple_music" | "spotify" | "unknown" | string;
  /** present only on community syncs */
  UploadAttribution?: { Uploader?: SpicyContributor; Maker?: SpicyContributor };
}

interface SpicyContributor {
  username?: string;
  url?: string;
}

/** Who to credit for these lyrics, as the API's terms require on screen. */
export interface SpicyAttribution {
  provider: string;
  uploader?: { name: string; url?: string };
  maker?: { name: string; url?: string };
}

const PROVIDER_NAMES: Record<string, string> = {
  spicy_lyrics: "Spicy Lyrics",
  apple_music: "Apple Music",
  spotify: "Spotify",
};

function attributionOf(data: SpicyLyrics): SpicyAttribution | null {
  if (!data.source) return null;
  const person = (c?: SpicyContributor) => (c?.username ? { name: c.username, url: c.url } : undefined);
  const credit = data.source === "spicy_lyrics" ? data.UploadAttribution : undefined;
  return {
    provider: PROVIDER_NAMES[data.source] ?? "Spicy Lyrics",
    uploader: person(credit?.Uploader),
    maker: person(credit?.Maker),
  };
}

export interface SpicyResult {
  lines: LyricLine[];
  /** transliterated variant, present only when the payload carries one */
  romanized: LyricLine[] | null;
  /** display-ready credit lines, rendered apart from the lyrics */
  credits: string[];
  synced: boolean;
  /** present for answers from the public API, which must be credited on screen */
  attribution: SpicyAttribution | null;
  /**
   * Finest timing this payload carries. Spicy is asked first because it is the
   * only source of syllable timing for most catalogue, but it does not win by
   * default: an unauthenticated request answers with `Static`, and a `Line`
   * payload is no better than LrcLib. The caller ranks it like any other.
   */
  tier: Tier;
}

const packer = new SLObjPack();

/**
 * Spicy-lyrics keeps each syllable — "won", "der", "ful" — as its own timed
 * unit and only groups them visually. Merging them back into whole words drops
 * the real per-syllable onsets, so the letter wave has to guess timing across
 * the entire fused word and drifts on anything held or multi-syllabic.
 *
 * We keep the same shape here: one LyricWord per syllable, carrying its real
 * StartTime/EndTime. IsPartOfWord travels through as `joinNext` so the renderer
 * can suppress the separating space and keep the surface text unchanged.
 */
function syllablesToWords(
  syllables: SpicySyllable[],
  transliterate: boolean,
): LyricWord[] {
  const words: LyricWord[] = [];
  for (let i = 0; i < syllables.length; i++) {
    const s = syllables[i];
    const text = ((transliterate ? (s.TransliteratedText ?? s.Text) : s.Text) ?? "").trim();
    if (!text) continue;
    words.push({
      t: s.StartTime,
      end: s.EndTime,
      text,
      joinNext: s.IsPartOfWord === true,
    });
  }
  return words;
}

/**
 * Rebuild the display text so joinNext runs of syllables print as one token —
 * otherwise the words[] array leaks syllable breaks into the fallback renderer
 * and into search/copy.
 */
function joinSyllableText(words: LyricWord[]): string {
  let text = "";
  for (let i = 0; i < words.length; i++) {
    text += words[i].text;
    if (i < words.length - 1 && !words[i].joinNext) text += " ";
  }
  return text;
}

function vocalGroupToLine(
  group: SpicyVocalGroup,
  transliterate: boolean,
  extra: Partial<LyricLine>,
): LyricLine | null {
  const syllables = group?.Syllables ?? [];
  if (!syllables.length) return null;
  const words = syllablesToWords(syllables, transliterate);
  if (!words.length) return null;
  return {
    t: group.StartTime ?? words[0].t,
    // Use the real EndTime from the spicy-lyrics payload (seconds).
    end: group.EndTime ?? words[words.length - 1].end,
    text: joinSyllableText(words),
    words,
    ...extra,
  };
}

function convert(data: SpicyLyrics, transliterate: boolean): LyricLine[] {
  const content = Array.isArray(data.Content) ? data.Content : [];
  const lines: LyricLine[] = [];

  if (data.Type === "Syllable") {
    for (const raw of content as SpicySyllableLine[]) {
      const opposite = raw?.OppositeAligned === true;
      const lead = raw?.Lead
        ? vocalGroupToLine(raw.Lead, transliterate, { opposite })
        : null;
      if (!lead) continue;
      lines.push(lead);

      // Backup vocals become their own lines, inserted directly after the lead
      // they belong to. The renderer relies on that adjacency to know which
      // lead a bg line hangs off (for depth blur and for the tighter gap).
      for (const bg of raw.Background ?? []) {
        const bgLine = vocalGroupToLine(bg, transliterate, { opposite, bg: true });
        if (bgLine) lines.push(bgLine);
      }
    }
    return lines;
  }

  if (data.Type === "Line") {
    for (const raw of content as SpicyTextLine[]) {
      const text = (transliterate ? (raw.TransliteratedText ?? raw.Text) : raw.Text) ?? "";
      if (!text.trim()) continue;
      // Use the real EndTime from the spicy-lyrics payload (seconds).
      lines.push({
        t: raw.StartTime ?? 0,
        end: raw.EndTime,
        text: text.trim(),
        opposite: raw.OppositeAligned === true,
      });
    }
    return lines;
  }

  // "Static" — unsynced; no timings at all, so leave them at 0 and let the
  // caller decide (we simply never auto-scroll these).
  for (const raw of content as SpicyTextLine[]) {
    const text = (transliterate ? (raw.TransliteratedText ?? raw.Text) : raw.Text) ?? "";
    if (text.trim()) lines.push({ t: 0, end: undefined, text: text.trim() });
  }
  return lines;
}

function hasTransliteration(data: SpicyLyrics): boolean {
  const content = Array.isArray(data.Content) ? data.Content : [];
  return content.some((raw) => {
    const line = raw as SpicySyllableLine & SpicyTextLine;
    if (line.TransliteratedText) return true;
    if ((line.Lead?.Syllables ?? []).some((s) => !!s.TransliteratedText)) return true;
    return (line.Background ?? []).some((bg) =>
      (bg.Syllables ?? []).some((s) => !!s.TransliteratedText),
    );
  });
}

/**
 * Fetch + unpack lyrics for the current track via Spicy Lyrics.
 * Returns null whenever the source can't serve this track, so the caller can
 * fall back to the existing LrcLib/NetEase chain.
 */
export async function loadSpicyLyrics(req: {
  artist: string;
  title: string;
  album?: string;
  duration?: number;
}): Promise<SpicyResult | null> {
  const bridge = window.player;
  if (!bridge?.fetchSpicyLyrics) return null;

  const res = await bridge.fetchSpicyLyrics(req);
  if (res?.status !== "ok") return null;

  let data: SpicyLyrics;
  if (res.lyrics != null) {
    // Came from the Spicy Lyrics client's own cache via the Spicetify bridge.
    // It stores what it already unpacked (see their GetExpireStore), so there
    // is nothing to decode here.
    data = res.lyrics as SpicyLyrics;
  } else if (res.packed != null) {
    try {
      data = packer.unpack(res.packed) as SpicyLyrics;
    } catch (e) {
      console.error("[spicy] unpack failed", e);
      return null;
    }
  } else {
    return null;
  }
  if (!data || !Array.isArray(data.Content) || !data.Content.length) return null;

  const lines = convert(data, false);
  if (!lines.length) return null;

  const romanized = hasTransliteration(data) ? convert(data, true) : null;

  // Matches ApplyLyricsCredits.ts: one "Written by: …" line built from the
  // structured array. The names never appear among the lyric lines here, so
  // there is nothing to strip — unlike the LRC sources.
  const writers = (data.SongWriters ?? []).filter((w) => w?.trim());

  const synced = data.Type === "Syllable" || data.Type === "Line";
  return {
    lines,
    romanized: romanized?.length ? romanized : null,
    credits: writers.length ? [`Written by: ${writers.join(", ")}`] : [],
    synced,
    attribution: attributionOf(data),
    tier:
      data.Type === "Syllable"
        ? TIER.syllable
        : data.Type === "Line"
          ? TIER.line
          : TIER.none,
  };
}
