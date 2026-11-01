# Project rules

## The name
The app is **Playback**; it was **player** up to 1.4.2. The rename (1.4.3) moved:
- the repo: `dev-avs/player` → `dev-avs/playback`. GitHub redirects the old name, and every install from before the rename updates through that redirect. **Never create a repo named `player` under `dev-avs`**: it would take the old name back and those installs would stop finding updates.
- the exe: `player.exe` → `Playback.exe`, in the same install folder (an existing install stays where it is; new installs go to `%LOCALAPPDATA%\Programs\Playback`).
- the data folder: `%APPDATA%\player` → `%APPDATA%\Playback`, copied (not moved) on the first launch (`electron/main.cjs`, top), so a Version control revert to a release from before the rename still finds its settings. Uninstall removes both.
- the file names: `playback-setup-<version>.exe`, `playback-<version>-x64.nsis.7z`, `playback-installer.exe` (`player-…` up to 1.4.2; the installer and Version control match either).
- unchanged on purpose: `appId` (`com.player.smtcwidget`, how the updater knows it is the same app), `window.player` (the preload API), `spicetify/player-bridge.js` (renaming it would make everyone re-apply Spicetify).

## After any code change: rebuild, release; the user updates
The user runs the installed app, not the dev build. A fix is not done until it is released. Never install it for them: they update through the app's own updater, which is how that path gets tested.

People install with `playback-installer.exe` (`installer/`, a Tauri app): it picks a folder and options (Spotify bridge, desktop shortcut), downloads the newest release's stub and package with progress, and runs the stub silently (`/S --package-file=<package> /D=<folder>`, the same call the updater makes). It reads the newest release at run time, so it only needs rebuilding when `installer/` changes; every release carries a copy so `releases/latest/download/playback-installer.exe` always works. "Spotify bridge" off is written to `%APPDATA%\Playback\install-options.json` (`"bridge": false`); Playback then leaves Spicetify alone until Setup's Turn on.

Underneath, the installer is a web installer (`nsis-web`): `playback-setup-<version>.exe` is a ~1 MB stub that downloads `playback-<version>-x64.nsis.7z` from the GitHub release while it installs. So the release has to be on GitHub **before** installing. The installed app updates itself from the same releases (`electron/updater.cjs`, electron-updater): it asks on launch (Later / Update), and Settings → Behavior → Updates checks and installs; nothing installs without a click. It reads `latest.yml` from the newest release, so every release must carry it.

0. Refresh the Spicy Lyrics copy (see below).
1. `npm test` — must pass.
2. Bump the version in `package.json` by one step, each part a single digit: 1.2.8 → 1.2.9 → 1.3.0 (after .9 the next part goes up). Add the new version's entry at the top of `src/changelog.ts`, written for the user and filed under the sections in `CHANGE_KINDS` (New, Appearance, Quality of life, Performance, Fixes).
3. `npm run package` — `scripts/pack.cjs` packages the app into `release/Playback-win32-x64/` (an allow-list: `dist`, `electron`, `spicetify`, `package.json` and the production dependencies from `npm ls --omit=dev`; electron-builder's own packaging fails on this machine, and packager's prune drops electron-updater), then electron-builder makes `release/nsis-web/`: `playback-setup-<version>.exe`, `playback-<version>-x64.nsis.7z` and `latest.yml`.
   If `native/smtc-thumb/` changed, run `npm run thumb` first: it builds the SMTC thumbnail helper (Rust) and copies it to `electron/smtc-thumb.exe`, which is committed and packaged from there.
   If `installer/` changed, also `npm run installer` (`npm install` in `installer/` first on a fresh clone): it builds `installer/src-tauri/target/release/playback-installer.exe`.
