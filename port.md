# GPUI port (Project Phoenix): instructions

This file is the full brief for porting Playback to native GPUI (Rust, no web view). It is written for a
**conductor** (Opus) that plans, splits and verifies the work, and **workers** (Sonnet subagents) that
each do one slice of it.

The goal is **1:1 parity with the Electron build**. "Parity" means the same pixels, the same motion, the
same behaviour, and the same settings with the same effects. "Looks about right" does not count. Every
difference is a bug until the user explicitly accepts it.

---

## 0. History: read this first

- A first GPUI attempt lived in `src-gpui/` (gpui 0.2.2, staged to `release/phoenix/Playback.exe`). It
  was **deleted on 2026-11-20**. Its last committed state is in git history (commits `9c55e58`,
  `f40cad0`, `0163b39`); look at it for reference only and do not resurrect it.
- **Why it failed:**
  1. It was a **rewrite that imitated the look**, not a port. Screens were rebuilt from screenshots and
     memory, not translated from `App.tsx`/`App.css`. Every screen drifted (settings buttons, fullscreen,
     album mode).
  2. There was **no animation layer**. CSS transitions, keyframes and springs were either missing or
     faked one at a time, so the motion felt wrong everywhere: lyrics opening, controls on hover,
     settings opening, the reset pill.
  3. **Verification was done by eye** on a few screenshots, so small differences piled up unnoticed.
  4. **Backend code was copied** from `src-tauri` (`bridge.rs`, `media.rs`) instead of shared, so it
     drifted too.
- This brief fixes each of those. Do not skip the phases: the earlier attempt went straight to screens,
  and that is exactly what failed.

## 1. Sources of truth

Electron is the oracle. When the GPUI build and Electron disagree, Electron is right.

| What | Where |
|---|---|
| Widget markup and behaviour | `src/App.tsx` (~2800 lines) |
| Widget styles | `src/App.css` (~3100 lines) |
| Settings | `src/windows/Settings.tsx`, `src/windows/Settings.css` |
| Other windows | `src/windows/{Changelog,Setup,Update,Versions}.tsx` + their `.css` |
| Settings list (defaults, storage keys) | `src/settings.ts` (`DEFAULTS`, `KEYS`, `tierDots`, …) |
| Lyrics pipeline | `src/lyrics/{lyrics,spicy,romanize,uroman}.ts` |
| Lyric motion (springs, per-letter fill) | `src/lyrics/lyricAnimator.ts` |
| Now playing, audio bars | `src/hooks/{useNowPlaying,useAudioBars}.ts` |
| Window behaviour (sizes, positions, always-on-top level, right-click, drag, resize IPC) | `electron/main.cjs`, `electron/preload.cjs` |
| Preload API contract | `src/types/player.d.ts` |
| Backend (Rust, already written) | `crates/playback-core/src/{media,bridge,lyrics,spicy,art,spotify,spicetify,updates,support,install,migrate,fonts}` (shared since Phase 0; `src-tauri` keeps only the glue) |
| Reference images | `scripts/snapshot/` (`npm run build && npm run snapshot`) → `snapshots/electron/` |
| Fonts | `src/fonts/spicy-lyrics/` (lyrics); SF Pro Text (UI) |

The Tauri build (`src-tauri/`) runs the same React bundle, so it already has parity for free. Use it as a
second reference for backend behaviour.

## 2. Mandatory architecture decisions

### 2.1 Share the backend; never copy it
**Phase 0:** move the backend modules out of `src-tauri/src/` into a library crate, e.g.
`crates/playback-core`. That covers media/SMTC, volume, bridge server, lyrics providers, spicy, art,
spotify token, spicetify setup, updates feed/installer and support/paths.

- Make `src-tauri` and the new `src-gpui` both depend on it, in a Cargo workspace at the repo root or
  through path dependencies.
- Only the Tauri command and event glue stays in `src-tauri`.
- This step changes Tauri. Run `npm test` and preview the Tauri build before going further; Tauri must
  behave exactly as before.

