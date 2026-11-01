import { useCallback, useEffect, useState } from "react";

/** Playback's logo as an image: what shows where a cover would, when there is none. */
export const LOGO_ART =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 64 64"><rect width="64" height="64" fill="#0b0b0d"/><g stroke-linecap="round" stroke="#fff"><line x1="17" y1="18" x2="40" y2="18" stroke-opacity=".28" stroke-width="5"/><line x1="13" y1="32" x2="51" y2="32" stroke-opacity=".28" stroke-width="9"/><line x1="13" y1="32" x2="37" y2="32" stroke-width="9"/><line x1="17" y1="46" x2="34" y2="46" stroke-opacity=".28" stroke-width="5"/></g></svg>`,
  );

/**
 * Settings live in localStorage, which both windows share. A write in one
 * window fires a `storage` event in the other, so the widget follows the
 * settings window live without any IPC of its own.
 */

/**
 * Lyric fonts on offer. Each replaces only the Latin part of the stack, so CJK
 * lines still fall through to their per-language faces. "Default" leaves the
 * stylesheet's own --latin-stack alone.
 */
export const LYRIC_FONTS: Record<string, string | null> = {
  Default: null,
  "Segoe UI": '"Segoe UI Variable Display", "Segoe UI"',
  Bahnschrift: '"Bahnschrift"',
  Georgia: '"Georgia"',
  Mono: '"Cascadia Code", "Consolas"',
};

/**
 * The --latin-stack value for a saved font: a preset's stack, or any installed
 * family picked from the full list. null leaves the stylesheet's default.
 */
export function fontStack(name: string): string | null {
  if (name in LYRIC_FONTS) return LYRIC_FONTS[name];
  return name ? `"${name.replace(/["\\]/g, "")}"` : null;
}

