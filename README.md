# player

A small floating music widget for Windows. It shows what's playing, lets you
control it, and scrolls the lyrics along with the song, highlighting each word
as it's sung.

It works with Spotify, Apple Music, your browser, VLC and anything else that
shows up in Windows' own media controls. The music keeps playing from your app
as usual; player just sits on top and follows along.

## What you get

- **Now playing at a glance:** cover, title, artist, a progress bar you can drag
  to seek, and play/pause, next, previous, shuffle, repeat and volume.
- **Synced lyrics** that follow the song, word by word when the source has that
  timing. Click a line to jump to it.
- **Romanized lyrics** for Japanese, Korean and Chinese songs, with one click.
- **Album mode:** click the cover to turn the widget into a big cover with the
  current line on top.
- **Up next:** with Spotify, see the next song in the corner and click it to skip.
- **Make it yours:** fonts, colours, glass, sizes, what's shown and more, with a
  live preview and search in Settings.

## Install

1. Run the installer, `player-setup-<version>.exe`. It installs for you only
   (no admin needed) and adds desktop and Start menu shortcuts.
2. player opens with a **Setup** window the first time. It's a checklist
   (see below). Lyrics already work with any player, so you can close it if you
   don't use Spotify.
   Its first step, **Spicy Lyrics key**, gets you your own free key for
   word-by-word lyrics: click **Get key**, sign in, create an application (name
   and project are filled in) and copy the key. player picks it up by itself.
   You can also get or paste one later in Settings → Behavior.
3. Start some music and it shows up in the widget. Drag the widget anywhere.

Windows 10 or 11 is required.

## Getting the most out of Spotify

player works with any music app, but with Spotify it can do more: Spotify's own
track and queue (for the right cover and **Up next**), and **word-by-word
lyrics** from Spicy Lyrics.