### 2.2 Port the TypeScript logic as logic, with golden tests
`lyrics.ts`, `spicy.ts`, `romanize.ts`, `uroman.ts`, `lyricAnimator.ts`, the line-state rules in
`App.tsx`, `tierDots` and the other helpers in `settings.ts` become pure Rust functions. They are
translated **function for function, in the same order and with the same names** (snake_case).

- Write a small Node script under `scripts/golden/` that runs the TypeScript versions on fixtures (real
  Spicy, NetEase and LRCLIB payloads, plus edge cases: gaps, backing vocals, CJK, romanize modes, empty
  lines) and dumps JSON.
- Rust tests load that JSON and must match it exactly. Springs must match too: sample them at fixed
  `dt` steps.
- A worker may not "simplify" an algorithm. If the TS has a quirk, port the quirk.

### 2.3 Generate the settings, never hand-write them
Generate the Rust settings struct from `src/settings.ts` (`DEFAULTS` + `KEYS`) with a script, e.g.
`scripts/gen-gpui-settings.cjs` → `src-gpui/src/settings_gen.rs`, and check in the generated file.

- Storage is `%APPDATA%\Playback\gpui-settings.json`: a camelCase object with exactly the shape
  `snapshotSettingsJson()` in `src/windows/Versions.tsx` writes. Version control writes this file when
  someone switches flavor (`crates/playback-core/src/updates/mod.rs` `bump`), so settings carry over.
- The Settings window rows (labels, explanations, chips, ranges, units, hint texts) are also extracted
  from `src/windows/Settings.tsx` `items`. Copy their text byte for byte, including `·` and `…`.

### 2.4 Build the CSS layer (`src-gpui/src/css/`) before any screen
GPUI has no CSS. Every CSS feature the app uses needs one shared Rust implementation. Each piece is
verified against a tiny Electron test page before any screen uses it. Required pieces:

| CSS feature | GPUI implementation |
|---|---|
| `transition` (any property, any duration, `ease`/`linear`/`cubic-bezier`) | `Tween` that retargets from its current value (CSS behaviour on reversal). Exact cubic-bezier solver. Named curves: `ease` = (.25,.1,.25,1), `ease-in-out`, standard (.4,0,.2,1), the springy (.3,1.5,.5,1) and (.3,1.6,.5,1) |
| `@keyframes` + `animation` (`both`, `forwards`, `infinite alternate`) | `Keyframes` with per-stop easing |
| Springs (`lyricAnimator.ts`, spr.lua underdamped) | ported 1:1 (2.2) |
| `scale` / `transform: scale()` on boxes | GPUI divs have no transform. Emulate by scaling size, padding, font size and radius around the box centre, inside a wrapper with the unscaled size so layout does not move. Verify against Electron at 0.85, 0.94 and 1.1 |
| `translate` | relative offsets |
| `opacity` | `.opacity()`; it applies to text too |
| Rounded clipping (`overflow: hidden` + `border-radius`) | GPUI clips to **rectangles only**. Any child that touches a rounded edge must carry its own radii (fills, images, backdrops). Write one helper and use it everywhere |
| `box-shadow` (outer, inset hairlines) | `BoxShadow`; inset 0.5px edges via the hairline border helper. Check that rounded hairlines are not jagged |
| `filter: blur/saturate/brightness` on the cover | CPU bake (`image` crate) once per track and size; gaussian sigma = CSS px scaled to the bake size |
| `text-shadow` (glow, blur halo) | ring of offset copies; tune per-copy alpha against Electron pixels, never by eye |
| `background-clip: text` + gradient (lyric sweep) | per-character `HighlightStyle` (see gotcha 7.1) |
| `mask-image` gradients (lyrics panel bottom fade) | per-row alpha from the row's y |
| `radial-gradient` | CPU bake |
| `max-height` transitions (lyrics panel, controls) | a clip wrapper whose height is tweened, with content laid out at full size |
| `:hover`, `.hovered` | GPUI hover plus a cursor poll (7.6) |
| `ResizeObserver` → window resize IPC | measure the card (`on_children_prepainted`) and resize the window (7.8) |