export const DEFAULTS = {
  romanizeDefault: false,
  lyricSize: 17,
  /** px per second a too-long title or artist slides at (PanText) */
  panSpeed: 8,
  /** fullscreen: cover and controls beside the lyrics, or a small header above them */
  fsLayout: "split" as "split" | "lyrics",
  /** fullscreen: cover and controls on the right instead of the left */
  fsSwap: false,
  /** fullscreen: without the big cover */
  fsHideCover: false,
  /** fullscreen opens with the lyrics showing */
  fsShowLyrics: true,
  /** fullscreen: the cover, blurred, drifting slowly behind everything */
  fsAnimatedBg: true,
  /** fullscreen lyric size, % of the screen-height default */
  fsLyricScale: 100,
  /** fullscreen: the controls stay instead of fading at rest */
  fsPinControls: false,
  /** how sung words move: Playback's own, or spicy's (lyricAnimator.ts) */
  animationStyle: "playback" as "playback" | "spicy",
  lyricFont: "Default",
  lyricOffset: 0,
  alwaysOnTop: true,
  blurLines: true,
  lyricsOnLaunch: false,
  /** seconds after a wheel scroll before the lyrics follow the song again; 0 = never */
  autoSync: 3,
  /** right-hand time shows the song's length instead of the time left */
  showTotalTime: false,
  lyricAlign: "left" as LyricAlign,
  /** colour of the sung part of the line; see HIGHLIGHTS */
  highlight: "white" as Highlight,
  /** widget corner radius, px */
  corners: 13,
  /** the "Next" song label, top right */
  showNext: true,
  /** "Spotify · spicy ●●●" under the progress bar */
  showSource: true,
  /** the rounded bars next to the title in album mode */
  showVisualizer: true,
  /** how much the sung letters lift and grow; see MOTIONS */
  lyricMotion: "normal" as LyricMotion,
  /** disappear while nothing is playing */
  hideWhenIdle: false,
  /** open the changelog on the first launch of each version */
  showChangelog: true,
  lyricWeight: "bold" as LyricWeight,
  /** gap between lyric lines, px (--row-gap) */
  lineSpacing: 10,
  /** click a lyric line to jump the song to it */
  clickToJump: true,
  /** the small time badge on a line under the pointer */
  showLineTimes: true,
  /** multiplier on mouse-wheel distance in the lyrics */
  scrollSpeed: 1,
  /** blur of the cover behind the widget, px */
  coverBlur: 50,
  coverShape: "rounded" as CoverShape,
  /** round cover that turns like a record while music plays */
  spinCover: false,
  /** lyrics in capitals */
  allCaps: false,
  /** per-language lyric faces; "default" keeps the built-in choice (CJK_FONTS) */
  fontJa: "default",
  fontKo: "default",
  fontZh: "default",
  /** lyric line height, multiple of the size */
  lineHeight: 1.18,
  /** lyric letter spacing, em */
  letterSpacing: 0,
  /** lyrics in italics */
  italicLyrics: false,
  /** title and artist size, multiple of the original */
  titleScale: 1,
  /** lines not yet sung, multiple of their usual opacity */
  upcomingBrightness: 1,
  /** lines already sung, multiple of their usual opacity */
  sungBrightness: 1,
  /** brightness of the lit fill, multiple of the original */
  highlightStrength: 1,
  /** glow around sung letters, multiple of the original */
  glowStrength: 1,
  /** the fill runs along the line or down it */
  sweepDirection: "across" as SweepDirection,
  /** width of the fill's soft edge, percent */
  sweepSoftness: 20,
  /** how long the list takes to glide to the next line, ms */
  followSpeed: 420,
  /** hide the instrumental-break dots */
  hideGaps: false,
  /** hide backing-vocal lines */
  hideBackingVocals: false,
  /** hide songwriter credits at the end */
  hideCredits: false,
  /** lyrics panel height, px */
  panelHeight: 300,
  /** widget width, px */
  widgetWidth: 350,
  /** the small cover next to the title */
  showCover: true,
  /** the artist under the title */
  showArtist: true,
  /** the elapsed and remaining times */
  showTimes: true,
  /** the progress bar */
  showProgress: true,
  /** the shuffle and repeat buttons */
  showShuffleRepeat: true,
  /** the volume button */
  showVolume: true,
  /** keep the controls shown, not just on hover */
  pinControls: false,
  /** whole-widget opacity while the pointer is away, percent */
  restOpacity: 100,
  /** progress bar thickness, px */
  barThickness: 2,
  /** progress bar in the Highlight colour */
  tintBar: false,
  /** colour of the glass */
  glassTint: "dark" as GlassTint,
  /** colour strength of the cover behind the widget, percent */
  coverSaturation: 140,
  /** brightness of the cover behind the widget, percent */
  coverBrightness: 100,
  /** the thin light edge around the widget */
  edgeHighlight: true,
  /** drop shadow, percent of the original */
  shadowStrength: 100,
  /** start in album mode */
  albumOnLaunch: false,
  /** album-mode lyric bubble darkness, percent */
  bubbleOpacity: 48,
  /** album-mode lyric size, px */
  albumLyricSize: 13,
  /** seconds per Rainbow cycle */
  rainbowSpeed: 8,
  /** seconds per spinning-cover turn */
  spinSpeed: 6,
  /** the cover glow cycles through colours */
  partyBackground: false,
  /** the small cover gently bobs */
  floatingCover: false,
  /**
   * Widget background opacity, percent: 0 clear, 72 the default glass, 100
   * solid. The cover blur and shadow fade with it below the default.
   */
  bgAlpha: 72,
  /** extra offset per song, keyed by songKey(); added to lyricOffset */
  songOffsets: {} as Record<string, number>,
  /** "auto" = best timing wins; otherwise that provider first */
  lyricProvider: "auto" as Provider,
  /** per-song provider, keyed by songKey(); beats lyricProvider */
  songProviders: {} as Record<string, Provider>,
};

export type Provider = "auto" | "spicy" | "lrcmux" | "netease" | "lrclib";

export type LyricAlign = "left" | "center";

/**
 * Colours for the sung part of a line, as HSL so one gradient rule serves all
 * of them (see .word in App.css). White is the original look. Rainbow uses its
 * saturation and lightness and lets the hue cycle (.rainbow).
 */
export const HIGHLIGHTS = {
  white: { name: "White", h: 0, s: 0, l: 100 },
  pink: { name: "Pink", h: 330, s: 90, l: 78 },
  gold: { name: "Gold", h: 45, s: 95, l: 65 },
  mint: { name: "Mint", h: 155, s: 70, l: 70 },
  sky: { name: "Sky", h: 200, s: 90, l: 72 },
  rainbow: { name: "Rainbow", h: 0, s: 90, l: 72 },
} as const;
export type Highlight = keyof typeof HIGHLIGHTS;

/** CSS variables for a highlight colour, consumed as --lit-h/s/l. */
export function highlightVars(h: Highlight): Record<string, string | number> {
  const c = HIGHLIGHTS[h] ?? HIGHLIGHTS.white;
  return { "--lit-h": c.h, "--lit-s": `${c.s}%`, "--lit-l": `${c.l}%` };
}

export type SweepDirection = "across" | "down";
/** The fill's travel: along the line as it is sung, or top to bottom (spicy's). */
export const SWEEP_ANGLES: Record<SweepDirection, string> = { across: "90deg", down: "180deg" };
/** Right-to-left lines (Arabic, Hebrew) sweep across the other way. */
export const SWEEP_ANGLES_RTL: Record<SweepDirection, string> = { across: "270deg", down: "180deg" };

