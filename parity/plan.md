# Project Phoenix: phase plan

Concrete plan for `port.md`. Written 2026-11-20 against the repo at `0163b39` (+ working tree).

## Repo facts this plan is built on
- `src-tauri/src`: 7.8k lines. 35 files have no Tauri code (lyrics, media, spicy api/cache/key, spotify,
  spicetify, support, art, updates/feed+download) and move as-is.
- 7 files touch Tauri: `bridge/{mod,server}`, `updates/mod`, `spicy/connect`, `art/thumbnail`,
  `lyrics/readings`, `migrate`. Pure glue: `commands`, `ui/*`, `main`, `launch`, `install`.
- UI: ~400 rule blocks in `App.css` (~640 across all CSS), 96 transition/animation lines, 22 `@keyframes`,
  83 settings in `DEFAULTS`, Settings `items` at `src/windows/Settings.tsx:777`, 7 `BrowserWindow`s in
  `electron/main.cjs`.
- Snapshot harness: 5 scenes today (`scripts/snapshot/main.cjs`). `gpui-0.2.2` is already in the cargo
  registry.

## Phase 0: shared backend (Tauri unchanged)
1. Create `crates/playback-core`; `src-tauri` uses it through a path dependency (not a root workspace,
   so target dirs and `scripts/pack.cjs` paths stay put).
2. Move the 35 Tauri-free files as-is. Put `lindera` behind a `readings` feature.
3. Decouple the 7 Tauri files:
   - `tauri::async_runtime::spawn*` → `tokio::spawn*`.
   - `AppHandle` → `trait Host { emit(&str, Value); data_dir(); version(); spotify_playing() }`;
     `src-tauri` implements it (~30 lines).
   - `bridge::server::start(Arc<dyn Host>)`, `Updates::check(&dyn Host)`.
   - `spicy/connect` and `migrate` stay in `src-tauri` (webview / Tauri specific).
4. Gate: `cargo test`, `npm test`, `npm run package` dry build, Tauri preview approved by the user.

