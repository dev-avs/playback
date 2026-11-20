//! Where the Spicetify bridge stands, for the Setup window.
//!
//! The bridge (`spicetify/player-bridge.js`) runs inside Spotify and hands
//! over what the widget cannot read itself. For it to run, Spicetify must be
//! installed, the bridge copied into its Extensions folder as the current
//! version, listed in config-xpui.ini, and applied into Spotify since. This
//! reports each, as `check()` in electron/spicetifySetup.cjs does.
//!
//! Read only: putting the bridge in place, and `spicetify apply` (which
//! restarts Spotify), are not in this build yet.

use crate::support::paths::data_file;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const BRIDGE: &str = "player-bridge.js";
/// Its name before 1.3.3.
const LEGACY: [&str; 1] = ["player-token.js"];
/// The bridge as this build ships it.
const SOURCE: &[u8] = include_bytes!("../../../spicetify/player-bridge.js");

/// Where Spicetify and Spotify keep things, per their installers' defaults.
pub struct Paths {
    cli: PathBuf,
    config_dir: PathBuf,
    spotify_exe: PathBuf,
    applied_dir: PathBuf,
}

impl Paths {
    fn of_this_pc() -> Paths {
        let env = |k| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
        let (local, roaming) = (env("LOCALAPPDATA"), env("APPDATA"));
        Paths {
            cli: find_cli(local.join("spicetify").join("spicetify.exe")),
            config_dir: roaming.join("spicetify"),
            spotify_exe: roaming.join("Spotify").join("Spotify.exe"),
            applied_dir: roaming.join("Spotify").join("Apps").join("xpui").join("extensions"),
        }
    }
}

/// The official installer uses %LOCALAPPDATA%\spicetify; scoop and winget
/// only put it on PATH.
fn find_cli(preferred: PathBuf) -> PathBuf {
    if preferred.exists() {
        return preferred;
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("spicetify.exe"))
        .find(|exe| exe.exists())
        .unwrap_or(preferred)
}

/// The extensions listed in config-xpui.ini.
fn registered(config_dir: &Path) -> Vec<String> {
    let Ok(ini) = fs::read_to_string(config_dir.join("config-xpui.ini")) else { return vec![] };
    ini.lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "extensions").then(|| value.split('|').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
        })
        .unwrap_or_default()
}

fn same_as(file: &Path, source: &[u8]) -> bool {
    fs::read(file).is_ok_and(|bytes| bytes == source)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    spotify: bool,
    spicetify: bool,
    bridge_copied: bool,
    registered: bool,
    applied: bool,
    applied_current: bool,
    legacy: Vec<String>,
}

impl Status {
    fn ready(&self) -> bool {
        self.spotify && self.spicetify && self.bridge_copied && self.registered && self.applied_current && self.legacy.is_empty()
    }
}

pub fn check(source: &[u8], paths: &Paths) -> Status {
    let list = registered(&paths.config_dir);
    let extensions = paths.config_dir.join("Extensions");
    let in_spotify = paths.applied_dir.join(BRIDGE);
    Status {
        spotify: paths.spotify_exe.exists(),
        spicetify: paths.cli.exists(),
        bridge_copied: same_as(&extensions.join(BRIDGE), source),
        registered: list.iter().any(|n| n == BRIDGE),
        applied: in_spotify.exists(),
        applied_current: same_as(&in_spotify, source),
        legacy: LEGACY
            .iter()
            .filter(|n| list.iter().any(|l| l == *n) || extensions.join(n).exists() || paths.applied_dir.join(n).exists())
            .map(|n| n.to_string())
            .collect(),
    }
}

/// The installer writes `"bridge": false` here when it was turned off there.
fn bridge_wanted() -> bool {
    fs::read_to_string(data_file("install-options.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .is_none_or(|o| o["bridge"] != false)
}

/// SetupState in src/player.d.ts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    status: Status,
    ready: bool,
    actions: Vec<String>,
    needs_apply: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    bridge_off: bool,
    first_run: bool,
}

pub fn state() -> SetupState {
    let status = check(SOURCE, &Paths::of_this_pc());
    SetupState {
        ready: status.ready(),
        status,
        actions: vec![],
        needs_apply: false,
        bridge_off: !bridge_wanted(),
        first_run: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_each_step() {
        let root = std::env::temp_dir().join(format!("playback-spicetify-{}", std::process::id()));
        let paths = Paths {
            cli: root.join("spicetify.exe"),
            config_dir: root.join("config"),
            spotify_exe: root.join("Spotify.exe"),
            applied_dir: root.join("applied"),
        };
        fs::create_dir_all(paths.config_dir.join("Extensions")).unwrap();
        fs::create_dir_all(&paths.applied_dir).unwrap();
        fs::write(&paths.cli, "").unwrap();
        fs::write(&paths.spotify_exe, "").unwrap();
        fs::write(paths.config_dir.join("config-xpui.ini"), "[AdditionalOptions]\nextensions = player-token.js|player-bridge.js\n").unwrap();
        fs::write(paths.config_dir.join("Extensions").join(BRIDGE), b"new").unwrap();
        fs::write(paths.applied_dir.join(BRIDGE), b"old").unwrap();

        let s = check(b"new", &paths);
        assert!(s.spotify && s.spicetify && s.bridge_copied && s.registered && s.applied);
        assert!(!s.applied_current);
        assert_eq!(s.legacy, ["player-token.js"]);
        assert!(!s.ready());
        fs::remove_dir_all(root).unwrap();
    }
}
