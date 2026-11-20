//! What's new after an update: the changelog opens on the first run of each
//! version, listing every version since the one recorded at the last run.
//! The version is recorded in `last-version.json`, the Electron build's file.

use super::cards::{self, Card};
use crate::support::paths::data_file;
use serde_json::json;
use std::fs;
use crate::launch::Launch;
use tauri::{AppHandle, Manager};

const FILE: &str = "last-version.json";

/// The version recorded last run, and records `current` in its place.
fn swap_recorded(current: &str) -> Option<String> {
    let file = data_file(FILE);
    let previous = fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["version"].as_str().map(String::from));
    if let Err(e) = fs::write(&file, json!({ "version": current }).to_string()) {
        eprintln!("[changelog] could not record the version: {e}");
    }
    previous
}

/// Called by the widget once per launch with the user's setting. A first run
/// (nothing recorded) shows nothing: a fresh install has nothing new. Opens
/// once Setup is out of the way.
pub async fn check_after_update(app: &AppHandle, enabled: bool) -> tauri::Result<()> {
    let current = app.package_info().version.to_string();
    match swap_recorded(&current) {
        Some(previous) if enabled && previous != current => {
            app.state::<Launch>().settled().await;
            cards::open(app, Card::Changelog, Some(format!("from={}", encode(&previous))))
        }
        _ => Ok(()),
    }
}

/// A version is digits and dots, but the file is the user's: keep the hash safe.
fn encode(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')).collect()
}
