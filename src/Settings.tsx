import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import "./Settings.css";
import {
  CJK_FONTS,
  COVER_SHAPES,
  DEFAULTS,
  GLASS_TINTS,
  HIGHLIGHTS,
  KEYS,
  LYRIC_FONTS,
  LYRIC_WEIGHTS,
  PROVIDER_NAMES,
  SWEEP_ANGLES,
  cjkStack,
  fontStack,
  highlightVars,
  tierDots,
  usePersisted,
  useSettingsStore,
  type CjkLang,
  type Highlight,
  type Provider,
  type SettingKey,
  type Settings as SettingsValues,
} from "./settings";
import { romanizeText } from "./romanize";

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const HOLD_MS = 1000;

// ─────────────────────────── building blocks ───────────────────────────

/**
 * What the preview shows for a control under the pointer, before it is used:
 * any settings, overridden, plus a romanize try-out (romanize is not one of
 * the stored settings — it is the per-launch state the あ button flips).
 */
type Peek = Partial<SettingsValues> & { romanize?: boolean };

// While the pointer is on a setting: its explanation under the preview, and a
// try-out of what using it would do.
const HoverContext = createContext<(desc: string | null, peek?: Peek) => void>(() => {});

function Toggle({ on, onChange, disabled }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      disabled={disabled}
      className={`st-switch${on ? " on" : ""}`}
      onClick={() => onChange(!on)}
    >
      <span className="st-knob" />
    </button>
  );
}

function Slider(props: {
  min: number;
  max: number;
  step: number;
  value: number;
  label: string;
  onChange: (v: number) => void;
}) {
  const { min, max, step, value, label, onChange } = props;
  const pos = ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      className="st-slider"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={value}
      style={{ "--pos": `${pos}%` } as React.CSSProperties}
      onChange={(e) => onChange(Number(e.target.value))}
    />
  );
}

