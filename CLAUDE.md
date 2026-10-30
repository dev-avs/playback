# Project rules

## After any code change: rebuild, reinstall, restart Spotify if needed
The user runs the installed app, not the dev build. A fix is not done until it is installed and running.

0. Refresh the Spicy Lyrics copy (see below).
1. `npm test` — must pass.
2. Bump the version in `package.json` by one step, each part a single digit: 1.2.8 → 1.2.9 → 1.3.0 (after .9 the next part goes up). Add the new version's entry at the top of `src/changelog.ts`, written for the user and filed under the sections in `CHANGE_KINDS` (New, Appearance, Quality of life, Performance, Fixes).
3. `npm run package` — builds `release/player-setup-<version>.exe`.
4. Delete every older installer in `release/`: old `player-setup-*.exe`, their `.blockmap`, and any old version folders (e.g. `release/1.0.1/`). Keep only the new installer, its blockmap, and `release/player-win32-x64/`.
5. Reinstall: stop any running `player` process, then run the new installer silently and wait for it:
   `Start-Process release/player-setup-<version>.exe -ArgumentList '/S' -Wait`
   Then start `%LOCALAPPDATA%\Programs\player\player.exe` (the installer's run-after-finish does not fire in silent mode). Starting the app checks the Spicetify bridge (electron/spicetifySetup.cjs) and refreshes Spotify's copy of it.
6. Restart Spotify **only if necessary**: when `spicetify/player-bridge.js` changed since the last install, because Spotify only loads the new extension copy on restart. Otherwise leave Spotify alone — restarting it interrupts the user's music. When needed: stop every `Spotify` process, then start `%APPDATA%\Spotify\Spotify.exe`. Do this after step 5, never before.
7. Commit and tag the release in git (the repo starts at 1.3.0; earlier versions are not in it): `git add -A`, commit as `Release <version>` with the changelog entry's points as the body, then `git tag v<version>`. Then push both to GitHub (`origin` = https://github.com/dev-avs/player, private): `git push` and `git push --tags`. Check `git status` is clean and up to date with `origin/main` afterwards.
8. Publish a GitHub release for the tag (pushing a tag does not make one, and `release/` is gitignored, so this is the only place the installer goes online): `gh release create v<version> release/player-setup-<version>.exe --title "player <version>" --notes-file <notes>`, where the notes are the changelog entry with one `### <section>` heading per `CHANGE_KINDS` section. Check it with `gh release view v<version>`.
9. Tell the user the new version is installed and running, whether Spotify was restarted (and why), and link the release.

## Spicy Lyrics copy
`vendor/spicy-lyrics/` is a full copy of https://github.com/Spikerko/spicy-lyrics, kept in this repo as plain files for reference. The app does not run it (`src/vendor/objpack.ts` is the one file taken from it), and ESLint skips it. Always update it by reinstalling with a fresh clone, never by editing it in place:
1. Delete `vendor/spicy-lyrics/`.
2. `git clone --depth 1 https://github.com/Spikerko/spicy-lyrics vendor/spicy-lyrics`
3. Note their commit with `git -C vendor/spicy-lyrics log -1 --format="%h %cs"`, then delete its `.git` folder so it stays plain files in this repo.
4. If `git status` shows changes under `vendor/spicy-lyrics/`, they go into the release commit; mention their commit in its body (e.g. `Spicy Lyrics reference: e1d49d8 (2026-10-05)`).

## How we work
- **Preview before install.** After building a change, run it for the user to try before packaging: stop the installed `player` process, then start `node_modules/electron/dist/electron.exe .` from the project folder (it loads `dist/`). Package and install only once they approve ("ship it" / "push it"), unless they say to just ship. The preview shares the installed app's data folder, so it records its version in `%APPDATA%\player\last-version.json`; before installing, set that back to the installed version so the changelog still pops up.
- **Defaults never change the look.** New settings default to today's appearance. Check it: capture the widget offscreen with default settings before and after (Electron `capturePage`, fixed fake player state), and diff the images — they must be identical.
- **Settings live in one list.** `src/settings.ts` holds every setting's default (`DEFAULTS`) and storage key (`KEYS`); the widget reads them with `usePersisted`. The settings window (`src/Settings.tsx`) is built from the `items` list (category, label, explanation, control) on top of `useSettingsStore()`, which gives search, Reset per section and hold-to-reset for free. A new setting = a `DEFAULTS`/`KEYS` entry, its effect in `App.tsx`/`App.css` (usually a CSS variable or class whose fallback is today's value), and an item in `Settings.tsx`.
- **Windows:** widget (`App.tsx`), settings (`#settings`), changelog (`#changelog`, `#changelog-past`); one bundle, routed in `src/main.tsx`. Each frameless window sizes itself to its card through a resize IPC.
- The Spicetify bridge is `spicetify/player-bridge.js` (it was `player-token.js` until 1.3.3). `electron/spicetifySetup.cjs` checks and installs it; its tests are in `tests/spicetifySetup.test.cjs`.

## In progress (update or remove this section as it moves)
1.3.4 is released but deliberately not installed: on 2026-10-30 the user had the app uninstalled, `%APPDATA%\player` deleted (key, cookies, lyrics cache) and `player-bridge.js` taken out of Spicetify, to try a new-PC first run with `release/player-setup-1.3.4.exe`. Don't assume an installed version.
- Spicy Lyrics key (`electron/spicyKey.cjs`): each user creates an application on the dashboard. A catalog template ("player") was submitted for review; once approved, switch `KEY_PAGE_URL` to its catalog page and the toast/Setup/Settings wording to "click Add".
- The Spicy API's rules: show attribution with the lyrics, cache for no more than 30 days (`REFRESH_AFTER_MS` in `lyricsCache.cjs`).
- Possibly later: NetEase sometimes returns credits as lyric lines ("Estelle -- American Boy (Ft. Kanye West)", "…, songwriters", "…, producer"); the user said to leave this for now.
- `npm run lint` fails on a setState-in-effect in `App.tsx` (`setOutOfSync(false)`), already in 1.3.2; not fixed yet.