### 2.5 Frame driver
One driver per window: while any tween, keyframe or spring is active, frames must arrive at display
rate.

The last attempt **got only about 2 frames during a tween, then nothing until a 250ms idle timer**
(`request_animation_frame()` was called from `render`, and the window was resized during the tween).
Root cause not found. **Fix this first** and prove it with a frame-timestamp log: ≥55 frames/s for the
whole of a 350ms tween, with and without a window resize in flight.

## 3. The parity ledger (the conductor owns it)

Before any UI work, the conductor builds `parity/ledger.md`: one row per unit of behaviour.

- **Every CSS rule** in `App.css`, `Settings.css`, `Changelog.css`, `Setup.css` (and the CSS of the
  Update and Versions windows): selector + property, with `file:line`.
- **Every element and conditional** in each `.tsx` render tree.
- **Every setting**, and what it changes.
- **Every window behaviour** in `electron/main.cjs`: size, position rule (settings opens beside the
  widget, see `x = right + SETTINGS_W <= …`), always-on-top level (`"screen-saver"`), `skipTaskbar`,
  `show: false` + `ready-to-show`, right-click closes, drag regions, resize IPC, fullscreen, album
  mode, tray, launch flags.
- **Every user interaction**: click, double-click, right-click, wheel, drag, keyboard (Ctrl+F, Esc),
  hold-to-reset (arm → hold 800ms → done, release drains, pointer-leave cancels).

Row format:

```
| id | source (file:line) | what | gpui (file:line) | status | evidence |
```

`status` is one of: todo / done / accepted-difference. `evidence` is the path to a diff image or test
that passed.

Only the user can set `accepted-difference`. A worker who cannot match a row reports it to the
conductor, and the conductor asks the user. Nobody silently approximates.

## 4. Verification harness (build it before the screens)

1. **Fake state shared by both builds.** Move the fake player state in `scripts/snapshot/preload.cjs`
   (paused song, fixed position, word-timed lyrics, generated cover) into a JSON fixture. Electron
   keeps reading it through the preload. GPUI reads the same file with a `--fake-state <file>` flag
   that replaces SMTC, the bridge and the lyrics fetch.
2. **More scenes.** Extend `SCENES` in `scripts/snapshot/main.cjs`:
   - widget idle, widget hovered, widget with lyrics open
   - every settings category
   - search results
   - armed, holding (at 50%) and done reset pill
   - token entry open
   - fullscreen, album mode
   - changelog, past changelog, setup, update prompt, Version control
   - every non-default value of each appearance setting (one scene per setting)
3. **GPUI capture.** A `--snapshot <scene> <out.png>` mode renders the same scene and saves the
   window's pixels (`PrintWindow` / `BitBlt` of its HWND). It also covers what lies outside the card,
   so the transparent stage is compared too.
4. **Diff.** A script (`scripts/parity-diff.mjs`, using pngjs + pixelmatch) writes a diff image and
   fails when:
   - more than **0.1%** of pixels differ by more than **8/255**, or
   - any pixel differs by more than **24/255**.

   Exception: the sung line, where the snapshot header already allows ~20/255 for spring settling.
   Tighten these numbers if both builds turn out more stable.
5. **Motion.** For each transition (hover in/out, lyrics open/close, settings open, reset pill
   arm/hold/release/done, sweep, scroll follow, next-song popup, fullscreen enter):
   - capture frames at **t = 0, 33, 66, … ms** after the trigger in both builds;
   - compare each pair with the diff above;
   - also compare the key numbers (heights, opacities) from a debug overlay or log.
6. **Behaviour.** Script the interactions from §3. Assert window rects (`GetWindowRect`, after taking
   off the 8px invisible frame, see 7.5), focus, z-order and settings file contents.

A ledger row becomes `done` only with evidence from this harness.

## 5. Phases