/** A row of choice chips. Pointing at one tries it in the preview (`peek`). */
function Chips<T extends string>(props: {
  options: { value: T; label: React.ReactNode }[];
  value: T;
  onChange: (v: T) => void;
  label: string;
  desc?: string;
  peek?: (v: T) => Peek;
}) {
  const hover = useContext(HoverContext);
  return (
    <div className="st-fonts" role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => (
        <button
          key={o.value}
          role="radio"
          aria-checked={props.value === o.value}
          className={`st-chip${props.value === o.value ? " on" : ""}`}
          onClick={() => props.onChange(o.value)}
          onMouseEnter={() => hover(props.desc ?? null, props.peek?.(o.value))}
          onMouseLeave={() => hover(null)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/**
 * A setting's name. Held down for HOLD_MS it resets that setting, and while
 * held it fills left to right the way a sung syllable does (see .st-holdable).
 * Only the name takes the hold, so dragging a slider can never reset it.
 */
function SettingLabel({ label, hint, onReset }: { label: string; hint?: string; onReset?: () => void }) {
  const [holding, setHolding] = useState(false);
  const timer = useRef(0);
  const stop = () => {
    clearTimeout(timer.current);
    setHolding(false);
  };
  useEffect(() => () => clearTimeout(timer.current), []);
  return (
    <span
      className={["st-label", onReset && "st-holdable", holding && "holding"].filter(Boolean).join(" ")}
      onPointerDown={(e) => {
        if (!onReset || e.button !== 0) return;
        setHolding(true);
        timer.current = window.setTimeout(() => {
          setHolding(false);
          onReset();
        }, HOLD_MS);
      }}
      onPointerUp={stop}
      onPointerLeave={stop}
      onPointerCancel={stop}
    >
      <span className="st-label-text">{label}</span>
      {hint && <span className="st-hint">{hint}</span>}
    </span>
  );
}

// ─────────────────────────── the settings, as data ───────────────────────────

const CATEGORIES = [
  { id: "lyrics", name: "Lyrics", icon: "♪" },
  { id: "languages", name: "Languages", icon: "あ" },
  { id: "text", name: "Text", icon: "Aa" },
  { id: "highlight", name: "Highlight", icon: "✦" },
  { id: "surface", name: "Surface", icon: "◐" },
  { id: "layout", name: "Layout", icon: "▤" },
  { id: "album", name: "Album mode", icon: "◼" },
  { id: "fun", name: "Fun", icon: "✿" },
  { id: "behavior", name: "Behavior", icon: "⚙" },
] as const;
type Category = (typeof CATEGORIES)[number]["id"];

/**
 * One setting in the window. Rows are drawn from these, which is what makes
 * search, per-section reset and hold-to-reset work the same for all of them.
 */
interface Item {
  id: string;
  cat: Category;
  label: string;
  desc: string;
  hint?: string;
  /** the stored setting, when there is one: hold-to-reset and Reset section use it */
  key?: SettingKey;
  /** reset for settings that are not a single stored value */
  reset?: () => void;
  /** beside the name */
  control?: React.ReactNode;
  /** under the name, full width (chips, lists) */
  below?: React.ReactNode;
  /** pointing at the row tries this in the preview */
  peek?: Peek;
}

const fmtOffset = (v: number) => (v === 0 ? "in sync" : `${v > 0 ? "+" : ""}${v.toFixed(1)}s`);
const pct = (v: number) => `${Math.round(v)}%`;
const times = (v: number) => `${Math.round(v * 100)}%`;

// ─────────────────────────── preview ───────────────────────────

// Sample lyrics per language. Kana and hangul only, no kanji: those two
// romanize without a dictionary, so the preview shows exactly what the widget
// would.
const PREVIEW_SAMPLES = {
  en: {
    name: "English",
    lines: [
      "Lines already sung stay dim",
      "The line being sung lights up",
      "the ones coming next wait",
      "and further ones fade back",
    ],
  },
  ja: { name: "日本語", lines: ["きみのこえが きこえる", "ひかりの なかで うたう", "よるが あける まで", "ずっと そばに いて"] },
  ko: { name: "한국어", lines: ["너의 목소리가 들려", "빛 속에서 노래해", "밤이 끝날 때까지", "언제나 곁에 있어"] },
  // Han has no algorithmic reading, so this one shows the font, not romanization
  zh: { name: "中文", lines: ["听见你的声音", "在光里歌唱", "直到天亮", "一直在你身边"] },
} as const;
type PreviewLang = keyof typeof PREVIEW_SAMPLES;
const PREVIEW_CURRENT = 1;
const FONT_LANG: Partial<Record<SettingKey, CjkLang>> = { fontJa: "ja", fontKo: "ko", fontZh: "zh" };

/**
 * The widget in miniature, drawn from the settings as they stand (with any
 * try-out on top): its glass and cover, the top row, four lyric lines around a
 * half-sung current one, and the progress row. It mirrors the widget's own
 * rules in App.css rather than mounting the real thing, which needs a player.
 */
function Preview(props: {
  s: SettingsValues;
  art: string | null;
  track: { title: string; artist: string } | null;
  romanized: boolean;
  peekRomanize?: boolean;
  peekLang?: CjkLang;
  caption: string;
}) {
  const { s } = props;
  const [chosenLang, setLang] = useState<PreviewLang>("en");
  // starts from the real Romanize state, then flips here without touching it
  const [roma, setRoma] = useState(props.romanized);
  // English has nothing to romanize, so a romanize try-out borrows Japanese
  const lang = props.peekLang ?? (props.peekRomanize && chosenLang === "en" ? "ja" : chosenLang);
  const canRoma = lang === "ja" || lang === "ko";
  const romanizing = (roma || !!props.peekRomanize) && canRoma;
  const k = Math.min(1, s.bgAlpha / DEFAULTS.bgAlpha);
  const style = {
    "--bg-a": s.bgAlpha / 100,
    "--bg-k": k,
    "--pv-tint": (GLASS_TINTS[s.glassTint] ?? GLASS_TINTS.dark).rgb,
    "--pv-edge": 0.12 * k * (s.edgeHighlight ? 1 : 0),
    "--pv-shadow": 0.45 * k * (s.shadowStrength / 100),
    "--pv-radius": `${Math.round(s.corners * 0.77)}px`,
    "--pv-cover-filter": `blur(${s.coverBlur * 0.6}px) saturate(${s.coverSaturation / 100}) brightness(${s.coverBrightness / 100})`,
    "--pv-cover-radius": COVER_SHAPES[s.coverShape],
    "--pv-meta-k": s.titleScale,
    "--pv-size": `${s.lyricSize}px`,
    "--pv-font": fontStack(s.lyricFont) ?? "var(--latin-stack)",
    "--pv-weight": LYRIC_WEIGHTS[s.lyricWeight],
    "--pv-lh": s.lineHeight,
    "--pv-tracking": s.letterSpacing ? `${s.letterSpacing}em` : "normal",
    "--pv-style": s.italicLyrics ? "italic" : "normal",
    "--pv-gap": `${Math.round(s.lineSpacing * 0.6)}px`,
    "--pv-ja": cjkStack("ja", s.fontJa),
    "--pv-ko": cjkStack("ko", s.fontKo),
    "--pv-zh": cjkStack("zh", s.fontZh),
    "--pv-angle": SWEEP_ANGLES[s.sweepDirection],
    "--pv-soft": `${s.sweepSoftness}%`,
    "--pv-lit-a": Math.min(1, 0.85 * s.highlightStrength),
    "--pv-glow": 0.35 * s.glowStrength,
    "--pv-bar-h": `${s.barThickness}px`,
    "--rainbow-s": `${s.rainbowSpeed}s`,
    "--pv-spin-s": `${s.spinSpeed}s`,
    ...highlightVars(s.highlight),
  } as React.CSSProperties;
  return (
    <div className="st-preview-wrap">
      <div
        className={[
          "st-preview",
          s.highlight === "rainbow" && "rainbow",
          s.partyBackground && "party",
          s.tintBar && "tint-bar",
        ]
          .filter(Boolean)
          .join(" ")}
        style={style}
        aria-label="Preview"
      >
        {props.art && <div className="st-preview-bg" style={{ backgroundImage: `url(${props.art})` }} />}
        <div className="st-pv-top">
          {s.showCover &&
            (props.art ? (
              <img
                className={["st-pv-art", s.spinCover && "spin", s.floatingCover && "float"].filter(Boolean).join(" ")}
                src={props.art}
                alt=""
              />
            ) : (
              <span className={["st-pv-art", s.spinCover && "spin"].filter(Boolean).join(" ")} />
            ))}
          <span className="st-pv-info">
            <span className="st-pv-title">{props.track?.title ?? "Song title"}</span>
            {s.showArtist && <span className="st-pv-artist">{props.track?.artist ?? "Artist"}</span>}
          </span>
          <span className="st-pv-langs" role="radiogroup" aria-label="Preview language">
            {(Object.keys(PREVIEW_SAMPLES) as PreviewLang[]).map((l) => (
              <button
                key={l}
                role="radio"
                aria-checked={lang === l}
                className={`st-pv-chip${lang === l ? " on" : ""}`}
                onClick={() => setLang(l)}
              >
                {PREVIEW_SAMPLES[l].name}
              </button>
            ))}
            <button
              className={`st-pv-chip st-pv-roma${romanizing ? " on" : ""}`}
              aria-pressed={romanizing}
              title="Romanize the preview"
              disabled={!canRoma}
              onClick={() => setRoma((r) => !r)}
            >
              あ
            </button>
          </span>
        </div>
        <div
          className={["st-pv-lyrics", s.lyricAlign === "center" && "centered", s.allCaps && "caps"]
            .filter(Boolean)
            .join(" ")}
          lang={lang}
        >
          {PREVIEW_SAMPLES[lang].lines.map((source, i) => {
            const text = romanizing ? romanizeText(source) : source;
            const distance = Math.abs(i - PREVIEW_CURRENT);
            // BLUR_STEP 0.55px per line away, capped at BLUR_MAX 2.2px
            const blur = s.blurLines ? Math.min(2.2, distance * 0.55) : 0;
            // .lyric-line's opacities: 0.497 sung, 0.51 not yet, 1 current
            const opacity =
              i === PREVIEW_CURRENT ? 1 : i < PREVIEW_CURRENT ? 0.497 * s.sungBrightness : 0.51 * s.upcomingBrightness;
            return (
              <div
                key={i}
                className={`st-pv-line${i === PREVIEW_CURRENT ? " current" : ""}`}
                style={{ "--pv-blur": `${blur}px`, opacity } as React.CSSProperties}
              >
                {text}
              </div>
            );
          })}
        </div>
        {(s.showProgress || s.showTimes) && (
          <div className="st-pv-bottom">
            {s.showProgress && (
              <div className="st-pv-bar">
                <span />
              </div>
            )}
            {s.showTimes && (
              <div className="st-pv-times">
                <span>1:12</span>
                <span>{s.showTotalTime ? "3:40" : "-2:28"}</span>
              </div>
            )}
          </div>
        )}
      </div>
      <div className="st-preview-caption">{props.caption}</div>
    </div>
  );
}

// ─────────────────────────── the window ───────────────────────────

export default function Settings() {
  const [s, set] = useSettingsStore();
  const [romanized, setRomanized] = usePersisted(KEYS.romanized, false);
  const [canRomanize] = usePersisted(KEYS.canRomanize, false);
  const [art] = usePersisted<string | null>(KEYS.art, null);
  const [track] = usePersisted<{ key: string; title: string; artist: string } | null>(KEYS.track, null);
  const [lyricSource] = usePersisted<{ provider: string; tier: number } | null>(KEYS.lyricSource, null);

  // The cover behind the card, cross-faded: the new one fades in over the one
  // before, so a song change never shows the bare card in between.
  const [cover, setCover] = useState<{ cur: string | null; prev: string | null }>({ cur: art, prev: null });
  if (art && art !== cover.cur) setCover({ cur: art, prev: cover.cur });

  const [cat, setCat] = useState<Category>("lyrics");
  const [query, setQuery] = useState("");
  const [explained, setExplained] = useState<string | null>(null);
  const [peek, setPeek] = useState<Peek>({});
  const hover = useCallback((desc: string | null, p?: Peek) => {
    setExplained(desc);
    setPeek(p ?? {});
  }, []);

  // ── per-song choices, stored as maps keyed by song ──
  const songOffset = track ? (s.songOffsets[track.key] ?? 0) : 0;
  const setSongOffset = (v: number) => {
    if (!track) return;
    const next = { ...s.songOffsets, [track.key]: v };
    // a song back at zero needs no entry
    if (v === 0) delete next[track.key];
    set("songOffsets", next);
  };
  const songProvider: Provider = (track && s.songProviders[track.key]) || "auto";
  const setSongProvider = (p: Provider) => {
    if (!track) return;
    const next = { ...s.songProviders, [track.key]: p };
    // "auto" here means "follow the all-songs choice", which needs no entry
    if (p === "auto") delete next[track.key];
    set("songProviders", next);
  };

  // ── installed fonts: for the font list, and to offer only language fonts you have ──
  const [fontsOpen, setFontsOpen] = useState(false);
  const [fonts, setFonts] = useState<string[] | null>(null);
  const [fontQuery, setFontQuery] = useState("");
  useEffect(() => {
    if ((!fontsOpen && cat !== "text" && cat !== "languages" && !query) || fonts) return;
    window.player?.listFonts?.().then(setFonts).catch(() => setFonts([]));
  }, [fontsOpen, cat, query, fonts]);
  const shownFonts = (fonts ?? []).filter((f) => f.toLowerCase().includes(fontQuery.trim().toLowerCase()));
  const customFont = s.lyricFont in LYRIC_FONTS ? null : s.lyricFont;

  // ── Spicy Lyrics key ──
  const [spicyKey, setSpicyKey] = useState<{ set: boolean; ending: string | null } | null>(null);
  const [keyBusy, setKeyBusy] = useState(false);
  const [pastingKey, setPastingKey] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  useEffect(() => {
    const refresh = () => window.player?.spicyKeyStatus?.().then(setSpicyKey).catch(() => {});
    refresh();
    return window.player?.onSpicyKeyChanged?.(refresh);
  }, []);
  const getKey = () => {
    if (keyBusy) return;
    setKeyBusy(true);
    setKeyError(null);
    window.player?.connectSpicyKey?.().finally(() => setKeyBusy(false));
  };
  const saveKey = async (raw: string) => {
    const res = await window.player?.setSpicyKey?.(raw);
    if (res?.ok) {
      setPastingKey(false);
      setKeyError(null);
    } else {
      setKeyError(res?.error ?? "could not save that key");
    }
  };
  const keyStatus = keyBusy
    ? "waiting for you to copy your key…"
    : spicyKey?.set
      ? `saved · ends in ${spicyKey.ending}`
      : "none — word-by-word lyrics then need Spotify open";

  // ── Uninstall: one click arms it, then hold to confirm ──
  const UNINSTALL_HOLD_MS = 1500;
  const [uninstall, setUninstall] = useState<"idle" | "armed" | "holding" | "busy">("idle");
  const [uninstallError, setUninstallError] = useState<string | null>(null);
  const holdTimer = useRef<number | null>(null);
  // disarms by itself if the hold never comes
  useEffect(() => {
    if (uninstall !== "armed") return;
    const id = setTimeout(() => setUninstall("idle"), 4000);
    return () => clearTimeout(id);
  }, [uninstall]);
  const runUninstall = async () => {
    setUninstall("busy");
    const res = await window.player?.uninstall?.();
    if (!res?.ok) {
      setUninstall("idle");
      setUninstallError(res?.error ?? "could not uninstall");
    }
  };
  const holdStart = () => {
    if (uninstall !== "armed") return;
    setUninstall("holding");
    holdTimer.current = window.setTimeout(runUninstall, UNINSTALL_HOLD_MS);
  };
  const holdEnd = () => {
    if (holdTimer.current == null) return;
    clearTimeout(holdTimer.current);
    holdTimer.current = null;
    setUninstall((u) => (u === "holding" ? "armed" : u));
  };

  // ── Spotify token ──
  const [auth, setAuth] = useState<{ signedIn: boolean; tokenExpiresAt: number | null } | null>(null);
  const [pasting, setPasting] = useState(false);
  const [tokenError, setTokenError] = useState<string | null>(null);
  const refreshAuth = useCallback(() => {
    window.player?.spotifyStatus().then(setAuth).catch(() => {});
  }, []);
  useEffect(() => {
    refreshAuth();
    return window.player?.onTokenArrived?.(refreshAuth);
  }, [refreshAuth]);
  const saveToken = async (raw: string) => {
    const res = await window.player?.spotifySetToken(raw);
    if (res?.ok) {
      setPasting(false);
      setTokenError(null);
    } else {
      setTokenError(res?.error ?? "could not save that token");
    }
  };
  const tokenStatus = auth?.signedIn
    ? auth.tokenExpiresAt
      ? `until ${new Date(auth.tokenExpiresAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`
      : "saved"
    : "none — Spicetify sends one when Spotify is open";

  // ── item builders: the common kinds of setting ──
  const toggle = (
    cat: Category,
    key: SettingKey,
    label: string,
    desc: string,
    opts: { preview?: boolean; hint?: string } = {},
  ): Item => ({
    id: key,
    cat,
    key,
    label,
    desc,
    hint: opts.hint,
    control: <Toggle on={!!s[key]} onChange={(v) => set(key, v as never)} />,
    // a visual switch tries its other position while pointed at
    peek: opts.preview ? ({ [key]: !s[key] } as Peek) : undefined,
  });
  const slider = (
    cat: Category,
    key: SettingKey,
    label: string,
    desc: string,
    o: { min: number; max: number; step: number; fmt: (v: number) => string },
  ): Item => ({
    id: key,
    cat,
    key,
    label,
    desc,
    hint: o.fmt(s[key] as number),
    control: (
      <Slider label={label} min={o.min} max={o.max} step={o.step} value={s[key] as number} onChange={(v) => set(key, v as never)} />
    ),
  });
  const chips = <T extends string>(
    cat: Category,
    key: SettingKey,
    label: string,
    desc: string,
    options: { value: T; label: React.ReactNode }[],
    o: { preview?: boolean; hint?: string } = { preview: true },
  ): Item => ({
    id: key,
    cat,
    key,
    label,
    desc,
    hint: o.hint,
    below: (
      <Chips
        label={label}
        options={options}
        value={s[key] as T}
        onChange={(v) => set(key, v as never)}
        desc={desc}
        peek={o.preview ? (v) => ({ [key]: v }) as Peek : undefined}
      />
    ),
  });
  const cjkChips = (key: "fontJa" | "fontKo" | "fontZh", lang: CjkLang) => {
    const have = fonts && new Set(fonts.map((f) => f.toLowerCase()));
    return chips(
      "languages",
      key,
      CJK_FONTS[lang].name,
      `Typeface for ${{ ja: "Japanese", ko: "Korean", zh: "Chinese" }[lang]} lyrics. Point at one to try it; only fonts you have are offered.`,
      [
        { value: "default", label: "Default" },
        ...CJK_FONTS[lang].options
          .filter((f) => !have || have.has(f.toLowerCase()))
          .map((f) => ({ value: f as string, label: <span style={{ fontFamily: `"${f}"` }}>{f}</span> })),
      ],
    );
  };

  // ── every setting ──
  const items: Item[] = [
    // Lyrics: where they come from and how they move
    {
      id: "lyricProvider",
      cat: "lyrics",
      key: "lyricProvider",
      label: "Provider · all songs",
      hint: lyricSource ? `now showing ${lyricSource.provider} ${tierDots(lyricSource.tier)}` : "Auto picks the best-timed lyrics",
      desc: "Where lyrics come from. Auto picks the best-timed source; pick one to always try it first.",
      below: (
        <Chips
          label="Preferred provider"
          options={(Object.keys(PROVIDER_NAMES) as Provider[]).map((p) => ({ value: p, label: PROVIDER_NAMES[p] }))}
          value={s.lyricProvider}
          onChange={(v) => set("lyricProvider", v)}
          desc="Where lyrics come from. Auto picks the best-timed source; pick one to always try it first."
        />
      ),
    },
    {
      id: "songProvider",
      cat: "lyrics",
      label: "Provider · this song",
      hint: track ? `${track.title} · Auto follows the choice above` : "nothing playing",
      desc: "Same as above, for the song playing now only.",
      reset: () => setSongProvider("auto"),
      below: (
        <Chips
          label="Provider for this song"
          options={(Object.keys(PROVIDER_NAMES) as Provider[]).map((p) => ({ value: p, label: PROVIDER_NAMES[p] }))}
          value={songProvider}
          onChange={setSongProvider}
          desc="Same as above, for the song playing now only."
        />
      ),
    },
    {
      id: "lyricOffset",
      cat: "lyrics",
      key: "lyricOffset",
      label: "Offset · all songs",
      hint: fmtOffset(s.lyricOffset),
      desc: "Lyrics running ahead or behind? Shift them later (+) or earlier (−) for every song. On the widget: [ and ].",
      control: (
        <Slider label="Lyric offset" min={-5} max={5} step={0.1} value={s.lyricOffset} onChange={(v) => set("lyricOffset", Math.round(v * 10) / 10)} />
      ),
    },
    {
      id: "songOffset",
      cat: "lyrics",
      label: "Offset · this song",
      hint: track ? `${track.title} · ${fmtOffset(songOffset)}` : "nothing playing",
      desc: "Same, for this song only; added to the all-songs offset. On the widget: ⇧ with [ and ], \\ resets.",
      reset: () => setSongOffset(0),
      control: (
        <Slider label="This song's offset" min={-5} max={5} step={0.1} value={songOffset} onChange={(v) => setSongOffset(Math.round(v * 10) / 10)} />
      ),
    },
    slider("lyrics", "autoSync", "Auto sync after scrolling", "After you scroll the lyrics, jump back to the line being sung this long after you stop. 0 turns it off.", {
      min: 0, max: 10, step: 1, fmt: (v) => (v ? `after ${v}s` : "off"),
    }),
    slider("lyrics", "followSpeed", "Follow speed", "How long the list takes to glide to the next line.", {
      min: 150, max: 900, step: 10, fmt: (v) => `${v} ms`,
    }),
    slider("lyrics", "scrollSpeed", "Scroll speed", "How far the lyrics move per mouse-wheel step.", {
      min: 0.25, max: 3, step: 0.25, fmt: (v) => `${v}×`,
    }),
    toggle("lyrics", "clickToJump", "Click a line to jump", "Clicking a lyric line moves the song to it."),
    toggle("lyrics", "showLineTimes", "Line times on hover", "A small badge with the line's start time while the pointer is on it."),
    toggle("lyrics", "hideGaps", "Hide instrumental dots", "Leave out the • • • shown during instrumental breaks."),
    toggle("lyrics", "hideBackingVocals", "Hide backing vocals", "Leave out the smaller backing-vocal lines."),
    toggle("lyrics", "hideCredits", "Hide credits", "Leave out the songwriter credits after the last line."),

    // Languages
    {
      id: "romanized",
      cat: "languages",
      label: "Romanize",
      hint: canRomanize ? undefined : "nothing to romanize in this song",
      desc: "Show Japanese, Korean or Chinese lyrics in Latin letters. Same as the あ button.",
      reset: () => setRomanized(s.romanizeDefault),
      control: <Toggle on={romanized} onChange={setRomanized} disabled={!canRomanize} />,
      peek: { romanize: true },
    },
    { ...toggle("languages", "romanizeDefault", "Romanize by default", "Start every launch with Romanize already on."), peek: { romanize: true } },
    cjkChips("fontJa", "ja"),
    cjkChips("fontKo", "ko"),
    cjkChips("fontZh", "zh"),

    // Text
    slider("text", "lyricSize", "Size", "How big the lyrics are.", { min: 13, max: 28, step: 1, fmt: (v) => `${v}px` }),
    {
      id: "lyricFont",
      cat: "text",
      key: "lyricFont",
      label: "Font",
      desc: "The lyrics' typeface. Point at one to try it in the preview.",
      below: (
        <>
          <div className="st-fonts" role="radiogroup" aria-label="Lyric font">
            {Object.entries(LYRIC_FONTS).map(([name, stack]) => (
              <button
                key={name}
                role="radio"
                aria-checked={s.lyricFont === name}
                className={`st-chip${s.lyricFont === name ? " on" : ""}`}
                style={{ fontFamily: stack ?? undefined }}
                onClick={() => set("lyricFont", name)}
                onMouseEnter={() => hover("The lyrics' typeface. Point at one to try it in the preview.", { lyricFont: name })}
                onMouseLeave={() => hover(null)}
              >
                {name}
              </button>
            ))}
            {customFont && (
              <button role="radio" aria-checked className="st-chip on" style={{ fontFamily: fontStack(customFont) ?? undefined }} onClick={() => setFontsOpen(true)}>
                {customFont}
              </button>
            )}
            <button className={`st-chip st-chip-more${fontsOpen ? " open" : ""}`} aria-expanded={fontsOpen} onClick={() => setFontsOpen((o) => !o)}>
              {fontsOpen ? "Done" : "More fonts…"}
            </button>
          </div>
          {fontsOpen && (
            <div className="st-fontlist">
              <input
                className="st-search-fonts"
                placeholder="Search installed fonts"
                autoFocus
                spellCheck={false}
                value={fontQuery}
                onChange={(e) => setFontQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setFontsOpen(false);
                }}
              />
              <div className="st-fontlist-scroll">
                {!fonts && <div className="st-hint st-fontlist-msg">Loading fonts…</div>}
                {fonts && !shownFonts.length && <div className="st-hint st-fontlist-msg">No installed font matches</div>}
                {shownFonts.map((f) => (
                  <button
                    key={f}
                    className={`st-font${s.lyricFont === f ? " on" : ""}`}
                    style={{ fontFamily: fontStack(f) ?? undefined }}
                    onClick={() => set("lyricFont", f)}
                    onMouseEnter={() => hover("Point at a font to try it in the preview.", { lyricFont: f })}
                    onMouseLeave={() => hover(null)}
                  >
                    {f}
                  </button>
                ))}
              </div>
              <div className="st-hint">Font file? Right-click it → Install, then reopen settings.</div>
            </div>
          )}
        </>
      ),
    },
    chips("text", "lyricWeight", "Weight", "How heavy the lyric letters are.", [
      { value: "regular", label: "Regular" },
      { value: "semibold", label: "Semibold" },
      { value: "bold", label: "Bold" },
    ]),
    slider("text", "lineHeight", "Line height", "Room within a wrapped line, top to bottom.", { min: 1, max: 1.8, step: 0.02, fmt: (v) => v.toFixed(2) }),
    slider("text", "lineSpacing", "Line spacing", "Space between one lyric line and the next.", { min: 2, max: 28, step: 1, fmt: (v) => `${v}px` }),
    slider("text", "letterSpacing", "Letter spacing", "Space between letters: tighter or looser than the font's own.", {
      min: -0.05, max: 0.2, step: 0.01, fmt: (v) => (v ? `${v > 0 ? "+" : ""}${v.toFixed(2)}em` : "normal"),
    }),
    chips("text", "lyricAlign", "Alignment", "Lyrics against the left edge, or centred.", [
      { value: "left", label: "Left" },
      { value: "center", label: "Center" },
    ]),
    toggle("text", "italicLyrics", "Italic", "Slanted lyrics.", { preview: true }),
    slider("text", "titleScale", "Title size", "Size of the song title and artist.", { min: 0.8, max: 1.6, step: 0.05, fmt: times }),

    // Highlight: how the line being sung lights up
    slider("highlight", "highlightStrength", "Highlight strength", "How bright the sung part of the line is.", { min: 0.5, max: 1.2, step: 0.05, fmt: times }),
    slider("highlight", "glowStrength", "Glow", "The soft glow around letters as they are sung.", { min: 0, max: 3, step: 0.1, fmt: times }),
    chips("highlight", "sweepDirection", "Sweep direction", "The fill runs along the line as it is sung, or top to bottom over each word.", [
      { value: "across", label: "Along the line" },
      { value: "down", label: "Top to bottom" },
    ]),
    slider("highlight", "sweepSoftness", "Sweep edge", "How soft the edge between sung and unsung is.", { min: 0, max: 50, step: 1, fmt: pct }),
    slider("highlight", "upcomingBrightness", "Upcoming lines", "How bright the lines not yet sung are.", { min: 0.2, max: 1.9, step: 0.05, fmt: times }),
    slider("highlight", "sungBrightness", "Sung lines", "How bright the lines already sung are.", { min: 0.2, max: 2, step: 0.05, fmt: times }),
    toggle("highlight", "blurLines", "Blur distant lines", "Soften lines the further they are from the one being sung.", { preview: true }),

    // Surface: the glass and the cover behind it
    slider("surface", "bgAlpha", "Background", "How see-through the widget is: 0% clear, 72% the default glass, 100% solid.", {
      min: 0, max: 100, step: 1, fmt: (v) => `${v}%${v === 0 ? " · clear" : v === 100 ? " · solid" : v === DEFAULTS.bgAlpha ? " · default" : ""}`,
    }),
    chips("surface", "glassTint", "Glass tint", "The colour of the glass itself.",
      (Object.keys(GLASS_TINTS) as (keyof typeof GLASS_TINTS)[]).map((t) => ({ value: t, label: GLASS_TINTS[t].name }))),
    slider("surface", "corners", "Corners", "How rounded the widget's corners are.", { min: 0, max: 24, step: 1, fmt: (v) => `${v}px` }),
    slider("surface", "coverBlur", "Cover blur", "How soft the cover behind the widget is: 0 shows it sharp.", { min: 0, max: 100, step: 5, fmt: (v) => `${v}px` }),
    slider("surface", "coverSaturation", "Cover colour", "How strong the cover's colours are behind the widget.", { min: 0, max: 250, step: 5, fmt: pct }),
    slider("surface", "coverBrightness", "Cover brightness", "How bright the cover is behind the widget.", { min: 30, max: 160, step: 5, fmt: pct }),
    toggle("surface", "edgeHighlight", "Edge highlight", "The thin light line around the widget's edge.", { preview: true }),
    slider("surface", "shadowStrength", "Shadow", "The drop shadow under the widget.", { min: 0, max: 200, step: 5, fmt: pct }),
    slider("surface", "restOpacity", "Fade when not hovered", "How visible the whole widget stays while the pointer is elsewhere.", { min: 20, max: 100, step: 5, fmt: pct }),

    // Layout: what is shown and how big it is
    slider("layout", "widgetWidth", "Widget width", "How wide the widget is.", { min: 300, max: 480, step: 10, fmt: (v) => `${v}px` }),
    slider("layout", "panelHeight", "Lyrics height", "How tall the lyrics panel is.", { min: 160, max: 520, step: 10, fmt: (v) => `${v}px` }),
    toggle("layout", "showCover", "Show cover", "The small cover next to the title.", { preview: true }),
    chips("layout", "coverShape", "Cover shape", "The small cover next to the title: rounded, square or a circle.", [
      { value: "rounded", label: "Rounded" },
      { value: "square", label: "Square" },
      { value: "circle", label: "Circle" },
    ]),
    toggle("layout", "showArtist", "Show artist", "The artist under the title.", { preview: true }),
    toggle("layout", "showNext", "Show the next song", "The NEXT label top right, from Spotify's queue. Click it to skip."),
    toggle("layout", "showSource", "Show source label", "The player and lyrics source under the progress bar, e.g. Spotify · spicy ●●●."),
    toggle("layout", "showTimes", "Show times", "The elapsed and remaining times.", { preview: true }),
    toggle("layout", "showProgress", "Show progress bar", "The bar showing how far into the song you are.", { preview: true }),
    slider("layout", "barThickness", "Progress bar thickness", "How thick the progress bar is.", { min: 1, max: 6, step: 1, fmt: (v) => `${v}px` }),
    toggle("layout", "tintBar", "Colour the progress bar", "Light the progress bar in the Highlight colour.", { preview: true }),
    toggle("layout", "showShuffleRepeat", "Show shuffle & repeat", "The shuffle and repeat buttons."),
    toggle("layout", "showVolume", "Show volume", "The volume button."),
    toggle("layout", "pinControls", "Controls always visible", "Keep the play controls shown, not only while the pointer is on the widget."),

    // Album mode
    toggle("album", "albumOnLaunch", "Open in album mode", "Start in album mode, with the big cover."),
    toggle("album", "showVisualizer", "Visualizer", "The bars next to the title, moving with the music."),
    slider("album", "bubbleOpacity", "Lyric bubble", "How dark the bubble behind the album-mode lyric is.", { min: 0, max: 100, step: 2, fmt: pct }),
    slider("album", "albumLyricSize", "Lyric size", "How big the album-mode lyric is.", { min: 11, max: 24, step: 1, fmt: (v) => `${v}px` }),

    // Fun
    chips("fun", "highlight", "Highlight colour", "The colour the sung part of each line lights up in. Rainbow cycles through them all.",
      (Object.keys(HIGHLIGHTS) as Highlight[]).map((h) => ({
        value: h,
        label: (
          <>
            <span
              className={`st-swatch st-swatch-${h}`}
              style={{ "--sw-h": HIGHLIGHTS[h].h, "--sw-s": `${HIGHLIGHTS[h].s}%`, "--sw-l": `${HIGHLIGHTS[h].l}%` } as React.CSSProperties}
            />
            {HIGHLIGHTS[h].name}
          </>
        ),
      }))),
    slider("fun", "rainbowSpeed", "Rainbow speed", "Seconds for one trip round the colours, with the Rainbow highlight.", { min: 1, max: 20, step: 1, fmt: (v) => `${v}s` }),
    toggle("fun", "spinCover", "Spinning cover", "The small cover turns round like a record while music plays.", { preview: true }),
    slider("fun", "spinSpeed", "Spin speed", "Seconds per turn of the spinning cover.", { min: 1, max: 20, step: 1, fmt: (v) => `${v}s` }),
    chips("fun", "lyricMotion", "Lyric motion", "How much the sung letters bounce: Calm keeps them still, Bouncy doubles the wave.", [
      { value: "calm", label: "Calm" },
      { value: "normal", label: "Normal" },
      { value: "bouncy", label: "Bouncy" },
    ], { preview: false }),
    toggle("fun", "floatingCover", "Floating cover", "The small cover gently bobs up and down.", { preview: true }),
    toggle("fun", "partyBackground", "Party background", "The cover's glow behind the widget cycles through every colour.", { preview: true }),
    toggle("fun", "allCaps", "ALL CAPS", "Lyrics in capital letters, in the panel and in album mode.", { preview: true }),

    // Behavior
    toggle("behavior", "lyricsOnLaunch", "Open lyrics on launch", "Start with the lyrics panel open instead of just the player."),
    toggle("behavior", "showTotalTime", "Show song length", "The time on the right shows the song's length instead of the time left. Clicking it does the same.", { preview: true }),
    toggle("behavior", "hideWhenIdle", "Hide when nothing plays", "The widget disappears while no music plays, and comes back when it starts."),
    toggle("behavior", "showChangelog", "Show changelog after updates", "Open What's new on the first launch of each new version."),
    toggle("behavior", "alwaysOnTop", "Always on top", "Keep the widget above every other window."),

    // Lyrics keys
    {
      id: "spicyKey",
      cat: "behavior",
      label: "Spicy Lyrics key",
      hint: keyStatus,
      desc: "Your own free key for word-by-word lyrics from Spicy Lyrics, with any player. Get key opens Spicy Lyrics: sign in, create an application, then copy its key.",
      control: (
        <span className="st-inline">
          {spicyKey?.set && (
            <button className="st-pill" onClick={() => window.player?.clearSpicyKey?.()}>
              Clear
            </button>
          )}
          <button
            className="st-pill"
            onClick={() => {
              setKeyError(null);
              setPastingKey((p) => !p);
            }}
          >
            {pastingKey ? "Cancel" : "Paste"}
          </button>
          <button className="st-pill" disabled={keyBusy} onClick={getKey}>
            {spicyKey?.set ? "Change" : "Get key"}
          </button>
        </span>
      ),
      below: (pastingKey || keyError) && (
        <>
          {pastingKey && (
            <form
              className="st-token"
              onSubmit={(e) => {
                e.preventDefault();
                saveKey((e.currentTarget.elements.namedItem("spicyKey") as HTMLInputElement).value);
              }}
            >
              <input
                name="spicyKey"
                type="password"
                placeholder="Paste Spicy Lyrics key (sl_sk_…)"
                autoFocus
                spellCheck={false}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setPastingKey(false);
                }}
              />
              <button className="st-pill" type="submit">
                Save
              </button>
            </form>
          )}
          {keyError && <div className="st-error">{keyError}</div>}
        </>
      ),
    },
    {
      id: "token",
      cat: "behavior",
      label: "Spotify token",
      hint: tokenStatus,
      desc: "Spotify's pass for word-by-word lyrics when there is no Spicy Lyrics key, or it fails. Spotify sends it by itself while it is open.",
      control: (
        <span className="st-inline">
          {auth?.signedIn && (
            <button
              className="st-pill"
              onClick={async () => {
                await window.player?.spotifySignOut();
                refreshAuth();
              }}
            >
              Clear
            </button>
          )}
          <button
            className="st-pill"
            onClick={() => {
              setTokenError(null);
              setPasting((p) => !p);
            }}
          >
            {pasting ? "Cancel" : "Paste"}
          </button>
        </span>
      ),
      below: pasting && (
        <>
          <form
            className="st-token"
            onSubmit={(e) => {
              e.preventDefault();
              saveToken((e.currentTarget.elements.namedItem("token") as HTMLInputElement).value);
            }}
          >
            <input
              name="token"
              type="password"
              placeholder="Paste Spotify access token"
              autoFocus
              spellCheck={false}
              onKeyDown={(e) => {
                if (e.key === "Escape") setPasting(false);
              }}
            />
            <button className="st-pill" type="submit">
              Save
            </button>
          </form>
          {tokenError && <div className="st-error">{tokenError}</div>}
        </>
      ),
    },
    {
      id: "uninstall",
      cat: "behavior",
      label: "Uninstall player",
      hint: uninstall === "busy" ? "removing the bridge, then closing…" : "removes everything, like it was never installed",
      desc: "Removes player, its settings, your Spicy Lyrics key and sign-in, saved lyrics, and the bridge from Spotify (Spotify restarts once).",
      control: (
        <button
          className={`st-pill st-danger${uninstall === "armed" || uninstall === "holding" ? " confirm" : ""}${
            uninstall === "holding" ? " holding" : ""
          }`}
          style={{ "--hold-ms": `${UNINSTALL_HOLD_MS}ms` } as React.CSSProperties}
          disabled={uninstall === "busy"}
          onClick={() => {
            if (uninstall !== "idle") return;
            setUninstallError(null);
            setUninstall("armed");
          }}
          onPointerDown={holdStart}
          onPointerUp={holdEnd}
          onPointerLeave={holdEnd}
        >
          {uninstall === "busy"
            ? "Uninstalling…"
            : uninstall === "holding"
              ? "Keep holding…"
              : uninstall === "armed"
                ? "Hold to uninstall"
                : "Uninstall"}
        </button>
      ),
      below: uninstallError && <div className="st-error">{uninstallError}</div>,
    },
  ];

  const resetItem = (it: Item) => (it.key ? () => set(it.key!, DEFAULTS[it.key!] as never) : it.reset);
  const resetItems = (list: Item[]) => list.forEach((it) => resetItem(it)?.());

  const q = query.trim().toLowerCase();
  const shown = q
    ? items.filter((it) => `${it.label} ${it.desc}`.toLowerCase().includes(q))
    : items.filter((it) => it.cat === cat);

  // Two clicks, so a stray one cannot wipe everything.
  const [confirmReset, setConfirmReset] = useState<"section" | "all" | null>(null);
  useEffect(() => {
    if (!confirmReset) return;
    const id = setTimeout(() => setConfirmReset(null), 3000);
    return () => clearTimeout(id);
  }, [confirmReset]);

  // Size the window to the card, the same way the widget does.
  const cardRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => window.player?.resizeSettings?.(el.getBoundingClientRect().height + STAGE_PAD * 2));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const effective = { ...s, ...peek } as SettingsValues;
  const peekLang = (Object.keys(peek) as SettingKey[]).map((k) => FONT_LANG[k]).find(Boolean);
  const catName = (c: Category) => CATEGORIES.find((x) => x.id === c)?.name ?? c;

  return (
    <div
      className="stage"
      onContextMenu={(e) => {
        e.preventDefault();
        window.player?.closeSettings?.();
      }}
    >
      <div className="st-card" ref={cardRef}>
        {cover.prev && <div className="st-bg" style={{ backgroundImage: `url(${cover.prev})` }} />}
        {cover.cur && <div key={cover.cur} className="st-bg st-bg-in" style={{ backgroundImage: `url(${cover.cur})` }} />}

        <header className="st-head">
          <div>
            <div className="st-title">Settings</div>
            <div className="st-sub">Right-click to close</div>
          </div>
          <input
            className="st-search"
            type="search"
            placeholder={`Search ${items.length} settings`}
            spellCheck={false}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQuery("");
            }}
          />
          <button className="st-close" aria-label="Close" onClick={() => window.player?.closeSettings?.()}>
            <svg viewBox="0 0 16 16" width="12" height="12">
              <path d="M3.5 3.5l9 9m0-9l-9 9" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </svg>
          </button>
        </header>

        <HoverContext.Provider value={hover}>
          <div className="st-body">
            <nav className="st-nav" aria-label="Settings sections">
              {CATEGORIES.map((c) => (
                <button
                  key={c.id}
                  className={`st-nav-item${!q && cat === c.id ? " on" : ""}`}
                  aria-current={!q && cat === c.id ? "page" : undefined}
                  onClick={() => {
                    setQuery("");
                    setCat(c.id);
                  }}
                >
                  <span className="st-nav-icon" aria-hidden="true">
                    {c.icon}
                  </span>
                  {c.name}
                  <span className="st-nav-count">{items.filter((it) => it.cat === c.id).length}</span>
                </button>
              ))}
            </nav>

            <div className="st-main">
              <Preview
                s={effective}
                art={art}
                track={track}
                romanized={romanized}
                peekRomanize={peek.romanize}
                peekLang={peekLang}
                caption={explained ?? "Point at a setting to try it here · hold its name 1 s to reset it"}
              />
              <div className="st-list">
                <div className="st-list-head">
                  {q ? `${shown.length} result${shown.length === 1 ? "" : "s"} for “${query.trim()}”` : catName(cat)}
                </div>
                {shown.map((it, i) => (
                  <div key={it.id}>
                    {q && (i === 0 || shown[i - 1].cat !== it.cat) && <div className="st-result-cat">{catName(it.cat)}</div>}
                    <div className="st-item" onMouseEnter={() => hover(it.desc, it.peek)} onMouseLeave={() => hover(null)}>
                      <div className="st-row">
                        <SettingLabel label={it.label} hint={it.hint} onReset={resetItem(it)} />
                        {it.control}
                      </div>
                      {it.below}
                    </div>
                  </div>
                ))}
                {!shown.length && <div className="st-hint st-empty">No setting matches — try another word.</div>}
              </div>
            </div>
          </div>
        </HoverContext.Provider>

        <footer className="st-foot">
          <button className="st-reset" onClick={() => window.player?.openChangelog?.()}>
            What's new
          </button>
          <span className="st-foot-right">
            {!q && (
              <button
                className={`st-reset${confirmReset === "section" ? " confirm" : ""}`}
                onClick={() => {
                  if (confirmReset === "section") {
                    resetItems(shown);
                    setConfirmReset(null);
                  } else setConfirmReset("section");
                }}
              >
                {confirmReset === "section" ? "Click again to reset" : `Reset ${catName(cat)}`}
              </button>
            )}
            <button
              className={`st-reset${confirmReset === "all" ? " confirm" : ""}`}
              onClick={() => {
                if (confirmReset === "all") {
                  resetItems(items);
                  // every song's own offset and provider, not just this one's
                  set("songOffsets", DEFAULTS.songOffsets);
                  set("songProviders", DEFAULTS.songProviders);
                  setConfirmReset(null);
                } else setConfirmReset("all");
              }}
            >
              {confirmReset === "all" ? "Click again to reset" : "Reset all settings"}
            </button>
          </span>
        </footer>
      </div>
    </div>
  );
}