4. Keep only the new build in `release/`: `release/nsis-web/` (just the new version's three files) and `release/Playback-win32-x64/`. Delete anything else (old installers, blockmaps, `builder-debug.yml`, old version folders).
5. Commit and tag the release in git: `git add -A`, commit as `Release <version>` with the changelog entry's points as the body, then `git tag v<version>`. Push both to GitHub (`origin` = https://github.com/dev-avs/playback, **public**): `git push` and `git push --tags`. Check `git status` is clean and up to date with `origin/main`.
6. Publish the GitHub release with all four files (`release/` is gitignored, so this is the only place they go online, and the installer, the stub and the updater all download from it): `gh release create v<version> release/nsis-web/playback-setup-<version>.exe release/nsis-web/playback-<version>-x64.nsis.7z release/nsis-web/latest.yml installer/src-tauri/target/release/playback-installer.exe --title "Playback <version>" --notes-file <notes>`, where the notes are the changelog entry with one `### <section>` heading per `CHANGE_KINDS` section. Check it with `gh release view v<version>`.
7. Do not install. If the installed app was stopped for the preview, start the installed app again so it offers the update on launch: on this machine `%LOCALAPPDATA%\Programs\player\` (`player.exe` up to 1.4.2, `Playback.exe` since). If the preview ran, first set `last-version.json` in the data folder (`%APPDATA%\Playback`, or `%APPDATA%\player` while the installed version is 1.4.2 or older) back to the installed version so the changelog still pops up after the update.
8. Never restart Spotify yourself. If `spicetify/player-bridge.js` changed in this release, tell the user Spotify needs one restart after they update (Spotify only loads the new extension copy on restart).
9. Tell the user the new version is released, link the release, and say to update from the prompt or Settings → Behavior → Updates.

## Spicy Lyrics copy
`vendor/spicy-lyrics/` is a full copy of https://github.com/Spikerko/spicy-lyrics, kept in this repo as plain files for reference. The app does not run it (`src/vendor/objpack.ts` is the one file taken from it), and ESLint skips it. Always update it by reinstalling with a fresh clone, never by editing it in place:
1. Delete `vendor/spicy-lyrics/`.
2. `git clone --depth 1 https://github.com/Spikerko/spicy-lyrics vendor/spicy-lyrics`
3. Note their commit with `git -C vendor/spicy-lyrics log -1 --format="%h %cs"`, then delete its `.git` folder so it stays plain files in this repo.
4. If `git status` shows changes under `vendor/spicy-lyrics/`, they go into the release commit; mention their commit in its body (e.g. `Spicy Lyrics reference: e1d49d8 (2026-10-05)`).

## How we work
- **Preview before install.** After building a change, run it for the user to try before packaging: stop the installed `Playback` (or `player`) process, then start `node_modules/electron/dist/electron.exe .` from the project folder (it loads `dist/`). Package and release only once they approve ("ship it" / "push it"), unless they say to just ship. The preview shares the installed app's data folder, so it records its version in `%APPDATA%\Playback\last-version.json`; before releasing, set that back to the installed version so the changelog still pops up after the update.
- **Defaults never change the look.** New settings default to today's appearance. Check it: capture the widget offscreen with default settings before and after (Electron `capturePage`, fixed fake player state), and diff the images — they must be identical.
- **Settings live in one list.** `src/settings.ts` holds every setting's default (`DEFAULTS`) and storage key (`KEYS`); the widget reads them with `usePersisted`. The settings window (`src/Settings.tsx`) is built from the `items` list (category, label, explanation, control) on top of `useSettingsStore()`, which gives search, Reset per section and hold-to-reset for free. A new setting = a `DEFAULTS`/`KEYS` entry, its effect in `App.tsx`/`App.css` (usually a CSS variable or class whose fallback is today's value), and an item in `Settings.tsx`.
- **Windows:** widget (`App.tsx`), settings (`#settings`), changelog (`#changelog`, `#changelog-past`), setup (`#setup`), update prompt (`#update`), version control (`#versions`); one bundle, routed in `src/main.tsx`. Each frameless window sizes itself to its card through a resize IPC.
- The Spicetify bridge is `spicetify/player-bridge.js` (it was `player-token.js` until 1.3.3). `electron/spicetifySetup.cjs` checks and installs it; its tests are in `tests/spicetifySetup.test.cjs`.

## In progress (update or remove this section as it moves)
- The changelog after an update now lists every version since the one you had (`?from=<version>` in `src/Changelog.tsx`). It can only be fully proven by updating across two releases, e.g. 1.3.9 → 1.4.1.