| Phase | Work | Exit gate |
|---|---|---|
| 0 | Backend crate extraction (2.1); Tauri unchanged | `npm test` + Tauri preview approved by the user |
| 1 | Golden tests + logic port (2.2); settings codegen (2.3) | all golden tests pass |
| 2 | Harness (§4) + ledger (§3) | Electron scenes stable; ledger complete |
| 3 | Window shell: frameless transparent windows, DWM strip, cloak, topmost, sizing, frame driver (2.5) | window-behaviour rows done; ≥55 fps proof |
| 4 | CSS layer (2.4), each piece against a test page | primitive diffs pass |
| 5 | Widget: idle → hover → lyrics → scrubber/times → controls → next-song → album mode → fullscreen | each scene + motion diff passes |
| 6 | Settings: shell → nav → preview → each category's rows → search → reset flows → token entry | same |
| 7 | Changelog, past changelog, setup, update, Version control | same |
| 8 | Packaging and flavor integration (§8) | installer offers GPUI; switching flavors keeps settings |

Per CLAUDE.md the order is still **Electron → Tauri → GPUI**. A UI change after this port lands first in
Electron, gets confirmed, then goes to Tauri and then GPUI, and the ledger rows are updated each time.

## 6. How the conductor runs the workers

- **Slices.** One worker task = a small set of ledger rows from one area: about 20–40 CSS rules or one
  component. Never "port the settings window".
- **The prompt for every worker includes:**
  - the exact ledger rows (ids, `file:line` sources);
  - the source excerpts themselves;
  - the CSS layer helpers they must use;
  - §7 of this file;
  - the scene names to verify with.
- **Worker rules (put these in every prompt verbatim):**
  1. Translate, don't redesign. Every value (px, alpha, colour, duration, curve, font size and weight,
     line-height, gap, radius) comes from the cited source line. Write the source `file:line` in a
     comment above each block.
  2. No new behaviour, no "improvements", no leftover TODOs.
  3. If a CSS feature has no helper in `css/`, stop and report it. Do not hand-roll a one-off.
  4. Run the parity diff for your scenes and attach the diff image paths. Report failures honestly,
     with numbers.
  5. Do not touch files outside your slice. Do not commit, push or release.
- **Review.** The conductor, or a fresh worker that did not write the code, checks each slice:
  - rerun the diffs;
  - read the code against the cited lines;
  - spot-check 5 random values.

  Only then does the conductor mark the rows `done`.
- **Preview.** At the end of each phase, show the user the running GPUI build next to Electron.
  Packaging and release still need their approval.

## 7. GPUI 0.2.2 gotchas the first attempt hit

1. **Highlight colours blend.** `TextStyle::highlight` does `color = base.blend(highlight)`, and
   `blend` keeps the **base alpha**. A highlight with alpha < 1 over a dim base barely shows, and a
   transparent base makes it invisible.

   Fix: pass the colour at alpha 1 (which replaces the base) and put the real alpha in `fade_out`:
   ```rust
   fn paint(c: Hsla) -> HighlightStyle {
       HighlightStyle { color: Some(Hsla { a: 1.0, ..c }), fade_out: Some(1.0 - c.a), ..Default::default() }
   }
   ```
2. `on_children_prepainted` exists only on `Div`. Call it **before** `.id(...)`, which turns the div
   into `Stateful<Div>`.
3. Clipping is rectangular (see 2.4).
4. Divs have no transform; `svg()` has `with_transformation` (see 2.4 for scale).
5. **Windows 11 chrome.** `titlebar: None` still leaves a DWM border, rounded corners and a shadow.
   Turn them off with `DwmSetWindowAttribute`: `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND`,
   `DWMWA_BORDER_COLOR = 0xFFFFFFFE`, `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED`.

   The window still has an **invisible 8px resize frame** on each side, so `GetWindowRect` is 16px
   wider and taller than the client area. Account for it when you position windows and compare rects.
6. **Hover gets stuck.** GPUI misses the leave event when the cursor leaves a transparent window
   quickly. Each frame, check `GetCursorPos` against `GetWindowRect` and clear hover when the cursor is
   outside.
