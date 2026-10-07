# Project rules

## After any code change: rebuild, reinstall, restart Spotify
The user runs the installed app, not the dev build. A fix is not done until it is installed and running.

1. `npm test` — must pass.
2. Bump the patch version in `package.json` (e.g. 1.0.5 → 1.0.6).
3. `npm run package` — builds `release/player-setup-<version>.exe`.
4. Delete every older installer in `release/`: old `player-setup-*.exe`, their `.blockmap`, and any old version folders (e.g. `release/1.0.1/`). Keep only the new installer, its blockmap, and `release/player-win32-x64/`.
5. Reinstall: stop any running `player` process, then run the new installer silently and wait for it:
   `Start-Process release/player-setup-<version>.exe -ArgumentList '/S' -Wait`
   Then start `%LOCALAPPDATA%\Programs\player\player.exe` (the installer's run-after-finish does not fire in silent mode). Starting the app copies the bundled Spicetify extension into Spotify.
6. Restart Spotify so it loads that extension copy: stop every `Spotify` process, then start `%APPDATA%\Spotify\Spotify.exe`. Do this after step 5, never before.
7. Tell the user the new version is installed and running.
