# player

A frameless, always-on-top desktop widget for Windows that mirrors and controls
whatever is currently playing — Spotify, Apple Music, browsers, VLC, anything
that registers with Windows' media transport controls — and shows time-synced
lyrics alongside it.

It does not play audio itself. Audio keeps coming from the source app; this is
an overlay that reads its state and sends it commands.

## Running it

```bash
npm install
npm run app      # dev: Vite dev server + Electron with HMR
npm start        # production build, then Electron
```

Windows only. Everything else is optional.

## How it works

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

**`electron/main.cjs`** owns the window, supervises the PowerShell process
(restarting it if it dies), and performs all network requests — the renderer
would otherwise be blocked by CORS.

**Lyrics** are looked up in order: LrcLib (exact match, then fuzzy search),
NetEase, then Genius if a key is provided. Word-level enhanced LRC is used when
available; plain lyrics are spread evenly across the track duration. Titles are
normalised first, since Spotify reports things like
`Song (feat. X) - Remastered` that lyric databases do not carry.

**Album art** is fetched from the iTunes Search API, falling back to Deezer.
The SMTC thumbnail is deliberately not used: PowerShell's WinRT projection
breaks on `IAsyncOperation<interface>`, so `OpenReadAsync` returns an
unusable `__ComObject` — and the online sources give 600 px+ artwork anyway.

## Spicy Lyrics (optional)

Syllable-timed lyrics with built-in transliteration, tried before the
LrcLib/NetEase chain and silently skipped whenever it can't serve a track.

It is keyed by **Spotify track id** and needs a **real user token**, neither of
which Windows SMTC provides — SMTC only reports title/artist/album as strings.
So this path adds a one-time Spotify login and resolves the metadata to a track
id through the Search API.

Setup:

1. Create an app at <https://developer.spotify.com/dashboard>.
2. Add `http://127.0.0.1:8888/callback` as a Redirect URI.
3. Provide the client id, either way:
   - `SPOTIFY_CLIENT_ID=<id>` in the environment, or
   - `{"clientId":"<id>"}` in `spotify.json` inside the app's userData folder.
4. Start the app and use the sign-in button that appears where lyrics would be.

Tokens land in `spotify-tokens.json` under userData (mode 600) and refresh
automatically. The consent page opens in your real browser, never an embedded
window.

### Notes on the API contract

Taken from `Query.ts` in the spicy-lyrics source, since the usual prose
description gets two details wrong:

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

## Optional configuration

Genius is only consulted if you supply a key (free at
<https://genius.com/api-clients>). Create `.env`:

```
VITE_GENIUS_KEY=your_key_here
```

Genius has no timestamps, so those lyrics scroll on an estimated schedule.

## Debugging

```bash
npm test                                # version discovery and cache regressions
npm run test:layout                     # hidden Electron lyric positioning check
node electron/test-bridge.mjs              # watch raw SMTC state for 10s
node electron/test-bridge.mjs "seek 30"    # watch, then send a command
```

Set `SMTC_LOG=trace.log` to trace the PowerShell poll loop to a file.

## Known limitations

- Windows only. macOS has no comparable public now-playing API.
- Controls are limited to what the source app reports as supported; the UI
  disables the rest (`caps` in the state payload).
- Seeking depends on the app honouring `TryChangePlaybackPositionAsync`.
  Spotify does; some apps ignore it.