/** Glass colours, as the RGB of the widget's tint (alpha is Background). */
export const GLASS_TINTS = {
  dark: { name: "Dark", rgb: "20, 20, 22" },
  black: { name: "Black", rgb: "0, 0, 0" },
  warm: { name: "Warm", rgb: "56, 34, 22" },
  cool: { name: "Cool", rgb: "18, 30, 56" },
} as const;
export type GlassTint = keyof typeof GLASS_TINTS;

export const LYRIC_WEIGHTS = { regular: 400, semibold: 600, bold: 700 } as const;
export type LyricWeight = keyof typeof LYRIC_WEIGHTS;

/** Corner radius of the small cover next to the title. */
export const COVER_SHAPES = { rounded: "6px", square: "2px", circle: "50%" } as const;
export type CoverShape = keyof typeof COVER_SHAPES;

/**
 * Lyric faces per language. `stack` is the built-in choice (it matches the
 * --cjk-* defaults in App.css); a picked font goes in front of it, so a font
 * that is not installed falls back to the built-in one.
 */
export const CJK_FONTS = {
  ja: {
    name: "Japanese font",
    stack: '"Yu Gothic UI", "Meiryo"',
    options: ["Yu Gothic UI", "Meiryo", "MS Gothic", "BIZ UDPGothic", "Noto Sans JP"],
  },
  ko: {
    name: "Korean font",
    stack: '"Noto Sans KR", "Malgun Gothic"',
    options: ["Noto Sans KR", "Malgun Gothic", "Gulim", "Batang"],
  },
  zh: {
    name: "Chinese font",
    stack: '"Microsoft YaHei UI", "Microsoft JhengHei UI", "Microsoft YaHei"',
    options: ["Microsoft YaHei UI", "Microsoft JhengHei UI", "SimHei", "DengXian", "Noto Sans SC"],
  },
} as const;
export type CjkLang = keyof typeof CJK_FONTS;

/** The font stack for a language, with the picked font (if any) first. */
export function cjkStack(lang: CjkLang, font: string): string {
  const base = CJK_FONTS[lang].stack;
  return font === "default" ? base : `"${font.replace(/["\\]/g, "")}", ${base}`;
}

/** Scale on the letters' lift and growth: calm holds them still. */
export const MOTIONS = { calm: 0, normal: 1, bouncy: 1.8 } as const;
export type LyricMotion = keyof typeof MOTIONS;
/** The providers that can be picked, in cycle order. */
export const PROVIDERS: Exclude<Provider, "auto">[] = ["spicy", "lrcmux", "netease", "lrclib"];
export const PROVIDER_NAMES: Record<Provider, string> = {
  auto: "Auto",
  spicy: "Spicy",
  lrcmux: "LRCMux",
  netease: "NetEase",
  lrclib: "LrcLib",
};

/** "netease/yrc" → "netease" */
export function providerOf(source: string | undefined): string {
  return (source ?? "").split("/")[0];
}

/** ●●● syllable, ●●○ word, ●○○ line */
export function tierDots(tier: number): string {
  return "●".repeat(tier) + "○".repeat(Math.max(0, 3 - tier));
}

/** Identifies a song across players and albums for its own offset. */
export function songKey(artist: string, title: string) {
  return `${artist.trim().toLowerCase()}|${title.trim().toLowerCase()}`;
}

