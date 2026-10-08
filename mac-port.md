# Mac port plan

Goal: player on macOS, behaving like the Windows version as far as macOS
allows, without paying for an Apple Developer account. Started 2026-10-08.

## Constraints and decisions (agreed with the user)

- **No Mac here.** The user's friend has one and tests occasionally; builds
  are made **on that Mac** (`npm run package:mac`, still to write), not in
  GitHub Actions. Work in steps big enough to be worth a test, and give the
  friend a short checklist each time.
- **No $99 Apple account.** Instead:
  - sign every build with one **self-made certificate** (made once in Keychain
    Access on the friend's Mac), so macOS keeps granted permissions across
    updates. Unverified: confirm on the Mac.
  - install with a **Terminal one-liner (curl)**, since curl downloads are not
    quarantined and so skip the "Apple can't check this app" block.
    Unverified. Problem: the repo is **private**, and curl can't fetch private
    release files without a token. Ask the user: public repo, a separate
    public repo for downloads, or `gh release download`.
- **What's playing:** Spotify and Apple Music through AppleScript (supported,
  stable). Not the private MediaRemote framework: since macOS 15.4 it returns
  nothing for third-party apps, and the perl-based workaround
  (mediaremote-adapter) could break on any update. Maybe later, as an extra.
- **Browsers later** through a small browser extension that reports
  `navigator.mediaSession` to the loopback server, like the Spicetify bridge
  does. VLC through its built-in HTTP interface. Not in the first version.
- **Visualiser:** system audio on macOS needs Screen Recording permission.
  The user accepted **fake bars** (animated from the playing state) instead.
- **Known gaps on Mac:** volume only for Spotify/Music (no per-app volume on
  macOS), Up next only for Spotify (Music hides its queue).

## Done (groundwork, Windows unchanged)

- `electron/mediaHelper.cjs`: `helperCommand()` picks the now-playing helper
  per platform; `main.cjs` uses it. Windows = the same `smtc.ps1` command as
  before. Returns `null` on macOS for now (main logs and skips). Tests:
  `tests/mediaHelper.test.cjs`.
- `electron/spicetifySetup.cjs`: `defaultPaths(env, platform)` has macOS
  paths: CLI `~/.spicetify/spicetify` (or PATH), config
  `$XDG_CONFIG_HOME|~/.config/spicetify`, Spotify `~/Applications` or
  `/Applications/Spotify.app`, applied extensions in
  `Spotify.app/Contents/Resources/Apps/xpui/extensions`. From Spicetify's
  docs and install.sh; **unverified on a real Mac**. Tests in
  `tests/spicetifySetup.test.cjs`.

## Phase 1: the Mac now-playing helper

A small Swift command-line program, `electron/mac/media-helper.swift`, speaking
exactly the smtc.ps1 protocol (see `electron/mediaHelper.cjs` and `SmtcState`
in `src/player.d.ts`): JSON lines out, one command per line in.

- Find the source **without launching it**: only talk to Spotify
  (`com.spotify.client`) or Music (`com.apple.Music`) when it is in
  `NSWorkspace.shared.runningApplications`. A plain `tell application` would
  start the app.
- Prefer the one that is playing; otherwise the last one that played.
- Read through `NSAppleScript` (or ScriptingBridge): name, artist, album,
  duration (Spotify gives ms, Music seconds), player position, player state,
  shuffling, repeating, sound volume; Spotify also `id` (`spotify:track:…`)
  and `artwork url`.
- Push, not just poll: listen for the distributed notifications
  `com.spotify.client.PlaybackStateChanged` and `com.apple.Music.playerInfo`;
  poll position about once a second while playing, plus a 5 s heartbeat like
  smtc.ps1.
- Commands: `playpause`, `play`, `pause`, `next track`, `previous track`,
  `set player position to <sec>`, `set sound volume to <0..100>`,
  `set shuffling to …`, `set repeating to …` (Music: `song repeat`).
- `appId`: `spotify.exe` for Spotify, so the renderer's existing Spotify
  checks keep working (grep `spotify.exe` in `src/`); pick a value for Music.
  `caps`: all true for both.
- Build: `npm run build:mac-helper` = `swiftc -O` for arm64 and x86_64, `lipo`
  into one universal binary at `electron/mac/media-helper`; add it to
  `build.asarUnpack`. Needs Xcode Command Line Tools on the Mac.
- `helperCommand()` on darwin: that binary (the unpacked copy when packaged),
  no args. Update `tests/mediaHelper.test.cjs`.
- Info.plist `NSAppleEventsUsageDescription` ("player reads what Spotify and
  Music are playing and controls them"). Without it macOS refuses the Apple
  Events silently.

## Phase 2: the app on macOS

- Window: `win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true })`
  so it shows on every Space and over full-screen apps; `app.dock.hide()` (no
  Dock icon, like a widget); keep the "screen-saver" level. Check dragging
  (`startDrag` follows `screen.getCursorScreenPoint`), transparency, shadow
  (`hasShadow: false` matters more on Mac).
- `listFonts()` is PowerShell-only: on macOS use
  `osascript -l JavaScript -e "ObjC.import('AppKit'); $.NSFontManager.sharedFontManager.availableFontFamilies.js.map(f => f.js).join('\n')"`.
- Visualiser: skip the loopback capture on macOS
  (`setDisplayMediaRequestHandler`, `src/useAudioBars.ts`) and drive fake bars
  from the playing state.
- Setup window (`src/Setup.tsx`): on macOS show Spicetify's Mac install command
  (`curl -fsSL https://raw.githubusercontent.com/spicetify/cli/main/install.sh | sh`),
  no Microsoft Store line, and a note that macOS may ask to let player change
  Spotify (App Management) when applying. Pass the platform from main
  (`setupState` can carry it).
- Anything else using `powershell.exe`, `windowsHide`, `%APPDATA%` or `.exe`
  names: grep `electron/` for them.

## Phase 3: packaging and install

- electron-builder `mac` target: `zip` (and maybe `dmg`), `arch: universal`,
  `hardenedRuntime: false` at first, `identity` = the self-made certificate's
  name, `extendInfo` with `NSAppleEventsUsageDescription`.
- `npm run package:mac` = build + mac helper + electron-builder.
- `install.sh`: download the zip, unzip into `~/Applications` (no admin),
  launch. Depends on the private-repo decision above.
- Add Mac release steps to CLAUDE.md (attach the Mac zip to the same GitHub
  release).

## Phase 4: later

Browser extension, VLC, maybe mediaremote-adapter for every other app.

## Checklist for the friend's Mac

Record the macOS version and chip (Apple Silicon or Intel) each time.

1. `spicetify path all` and `ls ~/.config/spicetify ~/.spicetify`: do they
   match the paths above?
2. Helper alone: `./electron/mac/media-helper` in Terminal while Spotify plays.
   JSON lines appear? Type `playpause` + Enter: does it pause? Same for Music.
   Was there an Automation prompt?
3. With Spotify **closed**, run the helper: Spotify must **not** start.
4. The app: shows on every Space and over a full-screen app? No Dock icon?
   Dragging, lyrics, Settings, fonts list.
5. Setup: Spicetify detected, Apply now works (App Management prompt?), the
   bridge sends the token (Spicy Lyrics word timing appears).
6. Signing: install build A, allow the prompts, install build B signed with
   the same certificate. Prompts again? (Tests the free-certificate trick.)
7. Install through curl: is there any "Apple can't check" block?
