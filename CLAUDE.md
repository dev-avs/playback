# Project rules

## After any code change: rebuild, release, reinstall, restart Spotify if needed
The user runs the installed app, not the dev build. A fix is not done until it is released, installed and running.

The installer is a web installer (`nsis-web`): `player-setup-<version>.exe` is a ~1 MB stub that downloads `player-<version>-x64.nsis.7z` from the GitHub release while it installs. So the release has to be on GitHub **before** installing. The installed app updates itself from the same releases (`electron/updater.cjs`, electron-updater): it asks on launch (Later / Update), and Settings → Behavior → Updates checks and installs; nothing installs without a click. It reads `latest.yml` from the newest release, so every release must carry it.

0. Refresh the Spicy Lyrics copy (see below).
1. `npm test` — must pass.
2. Bump the version in `package.json` by one step, each part a single digit: 1.2.8 → 1.2.9 → 1.3.0 (after .9 the next part goes up). Add the new version's entry at the top of `src/changelog.ts`, written for the user and filed under the sections in `CHANGE_KINDS` (New, Appearance, Quality of life, Performance, Fixes).
3. `npm run package` — `scripts/pack.cjs` packages the app into `release/player-win32-x64/` (an allow-list: `dist`, `electron`, `spicetify`, `package.json` and the production dependencies from `npm ls --omit=dev`; electron-builder's own packaging fails on this machine, and packager's prune drops electron-updater), then electron-builder makes `release/nsis-web/`: `player-setup-<version>.exe`, `player-<version>-x64.nsis.7z` and `latest.yml`.
4. Keep only the new build in `release/`: `release/nsis-web/` (just the new version's three files) and `release/player-win32-x64/`. Delete anything else (old installers, blockmaps, `builder-debug.yml`, old version folders).
5. Commit and tag the release in git: `git add -A`, commit as `Release <version>` with the changelog entry's points as the body, then `git tag v<version>`. Push both to GitHub (`origin` = https://github.com/dev-avs/player, **public**): `git push` and `git push --tags`. Check `git status` is clean and up to date with `origin/main`.
6. Publish the GitHub release with all three files (`release/` is gitignored, so this is the only place they go online, and the stub and the updater both download from it): `gh release create v<version> release/nsis-web/player-setup-<version>.exe release/nsis-web/player-<version>-x64.nsis.7z release/nsis-web/latest.yml --title "player <version>" --notes-file <notes>`, where the notes are the changelog entry with one `### <section>` heading per `CHANGE_KINDS` section. Check it with `gh release view v<version>`.
7. Reinstall: stop any running `player` process, then run the new stub silently and wait for it (it downloads from the release just made):
   `Start-Process release/nsis-web/player-setup-<version>.exe -ArgumentList '/S' -Wait`
   Then start `%LOCALAPPDATA%\Programs\player\player.exe` (run-after-finish does not fire in silent mode). Starting the app checks the Spicetify bridge (electron/spicetifySetup.cjs) and refreshes Spotify's copy of it. If the preview ran since the last install, first set `%APPDATA%\player\last-version.json` back to the installed version so the changelog still pops up.
8. Restart Spotify **only if necessary**: when `spicetify/player-bridge.js` changed since the last install, because Spotify only loads the new extension copy on restart. Otherwise leave Spotify alone — restarting it interrupts the user's music. When needed: stop every `Spotify` process, then start `%APPDATA%\Spotify\Spotify.exe`. Do this after step 7, never before.
9. Tell the user the new version is released, installed and running, whether Spotify was restarted (and why), and link the release.

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
- The changelog after an update now lists every version since the one you had (`?from=<version>` in `src/Changelog.tsx`). It can only be fully proven by updating across two releases, e.g. 1.3.9 → 1.4.1.
