//! What the Electron build left behind, brought into this one on its first
//! launch: the data folder from before the rename, and the settings the page
//! kept in Electron's localStorage.
//!
//! Both builds use the same data folder, so tokens, the Spicy key, the
//! lyrics cache and the recorded version carry over by themselves. The
//! page's settings do not: Electron kept them in its own profile (a LevelDB
//! under `Local Storage`), and the web view here has another. They are read
//! from there once and handed to the page before its scripts run, so the
//! widget opens looking as it did.

use crate::support::paths::data_dir;
pub use playback_core::migrate::adopt_legacy_folder;
use playback_core::migrate::copy_dir;
use rusty_leveldb::{LdbIterator, Options, DB};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

/// Electron's localStorage origin for the bundled page.
const ORIGIN: &[u8] = b"_file://\x00";

/// Written once the settings have been handed over, so they are not read
/// again (and a setting the user has since reset does not come back).
const IMPORTED: &str = "electron-settings-imported";

/// A localStorage string as Chromium stores it: a leading 0 for UTF-16LE, 1
/// for Latin-1.
fn decode(bytes: &[u8]) -> Option<String> {
    let (kind, rest) = bytes.split_first()?;
    match kind {
        0 if rest.len() % 2 == 0 => {
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16(&units).ok()
        }
        1 => Some(rest.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

/// Every key and value the bundled page stored.
fn read_electron_storage(leveldb: &Path) -> Result<BTreeMap<String, String>, String> {
    // a copy: Electron may still hold the original's lock, and opening
    // writes to it
    let copy = std::env::temp_dir().join("playback-settings-import");
    let _ = fs::remove_dir_all(&copy);
    copy_dir(leveldb, &copy, &["LOCK"]).map_err(|e| e.to_string())?;
    let opts = Options {
        create_if_missing: false,
        ..Options::default()
    };
    let found = (|| {
        let mut db = DB::open(&copy, opts).map_err(|e| e.to_string())?;
        let mut it = db.new_iter().map_err(|e| e.to_string())?;
        let mut out = BTreeMap::new();
        while let Some((k, v)) = it.next() {
            if let Some(key) = k.strip_prefix(ORIGIN).and_then(decode) {
                out.extend(decode(&v).map(|v| (key, v)));
            }
        }
        Ok(out)
    })();
    let _ = fs::remove_dir_all(&copy);
    found
}

/// The script that hands the settings over: each one the page does not have
/// yet, once (the page's own marker guards a second window).
fn script(settings: &BTreeMap<String, String>) -> String {
    let data = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    format!(
        "(() => {{ try {{ \
           if (localStorage.getItem('playback:electronImported')) return; \
           for (const [k, v] of Object.entries({data})) if (localStorage.getItem(k) === null) localStorage.setItem(k, v); \
           localStorage.setItem('playback:electronImported', '1'); \
         }} catch {{}} }})();"
    )
}

fn marker(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join(IMPORTED))
}

/// The settings script for every window to run, until it has run once.
/// Read on the first call of a launch.
pub fn settings_script(app: &AppHandle) -> Option<&'static str> {
    static SCRIPT: OnceLock<Option<String>> = OnceLock::new();
    SCRIPT.get_or_init(|| read_script(app)).as_deref()
}

fn read_script(app: &AppHandle) -> Option<String> {
    if marker(app)?.exists() {
        return None;
    }
    let leveldb = data_dir().join("Local Storage").join("leveldb");
    if !leveldb.exists() {
        return None;
    }
    match read_electron_storage(&leveldb) {
        Ok(settings) if !settings.is_empty() => {
            eprintln!(
                "[migrate] {} settings from the Electron build",
                settings.len()
            );
            Some(script(&settings))
        }
        Ok(_) => None,
        Err(e) => {
            eprintln!("[migrate] could not read the Electron settings: {e}");
            None
        }
    }
}

/// The widget's page has loaded with the settings: they are not read again.
pub fn settings_imported(app: &AppHandle) {
    let Some(file) = marker(app) else { return };
    if file.exists() {
        return;
    }
    let _ = file.parent().map(fs::create_dir_all);
    if let Err(e) = fs::write(&file, "") {
        eprintln!("[migrate] could not record the import: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_both_string_forms() {
        assert_eq!(decode(b"\x01lyricsFont").as_deref(), Some("lyricsFont"));
        assert_eq!(decode(b"\x01caf\xe9").as_deref(), Some("café"));
        let utf16: Vec<u8> = std::iter::once(0)
            .chain("日本".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode(&utf16).as_deref(), Some("日本"));
        assert_eq!(decode(b"\x00\x41"), None);
        assert_eq!(decode(b""), None);
    }

    #[test]
    fn the_script_carries_the_settings() {
        let s = script(&BTreeMap::from([("a'b".to_string(), "\"x\"".to_string())]));
        assert!(s.contains(r#"{"a'b":"\"x\""}"#));
        assert!(s.contains("playback:electronImported"));
    }

    #[test]
    fn reads_a_leveldb_like_electrons() {
        let dir =
            std::env::temp_dir().join(format!("playback-migrate-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let mut db = DB::open(&dir, Options::default()).unwrap();
            db.put(b"VERSION", b"1").unwrap();
            db.put(b"META:file://", b"x").unwrap();
            db.put(b"_file://\x00\x01opacity", b"\x010.8").unwrap();
            db.put(b"_http://localhost:5173\x00\x01opacity", b"\x010.3")
                .unwrap();
            db.flush().unwrap();
        }
        let got = read_electron_storage(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            got,
            BTreeMap::from([("opacity".to_string(), "0.8".to_string())])
        );
    }
}