export const KEYS = {
  romanizeDefault: "set.romanizeDefault",
  lyricSize: "set.lyricSize",
  panSpeed: "set.panSpeed",
  animationStyle: "set.animationStyle",
  fsLayout: "set.fsLayout",
  fsSwap: "set.fsSwap",
  fsHideCover: "set.fsHideCover",
  fsShowLyrics: "set.fsShowLyrics",
  fsAnimatedBg: "set.fsAnimatedBg",
  fsLyricScale: "set.fsLyricScale",
  fsPinControls: "set.fsPinControls",
  lyricFont: "set.lyricFont",
  // predates the settings window; kept so saved offsets survive
  lyricOffset: "lyricOffset",
  alwaysOnTop: "set.alwaysOnTop",
  blurLines: "set.blurLines",
  lyricsOnLaunch: "set.lyricsOnLaunch",
  autoSync: "set.autoSync",
  showTotalTime: "set.showTotalTime",
  lyricAlign: "set.lyricAlign",
  highlight: "set.highlight",
  corners: "set.corners",
  showNext: "set.showNext",
  showSource: "set.showSource",
  showVisualizer: "set.showVisualizer",
  lyricMotion: "set.lyricMotion",
  hideWhenIdle: "set.hideWhenIdle",
  showChangelog: "set.showChangelog",
  lyricWeight: "set.lyricWeight",
  lineSpacing: "set.lineSpacing",
  clickToJump: "set.clickToJump",
  showLineTimes: "set.showLineTimes",
  scrollSpeed: "set.scrollSpeed",
  coverBlur: "set.coverBlur",
  coverShape: "set.coverShape",
  spinCover: "set.spinCover",
  allCaps: "set.allCaps",
  fontJa: "set.fontJa",
  fontKo: "set.fontKo",
  fontZh: "set.fontZh",
  lineHeight: "set.lineHeight",
  letterSpacing: "set.letterSpacing",
  italicLyrics: "set.italicLyrics",
  titleScale: "set.titleScale",
  upcomingBrightness: "set.upcomingBrightness",
  sungBrightness: "set.sungBrightness",
  highlightStrength: "set.highlightStrength",
  glowStrength: "set.glowStrength",
  sweepDirection: "set.sweepDirection",
  sweepSoftness: "set.sweepSoftness",
  followSpeed: "set.followSpeed",
  hideGaps: "set.hideGaps",
  hideBackingVocals: "set.hideBackingVocals",
  hideCredits: "set.hideCredits",
  panelHeight: "set.panelHeight",
  widgetWidth: "set.widgetWidth",
  showCover: "set.showCover",
  showArtist: "set.showArtist",
  showTimes: "set.showTimes",
  showProgress: "set.showProgress",
  showShuffleRepeat: "set.showShuffleRepeat",
  showVolume: "set.showVolume",
  pinControls: "set.pinControls",
  restOpacity: "set.restOpacity",
  barThickness: "set.barThickness",
  tintBar: "set.tintBar",
  glassTint: "set.glassTint",
  coverSaturation: "set.coverSaturation",
  coverBrightness: "set.coverBrightness",
  edgeHighlight: "set.edgeHighlight",
  shadowStrength: "set.shadowStrength",
  albumOnLaunch: "set.albumOnLaunch",
  bubbleOpacity: "set.bubbleOpacity",
  albumLyricSize: "set.albumLyricSize",
  rainbowSpeed: "set.rainbowSpeed",
  spinSpeed: "set.spinSpeed",
  partyBackground: "set.partyBackground",
  floatingCover: "set.floatingCover",
  // not "set.bgOpacity": that one meant the default look at 100
  bgAlpha: "set.bgAlpha",
  songOffsets: "set.songOffsets",
  lyricProvider: "set.lyricProvider",
  songProviders: "set.songProviders",
  /** written by the widget: { provider, tier } of the lyrics on screen */
  lyricSource: "state.lyricSource",
  /** written by the widget: { key, title, artist } of the song playing */
  track: "state.track",
  /** romanize on/off right now, as opposed to the launch default */
  romanized: "state.romanized",
  /** written by the widget so the settings window can show it */
  canRomanize: "state.canRomanize",
  art: "state.art",
};

function read<T>(key: string, initial: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw == null ? initial : (JSON.parse(raw) as T);
  } catch {
    return initial;
  }
}

export function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* not saved — still works for this session */
  }
}

export type Settings = typeof DEFAULTS;
export type SettingKey = keyof Settings;

const SETTING_KEYS = Object.keys(DEFAULTS) as SettingKey[];
const keyOf = (k: SettingKey) => KEYS[k as keyof typeof KEYS];

/**
 * Every setting in one object, for the settings window: one storage listener
 * instead of one hook per setting, and a setter that takes the setting's name,
 * so the window can be built from a list of definitions. Values are written
 * where usePersisted reads them, so the widget follows along unchanged.
 */
export function useSettingsStore() {
  const [values, setValues] = useState<Settings>(
    () => Object.fromEntries(SETTING_KEYS.map((k) => [k, read(keyOf(k), DEFAULTS[k])])) as Settings,
  );
  useEffect(() => {
    const byStorageKey = new Map(SETTING_KEYS.map((k) => [keyOf(k), k]));
    const onStorage = (e: StorageEvent) => {
      const k = e.key ? byStorageKey.get(e.key) : undefined;
      if (k) setValues((v) => ({ ...v, [k]: read(keyOf(k), DEFAULTS[k]) }));
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
  const set = useCallback(<K extends SettingKey>(k: K, v: Settings[K]) => {
    write(keyOf(k), v);
    setValues((prev) => ({ ...prev, [k]: v }));
  }, []);
  return [values, set] as const;
}

/** useState that survives restarts and follows changes made in the other window. */
export function usePersisted<T>(key: string, initial: T) {
  const [value, setValue] = useState<T>(() => read(key, initial));
  useEffect(() => write(key, value), [key, value]);
  useEffect(() => {
    // Setting an unchanged value fires no event, so this cannot ping-pong.
    const onStorage = (e: StorageEvent) => {
      if (e.key === key) setValue(read(key, initial));
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [key, initial]);
  return [value, setValue] as const;
}