7. **Hidden windows never paint.** With `show: false`, the window gets no `WM_PAINT`, so it never lays
   out. For Electron's `show: false` + `ready-to-show`, open it shown but **cloaked**
   (`DWMWA_CLOAK = 1`), call `activate_window()`, and uncloak once the card has kept the same size for
   two frames.
8. **Window sizing.** The card is measured during prepaint, so a resize lags one frame. During a
   height tween, grow the window once to the final height and shrink it only after the tween ends.
   The first attempt still saw 184 → 148 → 173 jumps on hover; that is linked to the frame stall in
   2.5.
9. **Mouse events bubble.** Right-click on the cover and on the card both opened settings, so it
   opened twice. Call `cx.stop_propagation()` in inner handlers.
10. **SVG colour.** `svg().text_color(c)` colours the whole icon. CSS icons inherit `currentColor` and
    sit inside an opacity wrapper; reproduce both. For example, a nav icon is the item colour (.7, or 1
    when hovered or active) × .78 opacity.
11. **Native button centring.** Text in a `<button>` is centred however much flex stretches the
    button; a GPUI div is not. Pills need `flex().items_center().justify_center()`.
12. **Line height.** GPUI's default line-height is 1.618. CSS `normal` for SF Pro is about 1.19. Set it
    explicitly on every text root.
13. **Image format.** SMTC thumbnails and `RenderImage` frames are BGRA on Windows; convert once.
14. **Cover glow bake (worked in the first attempt; reuse).** `.widget-bg`/`.st-bg` use
    `inset: -60px`, `.st-preview-bg` uses -40px. Resize the cover to 192², apply CSS `saturate(s)` with
    the W3C matrix (`0.213/0.715/0.072` rows) then `brightness(b)`, gaussian blur with
    sigma = `blur_px * 192 / span_px`, crop only the part under the card, and draw it with the card's
    radii. `.st-bg-mono` (160deg gradient #1d1d21→#0b0b0d plus two white radial glows, a .14 and .07,
    falling to transparent at 70%) is baked at half size.
15. **Debug builds.** The running debug exe locks `target\debug\playback.exe`. Stop it by its **path**
    before `cargo build`. Never stop processes by name: the installed Tauri app is also `playback.exe`.

## 8. Packaging and flavor integration (already wired)

- **Flavors already know GPUI.** The installer (`installer/index.html`, `installer/src/main.ts`,
  `installer/src-tauri/src/main.rs`), Version control (`src/windows/Versions.tsx`) and the Tauri updater
  (`crates/playback-core/src/updates/{feed,mod}.rs`) all list `"gpui"`. A release offers it when it carries:
  - `playback-gpui-setup-<version>.exe`
  - `playback-gpui-<version>-x64.nsis.7z`
- **Staging.** Stage the exe to `release/phoenix/Playback.exe`. Add a GPUI stage to `scripts/pack.cjs`
  that builds `cargo build --release` in `src-gpui` and makes the nsis-web pair, the same way `eagle/`
  is staged for Tauri.
- **Unchanged identifiers.** Keep the same `appId` (`com.player.smtcwidget`), install folder and data
  folder (`%APPDATA%\Playback`). Settings live in `gpui-settings.json` (2.3).
- **Spicetify.** The bridge is `spicetify/player-bridge.js` on 127.0.0.1:8899, served by the shared
  backend crate. Never restart Spotify.
- **Repo docs.** Bring back `src-gpui/target` in `.gitignore` and the Phoenix lines in `CLAUDE.md` when
  work starts again.

## 9. Definition of done

- Every ledger row is `done`, or `accepted-difference` with the user's OK recorded in the row.
- All scenes and motion sequences pass the diff thresholds in §4.
- Golden tests pass; `npm test` passes; the Tauri build is unchanged.
- The user has previewed GPUI side by side with Electron and approved it.
- Only after that: package and release, following the CLAUDE.md release steps.
- The GPUI app has 100% 1:1 parity with the Electron app.