For that it uses [Spicetify](https://spicetify.app), a popular free tool that
adds extensions to the Spotify desktop app. The Setup window walks you through
it:

| Step | What to do |
| --- | --- |
| **Spotify** | Use the desktop app from [spotify.com](https://www.spotify.com/download/windows/). The Microsoft Store version can't run Spicetify. |
| **Spicetify** | If it's missing, Setup shows the install command. Click **Copy**, paste it into PowerShell, follow its prompts, then click **Check again**. |
| **Bridge** | player's small helper inside Spotify. Setup puts it in place for you. Spotify has to restart once to load it; on the first launch that happens on its own, otherwise click **Apply now (restarts Spotify)**. |

After that there's nothing to do: player keeps the bridge up to date, and Setup
only comes back if something needs you.

## Using it

- **Hover** the widget to show the controls.
- **Lyrics button** opens and closes the lyrics.
- **Click a lyric line** to jump there in the song.
- **Click the cover** for album mode; the back arrow returns.
- **Right-click** the widget to open or close **Settings**.
- **Click the time on the right** to switch between time left and song length.
- **Click the lyrics source** (under the progress bar) to try another lyrics
  provider for this song.

### Settings

Settings has a search box and these sections: Lyrics, Languages, Text,
Highlight, Surface, Layout, Album mode, Fun and Behavior. Every section has its
own Reset. Useful ones:

- **Behavior:** always on top, hide when nothing plays, open with lyrics shown,
  your Spicy Lyrics key for word-by-word lyrics, and the Spotify token used when
  there is no key (Spicetify sends it by itself; you can also paste one).
- **Lyrics:** pick a provider, or shift the timing if lyrics run early or late
  (for all songs or just this one).

**What's new** in Settings shows the changelog. It also opens by itself after an
update (you can turn that off in Behavior).

## Troubleshooting

**No lyrics for a song.** Not every song has them. Click the lyrics source to
try another provider, or choose one in Settings → Lyrics.

**Lyrics are early or late.** Settings → Lyrics → Offset, for this song or all
songs.

**No word-by-word lyrics or Up next with Spotify.** Make sure Spicetify is
installed, then restart player: Setup opens by itself if the bridge still needs
applying. If Spotify updated itself, Spicetify needs re-applying: run
`spicetify backup apply` in PowerShell.

**The widget is gone.** If "Hide when nothing plays" is on, it comes back when
music starts. Otherwise open player from the Start menu; only one copy runs at a
time.

**Buttons are greyed out.** The music app doesn't support that control (some
apps can't seek or change volume from outside).

## Uninstall

Settings → Behavior → **Uninstall player**: click it, then hold it to confirm.
It removes player, its settings and saved lyrics, and takes the bridge out of
Spotify (Spotify restarts once).

Or: Windows Settings → Apps → player → Uninstall. Your settings stay in
`%APPDATA%\player`; delete that folder too for a clean removal. To take the
bridge out of Spotify, run `spicetify config extensions player-bridge.js-`
and then `spicetify apply`.

---

## For developers

```bash
npm install
npm run app      # dev: Vite dev server + Electron with HMR
npm start        # production build, then Electron
npm test         # unit tests
npm run package  # builds release/player-setup-<version>.exe
```

Windows only. Project rules (release steps, settings conventions, the Spicy
Lyrics reference copy) are in [CLAUDE.md](CLAUDE.md).

### How it works

```
Spotify / Apple Music / …
        │  (Windows SMTC)
        ▼
electron/smtc.ps1 ──JSON on stdout──▶ electron/main.cjs ──IPC──▶ React renderer
        ▲                                     │
        └──── commands on stdin ──────────────┘
```

**`electron/smtc.ps1`** is a long-lived Windows PowerShell 5.1 process that
polls `GlobalSystemMediaTransportControlsSessionManager` every 250 ms. It emits
a JSON line whenever the track, playback status, or position meaningfully
changes, plus a 5 s heartbeat, and accepts one command per line on stdin
(`play`, `pause`, `playpause`, `next`, `prev`, `seek <sec>`, `volume <0..1>`,
`shuffle`, `repeat`).

Notes for anyone editing that script:

- It must run under **Windows PowerShell 5.1** (`powershell.exe`), not
  PowerShell 7 — .NET Core has no WinRT projection.
- It reads the raw stdin handle rather than `[Console]::In`, which is already
  at EOF under `-NonInteractive`.
- Position is reported alongside `updatedAt` (Windows' `LastUpdatedTime`).
  Spotify only refreshes its timeline every few seconds, so `position` on its
  own goes stale; the renderer extrapolates from `updatedAt` to get a smooth
  clock, and the script does the same correction to distinguish a real seek
  from ordinary staleness.
- Per-app volume comes from Core Audio (`IAudioSessionManager2`) via inline
  C#, because SMTC has no volume channel. Apps like Spotify run several
  processes, so every candidate PID is probed for an audio session.

**`electron/main.cjs`** owns the windows, supervises the PowerShell process
(restarting it if it dies), and performs all network requests — the renderer
would otherwise be blocked by CORS.

**The Spicetify bridge** (`spicetify/player-bridge.js`) runs inside Spotify and
posts the Spotify token, the playing track's id and queue, and lyrics Spicy
Lyrics already fetched to a loopback server (`electron/tokenBridge.cjs`).
`electron/spicetifySetup.cjs` installs, registers, migrates and applies it; the
Setup window (`src/Setup.tsx`) is its front end.

**Lyrics** are looked up in order: Spicy Lyrics (when a Spotify token is
available), LrcLib (exact match, then fuzzy search), NetEase, then Genius if a
key is provided. Word-level timing is used when available; plain lyrics are
spread evenly across the track duration. Titles are normalised first, since
Spotify reports things like `Song (feat. X) - Remastered` that lyric databases
do not carry.

**Album art** is fetched from the iTunes Search API, falling back to Deezer.
The SMTC thumbnail is deliberately not used: PowerShell's WinRT projection
breaks on `IAsyncOperation<interface>`, so `OpenReadAsync` returns an
unusable `__ComObject` — and the online sources give 600 px+ artwork anyway.

### Spicy Lyrics

Syllable-timed lyrics with built-in transliteration, tried before the
LrcLib/NetEase chain and silently skipped whenever it can't serve a track. It is
keyed by **Spotify track id** and needs a **real user token**, neither of which
SMTC provides. Both normally come from the Spicetify bridge; a token can also be
pasted in Settings → Behavior.

There is also an optional Spotify login, which resolves tracks through the
Search API:

1. Create an app at <https://developer.spotify.com/dashboard>.
2. Add `http://127.0.0.1:8888/callback` as a Redirect URI.
3. Provide the client id, either way:
   - `SPOTIFY_CLIENT_ID=<id>` in the environment, or
   - `{"clientId":"<id>"}` in `spotify.json` inside the app's userData folder.
4. Start the app and use the sign-in link that appears where lyrics would be.

Tokens land in `spotify-tokens.json` under userData (mode 600) and refresh
automatically. The consent page opens in your real browser, never an embedded
window.

Notes on the API contract, taken from `Query.ts` in the spicy-lyrics source,
since the usual prose description gets two details wrong:

- the token goes in a **`SpicyLyrics-WebAuth: Bearer <token>`** header, *not*
  `Authorization`
- the client does **not** send `operationId`; the server assigns it and it is
  read back as the string `"0"`

`httpStatus` 200 = found, 404 = none, 503 = queued server-side. Times in the
payload are already in **seconds**, matching `LyricLine.t`.

The player discovers the current request version through `ext_version` and
checks it again on the next lookup after 15 minutes. A line-only or untimed
response also triggers a version check, with one retry if the version changed.
Cached line lyrics record their request version and refresh when it changes;
older cache entries refresh automatically. Cached lyrics remain available if
authentication or the network fails, and syllable lyrics can be served offline.

### Licensing

`src/vendor/objpack.ts` is copied from spicy-lyrics, which is **AGPL-3.0**. That
is fine for private personal use, but if this app is ever distributed or made
reachable by other people over a network, AGPL section 13 requires offering the
complete source of the combined work under AGPL.

### Genius (optional)

Genius is only consulted if you supply a key (free at
<https://genius.com/api-clients>). Create `.env`:

```
VITE_GENIUS_KEY=your_key_here
```

Genius has no timestamps, so those lyrics scroll on an estimated schedule.

### Debugging

```bash
npm test                                # unit tests
npm run test:layout                     # hidden Electron lyric positioning check
node scripts/test-bridge.mjs               # watch raw SMTC state for 10s
node scripts/test-bridge.mjs "seek 30"     # watch, then send a command
```

Set `SMTC_LOG=trace.log` to trace the PowerShell poll loop to a file.

### Known limitations

- Windows only. macOS has no comparable public now-playing API.
- Controls are limited to what the source app reports as supported; the UI
  disables the rest (`caps` in the state payload).
- Seeking depends on the app honouring `TryChangePlaybackPositionAsync`.
  Spotify does; some apps ignore it.
