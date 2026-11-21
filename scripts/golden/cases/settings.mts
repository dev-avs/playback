// golden/settings.json: src/settings.ts and snapshotSettingsJson() (src/windows/Versions.tsx),
// plus the rows scripts/gen-gpui-settings.mts extracted from src/windows/Settings.tsx.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import type { Fixtures } from "../run.mts";
import * as S from "../../../src/settings.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");

/** a fake localStorage over a plain object */
function fakeStorage(items: Record<string, string>) {
  const written: Record<string, string> = {};
  (globalThis as any).localStorage = {
    getItem: (k: string) => (k in items ? items[k] : null),
    setItem: (k: string, v: string) => {
      written[k] = v;
    },
  };
  return written;
}

/** snapshotSettingsJson from Versions.tsx, compiled from its own source */
function snapshotFn(): () => string {
  const text = readFileSync(join(root, "src/windows/Versions.tsx"), "utf8");
  const start = text.indexOf("function snapshotSettingsJson");
  const end = text.indexOf("\n}", start) + 2; // the closing brace of the function, at column 0
  const js = ts.transpile(text.slice(start, end));
  return new Function("DEFAULTS", "KEYS", `${js}\nreturn snapshotSettingsJson;`)(S.DEFAULTS, S.KEYS);
}

export default (fx: Fixtures) => {
  const D = S.DEFAULTS as Record<string, unknown>;
  const K = S.KEYS as Record<string, string>;

  // ── snapshot scenarios: what localStorage holds → the file's text ──
  const snap = snapshotFn();
  const scenarios: Record<string, string>[] = [
    {},
    { "set.lyricSize": "21", lyricOffset: "-0.3", "set.highlight": '"gold"', "set.alwaysOnTop": "false" },
    { "set.songOffsets": '{"artist|title":1.5,"a|b":-0.2}', "set.songProviders": '{"artist|title":"netease"}' },
    { "set.lineHeight": "1.18", "set.letterSpacing": "-0.05", "set.scrollSpeed": "0.25", "set.glowStrength": "0.1" },
    { "set.lyricSize": "not json", "set.corners": "", "set.panSpeed": "NaN" },
    { "set.romanizeDefault": "true", "set.nextSong": "null", "set.coverBlur": '"50"', "set.fontJa": '"Yu \\"Gothic\\""' },
    { "set.lyricFont": '"Segoe UI"', "set.bgAlpha": "1e2", "set.widgetWidth": "350.0", "set.rainbowSpeed": "1e21" },
    { "set.lyricFont": '"日本語 ✓ \\u2028 \\u007f"', "set.followSpeed": "9007199254740993" },
    { "state.track": '{"key":"a|b"}', "set.unknown": "1" },
  ];
  const snapshots = scenarios.map((storage) => {
    fakeStorage(storage);
    return { storage, out: snap() };
  });

  // ── read / write over JSON texts ──
  const readRaws: (string | null)[] = [
    null, "", " ", "5", " 5 ", "-0", "1.18", "1e21", "1e-7", "0.1", "true", "null", '"x"', '"\\u00e9"', "{}", "[]",
    '{"a":1,"b":[1,2,{"c":null}]}', "undefined", "NaN", "{", "[1,]", "'x'", "01", "1.", ".5", "+1", " 5", "123456789012345678901234567890",
  ];
  const reads = readRaws.map((raw) => {
    fakeStorage(raw == null ? {} : { k: raw });
    return { raw, out: S.read("k", "INIT") };
  });
  const writeTexts = [
    "null", "true", "false", "0", "1", "-1", "1.18", "0.1", "1e21", "1e-7", "1.5e300", "123456789.123", "0.000001", "0.0000001",
    '"x"', '"a\\"b\\\\c\\n\\t\\u0001é日本"', "[]", "{}", "[1,[2,3],{}]", '{"b":1,"a":{"x":[true,null,"s"]}}', '{"songs":{"a|b":1.5}}',
    "100", "1e3", "12345678901234567890", "-0.5", "2.5e-5",
  ];
  const writes = writeTexts.map((text) => {
    const written = fakeStorage({});
    S.write("k", JSON.parse(text));
    return { in: text, out: written.k };
  });

  // ── the showNext → nextSong carry-over (settings.ts top level), run as written there ──
  const carry = (
    [
      [null, null],
      [null, "false"],
      [null, "true"],
      [null, "x"],
      ['"label"', "false"],
      ['"off"', null],
      ["null", "false"],
      ["", "false"],
    ] as [string | null, string | null][]
  ).map(([nextSong, showNext]) => {
    const items: Record<string, string> = {};
    if (nextSong != null) items["set.nextSong"] = nextSong;
    if (showNext != null) items["set.showNext"] = showNext;
    const written = fakeStorage(items);
    try {
      if (localStorage.getItem("set.nextSong") == null && localStorage.getItem("set.showNext") === "false")
        localStorage.setItem("set.nextSong", JSON.stringify("off"));
    } catch {
      /* nothing */
    }
    return { nextSong, showNext, out: written["set.nextSong"] ?? null };
  });

  // ── helpers ──
  const fontNames = [...Object.keys(S.LYRIC_FONTS), "", "Segoe UI Variable", 'a"b\\c', '"', "\\", "Noto Sans JP", "日本語", " ", "default", "Default "];
  const romaIn: unknown[] = [true, false, "off", "latin", "dual", "Off", "both", "", null, 0, 1, "latin ", [], {}, 1.5];
  const hl = [...Object.keys(S.HIGHLIGHTS), "bogus", "", "White"];
  const cjkFonts = ["default", "Meiryo", "", 'a"b', "\\", "Noto Sans KR", "日本語", "Default"];
  const tiers = [0, 1, 2, 3, 4, 5, 1.5, 2.7, 0.2, 3.5, 100, -1, -0.5, 1e21, Infinity];
  const songs = [
    ["Artist", "Title"], ["  Artist  ", "\tTitle\n"], ["ÄRTIST", "ΣΊΣΥΦΟΣ"], ["", ""], ["a|b", "c|d"], [" x ", "﻿y"], ["\u0085z", "z\u0085"],
    ["İstanbul", "ǅ"], ["ΟΔΥΣΣΕΎΣ", "Σ"], ["日本語", "한국어"], ["᠎", "​"],
  ];
  const sources: (string | null)[] = [null, "", "netease/yrc", "spicy", "a/b/c", "/x", "x/", "lrclib/lrc"];

  const helpers = {
    fontStack: fontNames.map((n) => ({ in: n, out: S.fontStack(n) })),
    romaMode: romaIn.map((v) => ({ in: v, out: S.romaMode(v) })),
    highlightVars: hl.map((h) => ({ in: h, out: S.highlightVars(h as S.Highlight) })),
    cjkStack: (["ja", "ko", "zh"] as const).flatMap((l) => cjkFonts.map((f) => ({ lang: l, font: f, out: S.cjkStack(l, f) }))),
    tierDots: tiers.map((t) => {
      try {
        return { in: String(t), out: S.tierDots(t) };
      } catch {
        return { in: String(t), out: null };
      }
    }),
    songKey: songs.map(([a, t]) => ({ artist: a, title: t, out: S.songKey(a, t) })),
    providerOf: sources.map((s) => ({ in: s, out: S.providerOf(s ?? undefined) })),
  };

  // ── slider formats, evaluated from the sources the generator extracted ──
  const items = fx.json("settings/items.json");
  const edge = [0, 0.5, 1, -1, 0.005, 0.015, 1.005, 2.5, -2.5, 0.125, 0.375, 12.345, -0.0001, 100, 72, 99.5, 1e21, 1e-7, 1234567.891, 0.049999999999999996, 8.345, -0.05, -0.01];
  const fmtOut: Record<string, { v: number; out: string }[]> = {};
  const used = new Map<string, any[]>();
  for (const it of items.items)
    if (it.control.kind === "slider" && it.control.fmt) used.set(it.control.fmt, [...(used.get(it.control.fmt) ?? []), it.control]);
  for (const [name, def] of Object.entries<any>(items.fmts)) {
    const fn = new Function(
      "DEFAULTS",
      `const pct = ${items.fmts.Pct.js}, times = ${items.fmts.Times.js}, fmtOffset = ${items.fmts.FmtOffset.js}; return (${def.js});`,
    )(S.DEFAULTS);
    const values = [...edge];
    for (const c of used.get(name) ?? []) {
      const n = Math.round((c.max - c.min) / c.step);
      for (let i = 0; i <= n; i++) values.push(Number((c.min + i * c.step).toFixed(6)));
    }
    fmtOut[name] = values.map((v) => ({ v, out: fn(v) }));
  }
  const numbers = edge
    .flatMap((v) => [v, v * 10, v / 7])
    .concat([0.1 + 0.2, 1 / 3, 5e-7, 123456789012345680000, 1e21, 1.7976931348623157e308, 5e-324, 4.35, 1.45, 10.235, 1.0000000000000002]);
  const jsNum = numbers.map((v) => ({
    v,
    str: String(v),
    round: Math.round(v),
    fixed: [0, 1, 2, 3].map((d) => v.toFixed(d)),
  }));

  return {
    defaults: D,
    keys: K,
    defaultKeyOrder: Object.keys(D),
    keyOrder: Object.keys(K),
    snapshots,
    reads,
    writes,
    carry,
    helpers,
    logoArt: S.LOGO_ART,
    tables: {
      LYRIC_FONTS: S.LYRIC_FONTS,
      ROMA_MODES: S.ROMA_MODES,
      ROMA_LABELS: S.ROMA_LABELS,
      HIGHLIGHTS: S.HIGHLIGHTS,
      SWEEP_ANGLES: S.SWEEP_ANGLES,
      SWEEP_ANGLES_RTL: S.SWEEP_ANGLES_RTL,
      GLASS_TINTS: S.GLASS_TINTS,
      LYRIC_WEIGHTS: S.LYRIC_WEIGHTS,
      COVER_SHAPES: S.COVER_SHAPES,
      CJK_FONTS: S.CJK_FONTS,
      MOTIONS: S.MOTIONS,
      PROVIDERS: S.PROVIDERS,
      PROVIDER_NAMES: S.PROVIDER_NAMES,
    },
    items,
    fmt: fmtOut,
    jsNum,
  };
};
