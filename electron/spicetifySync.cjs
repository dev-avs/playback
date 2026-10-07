// Keeps the Spicetify extension in sync with the copy shipped in this build.
//
// The extension is a small companion script (`spicetify/player-token.js`) that
// runs inside Spotify and reads two things the widget cannot get on its own:
// the current Spotify token, and the syllable-timed lyrics that Spicy Lyrics
// already cached in that client's Cache Storage. Both are posted to the
// loopback bridge in tokenBridge.cjs.
//
// It lives in `%APPDATA%\spicetify\Extensions\` (Spicetify's source folder)
// and, after `spicetify apply`, also in `%APPDATA%\Spotify\Apps\xpui\extensions\`
// (the injected copy Spotify actually loads). Both are refreshed here so a
// user who never re-runs `spicetify apply` still picks up fixes — this has
// bitten us once, when Spicy Lyrics quietly renamed its Cache Storage store
// and the deployed extension kept reading the old (now-empty) name for weeks.
//
// Only touches files whose content differs, and only inside the two well-known
// paths above. If neither exists the user does not have Spicetify installed
// and this is a no-op.

const fs = require("fs");
const path = require("path");

const EXT_NAME = "player-token.js";

function targets() {
  const appData = process.env.APPDATA;
  if (!appData) return [];
  return [
    path.join(appData, "spicetify", "Extensions", EXT_NAME),
    path.join(appData, "Spotify", "Apps", "xpui", "extensions", EXT_NAME),
  ];
}

/**
 * @param {string} sourcePath Absolute path to the bundled extension.
 * @returns {{updated: string[], skipped: string[]}}
 */
function syncSpicetifyExtension(sourcePath) {
  const updated = [];
  const skipped = [];
  let source;
  try {
    source = fs.readFileSync(sourcePath);
  } catch (e) {
    console.error(`[spicetify-sync] cannot read bundled extension at ${sourcePath}: ${e.message}`);
    return { updated, skipped };
  }

  for (const target of targets()) {
    // Only refresh a copy that already exists. Creating one from scratch would
    // register an extension the user never asked for, and Spotify would ignore
    // it anyway until `spicetify config extensions player-token.js` had run.
    if (!fs.existsSync(target)) {
      skipped.push(target);
      continue;
    }
    try {
      const current = fs.readFileSync(target);
      if (current.equals(source)) {
        skipped.push(target);
        continue;
      }
      fs.writeFileSync(target, source);
      updated.push(target);
      console.log(`[spicetify-sync] refreshed ${target}`);
    } catch (e) {
      console.error(`[spicetify-sync] could not refresh ${target}: ${e.message}`);
    }
  }

  return { updated, skipped };
}

module.exports = { syncSpicetifyExtension };