**Frame-driver spike: done, passes (2026-11-20).** `spikes/frame-driver` (gpui 0.2.2), 350 ms tweens,
6 per run, frame timestamps in `spikes/frame-driver/logs*/`:
- `request_animation_frame()` from render, from a canvas prepaint, or `on_next_frame` + notify: ~180 fps
  (this display's rate), worst gap < 10 ms, with no resize, one resize, a resize every frame (`follow`),
  a resize from `on_children_prepainted` (`measure`), and a redundant resize each render (`spam`).
  `follow` renders ~470/s (extra renders on resize), still no gap > 8.4 ms.
- A background `timer(4ms)` + notify: 64 fps, gaps up to 21 ms. Rejected.
- The first attempt's stall (2 frames, then a 250 ms idle) did not reproduce. Use request_animation_frame
  from render; keep a frame-gap log in the real driver (Phase 3) to catch a regression.

**In parallel: frame-driver spike (port.md 2.5).** One test window, a 350ms tween, a window resize
mid-tween, frame-timestamp log. If GPUI 0.2.2 cannot hold ≥55 fps here the port is blocked, so find out
on day 1, not in Phase 3.

## Phase 1: logic port + golden tests
- **Where (decided 2026-11-20):** the ported logic lives in `src-gpui/logic` (pure Rust, no GPUI,
  so its tests build in seconds); `src-gpui` will depend on it. Golden JSON goes to
  `src-gpui/logic/golden/`, fixtures to `scripts/golden/fixtures/`, one case file per module in
  `scripts/golden/cases/`. Settings codegen writes `src-gpui/logic/src/settings_gen.rs` and
  `settings_items_gen.rs` (not `src-gpui/src/`).
- `romanize.ts` uses `pinyin-pro` 3.29.5 (npm, MIT) for Chinese. **Decided (2026-11-20, left to the
  conductor):** port pinyin-pro itself: export its dictionaries to `src-gpui/logic/data/` with its
  licence, and translate only the path Playback calls (`pinyin(text, { type: "all", toneType: "none" })`),
  word segmentation and polyphone rules included. No other pinyin crate: their readings differ.
- `scripts/golden/run.mts` (loads TS the way `tests/*.mts` already do) runs over
  `scripts/golden/fixtures/` and writes `golden/*.json`.
- Fixtures: real Spicy (syllable, line, static), NetEase YRC, LRCLIB, lrcmux payloads; edge cases: gaps,
  backing vocals, CJK, every romanize mode, empty lines, RTL.
- Port order, one worker each: `uroman` → `romanize` → `lyrics` → `spicy` → `lyricAnimator` (springs
  sampled at dt = 1/60 and 1/144 for 2s) → `App.tsx` line-state rules → `settings.ts` helpers
  (`tierDots`, `highlightVars`, `fontStack`, `cjkStack`, …).
- `scripts/gen-gpui-settings.cjs` imports `src/settings.ts` → `src-gpui/src/settings_gen.rs` (struct,
  defaults, serde camelCase matching `snapshotSettingsJson()` in `src/windows/Versions.tsx`), plus
  `settings_items_gen.rs` from `items` (text byte for byte).
- Gate: all golden tests pass; re-running the codegen produces no diff. **Done 2026-11-20:** 49 Rust tests pass against goldens from the TS (uroman, romanize + pinyin-pro, lyrics, spicy, objpack, lyricAnimator + cubic-spline, settings, App.tsx line state moved to `src/lineState.ts`).

## Phase 2: harness + ledger
1. Move the fake state out of `scripts/snapshot/preload.cjs` into `fixtures/fake-state.json` (cover as a
   PNG).
2. Grow `SCENES` from 5 to ~40 core scenes + ~60 appearance-setting scenes (one per non-default value),
   defined in a data file.
3. `scripts/parity-diff.mjs`: pngjs + pixelmatch, thresholds from port.md §4, sung-line mask region.
4. Motion capture: Electron `--motion <transition>`, a frame every 33ms via `capturePage` on a fixed
   clock. Offscreen timing is not frame-exact, so fake `performance.now`/rAF in the preload and step a
   virtual clock in both builds.
5. Ledger: `scripts/ledger/build.mjs` parses CSS (postcss) and TSX conditionals (TS compiler API) →
   `parity/ledger.md` (~2–3k rows). Window behaviour and interaction rows are added by hand from
   `electron/main.cjs`.
6. Gate: every Electron scene is stable on two runs; ledger complete. **Done 2026-11-20:** 128 stills + 14 motions, 553/553 PNGs byte-identical over two runs (`node scripts/snapshot/stability.mjs`); ledger 3281 rows (`node scripts/ledger/build.mjs`). `window-behaviour?` rows are checked by Phase 3 asserts, not scenes.

## Phase 3: window shell
- `src-gpui` skeleton: gpui 0.2.2 pinned, depends on `playback-core`, `--fake-state` and `--snapshot`
  flags.
- Windows: frameless + transparent, DWM strip (7.5), cloak until stable (7.7), `screen-saver` topmost,
  `skipTaskbar`, settings placement rule, right-click close, drag regions, card-measure resize (7.8),
  cursor-poll hover (7.6), tray, single instance.
- The real frame driver, built from the spike.
- Gate: window-behaviour rows done (`GetWindowRect` asserts), fps log ≥55. **Done 2026-11-20
  (off-screen):** `--selftest-shell` 48/48, `--selftest-cards` 192/192, frame log min ≥95 fps, max gap
  ≤10.4 ms; ledger 151 rows done. Open: webPreferences rows (no Chromium, N/A?), `H.widget-no-throttle`,
  skipTaskbar (not provable off-screen), a real on-screen launch by the user, and H rows `user-data`,
  `fullscreen-enter`, `widget-initial`, `replay-state`, `external-links`, `audio-capture`, `routes`.
  Known deviations: fixed changelog-then-update order, no cross-window drag-stop height replay,
  approximate multi-DPI, no Setup timeout.

## Phase 4: CSS layer (`src-gpui/src/css/`)
One slice per helper (~12), each with a tiny Electron test page and a passing diff: tween + cubic-bezier,
keyframes, scale emulation (0.85 / 0.94 / 1.1), rounded-clip helper, box-shadow / hairline, filter bake,
text-shadow ring, gradient-text highlight (7.1), mask fade, radial bake, max-height clip, hover.

### Slice A: harness + motion helpers (done 2026-11-20)
Helpers in `src-gpui/src/css/`: `easing` (Chromium `gfx::CubicBezier`), `tween` (retarget from the current
value, reversing shortening factor on output progress, `active()`), `keyframes` (Web Animations timing,
per-stop easing, fill, alternate, delay), `clip_height` (max-height), `hover` (GPUI hover + cursor poll),
`translate` (added: taffy rounds relative offsets to whole px, Chromium does not snap transforms, so the
offset goes through `Window::with_element_offset`).
- Electron: `node scripts/primitives/run.mjs` (→ `parity/out/primitives/electron/`), `--golden` writes
  `src-gpui/src/css/golden/*.json`, `--stability` renders twice and compares bytes.
- GPUI: `node scripts/primitives/run.mjs --gpui` (`Playback.exe --primitive <case> <dir>` per case →
  `parity/out/primitives/gpui/`), `--gpui --stability` likewise.
- Diff: `node scripts/primitives/diff.mjs` (parity-diff thresholds with `edges: true`, port.md §4; diff
  images in `parity/out/primitives/diff/`).

Results: `cargo test` css 9/9 (goldens), both sides byte-stable (178 PNGs each), 10/10 cases pass with
no untolerated pixel. Tolerated edge pixels: corners ≤ 31/255 (tween-*, clip-*, hover-next-up,
kf-bar-pulse), translated edges ≤ 20/255 (kf-next-popup-in) and ≤ 52/255 (kf-vol-pill-in);
kf-empty-pulse and kf-fs-in max delta 1. `parity-diff.mjs --self-test` 19/19.

### Slice B: geometry helpers (done 2026-11-21, 20/23 pass + 3 accepted)
**User decision 2026-11-21:** `scale-pill-in` f001 (group opacity of overlapping pieces) and
`scale-pill-pop` (Chromium's smeared first frame, 1-2 rounded-end pixels at 65-73/255) are accepted
differences. `scale-pill-text`'s failure is DirectWrite measuring the text ~1px wider: it moves to
slice C (text), not a scale issue.

Helpers in `src-gpui/src/css/`: `scale` (a tree of `scale::Node`s laid out unscaled as GPUI divs, whose
bounds are recorded and painted by the helper at `centre + (bounds − centre) × s` in f32, with no second
layout pass; unrounded fills as pixel-aligned quads with alpha × covered area, rounded fills as GPUI
quads, shadows through `shadow`'s bakes, text shaped at the scaled size), `rounded` (`clip_child`: the
visible rect of a child of an `overflow: hidden` rounded box and the radii it carries; `clamp` = CSS
radius scaling), `shadow` (outer and inset `box-shadow` baked on the CPU as Skia draws it: area coverage,
sigma = blur / 2 as three box blurs, clipped to outside / inside the box; painted as a sprite), `mask`
(`mask-image: linear-gradient(to bottom, …)` as one copy per fading device row at the row's alpha).
Cases' `text` boxes ([x, y, w, h]) are diffed with the glyph AA tolerance (`--aa --text`).
- Same commands as slice A: `node scripts/primitives/run.mjs [--gpui] [--stability] [--only=…]`, then
  `node scripts/primitives/diff.mjs` (diff images in `parity/out/primitives/diff/`).

| case | source | frames | max delta | untolerated soft % | edge px (max) | result |
|---|---|---|---|---|---|---|
| shadow-widget | App.css:115-131 | 1 | 23 | 0 | 24 (23) | pass |
| shadow-preview | Settings.css:505-516 | 1 | 33 | 0 | 53 (33) | pass |
| shadow-vol-pill | App.css:999-1018 | 1 | 22 | 0 | 29 (22) | pass |
| shadow-hairline-pill | Settings.css:429-444, 472-476, 298-306 | 1 | 47 | 0 | 109 (47) | pass |
| rounded-widget | App.css:115-131, 138-148, 166-190, 205-210 | 1 | 54.6 | 0 | 90 (54.6) | pass |
| mask-lyrics | App.css:268-293 | 1 | 1.4 | 0 | 0 | pass |
| mask-lyrics-open | App.css:268-294 | 18 | 30 | 0 | 29 (30) | pass |
| scale-pill-085 | Settings.css:429-444, 482-485 | 1 | 26.4 | 0 | 168 (26.4) | pass |
| scale-pill-094 | Settings.css:461-464 | 1 | 24 | 0 | 36 (24) | pass |
| scale-pill-110 | Settings.css:487-491 | 1 | 45 | 0 | 240 (45) | pass |
| scale-pill-text | Settings.css:461-464, Settings.tsx:182 | 1 | 115 | 0.2765 | 282 (63) | FAIL (24 hard px) |
| scale-pill-in | Settings.css:429-444, 482-485 | 10 | 54 | 7.83 | 634 (54) | FAIL (f001 only, soft) |
| scale-pill-pop | Settings.css:472-476, 487-491 | 14 | 78 | 0.99 | 598 (64) | FAIL (7/14 frames) |

Shadow tails (below the box) match Electron within 1-2/255 alpha, which confirms sigma = blur / 2 with
Skia's three-box filter. The .94 stand-in's top row (y 24, edge at 24.42) is alpha 162 on both sides.
Both sides byte-stable (230 PNGs each). The 10 slice A cases still pass.

Why the remaining scale cases fail (no threshold changed):
1. scale-pill-text: the unscaled pill is 101 px wide in GPUI and 100 px in Electron (text measured ~1 px
   wider by DirectWrite). 15 of the 24 hard pixels are the pill's right end (x 115-119); the other 9 are
   inside the text box (glyph advances off by the same width).
2. scale-pill-in f001 (soft 7.83%, hard 0): CSS `opacity` fades the composited group, the helper fades
   each primitive, so where the stand-in overlaps the pill a partly faded frame differs (stand-in pixel
   Electron 247,245,123 / GPUI 230,228,139 RGBA). Every other frame passes.
3. scale-pill-pop: f001 (86 hard px) is Chromium's first compositor frame of the animation: the pill's
   coverage is smeared over rows 21-41 (GPUI 21-40; alpha area 1380 against 1415), as from a layer
   rasterized at another scale and resampled. f002-f006 and f010 each have 1-2 pixels on the rounded
   ends at 65-73/255 against the 64 edge limit (GPUI's SDF AA against Skia's at fractional positions).

## Phases 5–7: screens
Slices of 20–40 ledger rows, in port.md §5 order. Estimate: widget ~15 slices, settings ~15, other
windows ~6. Each slice: worker → review by a different worker (rerun diffs + 5 spot checks) → rows
marked `done`. User preview at the end of each phase.

## Phase 8: packaging
- GPUI stage in `scripts/pack.cjs` → `release/phoenix/` → `playback-gpui-setup-<v>.exe` +
  `playback-gpui-<v>-x64.nsis.7z`.
- Flavor-switch test: Tauri → GPUI → Tauri, settings file round-trips.
- Restore the Phoenix lines in `CLAUDE.md` when work starts (`src-gpui/target` and `src-gpui/*/target` are back in `.gitignore`).

## Decisions (user, 2026-11-20)
1. **Text antialiasing:** diff with an edge tolerance. GPUI uses DirectWrite and Electron uses Skia, so
   glyph edges differ; `parity-diff.mjs` blurs both images slightly (or ignores pixels on glyph edges)
   before comparing text regions. Layout, colour and weight differences must still fail.
2. **Spicy key connect:** embed `wry` for that one window in `src-gpui`, matching `spicy/connect.rs`
   (own profile, no capabilities, toast, clipboard watch, auto-fill).
3. **Workers:** Sonnet subagents approved, as port.md §6 describes.

## Scale
~50–60 worker slices plus reviews: many sessions. Start: Phase 0 together with the frame-driver spike.
