//! The data folder from before the rename (1.4.3), brought over on the
//! first launch. The page's settings from Electron's localStorage are the
//! Tauri build's own step (src-tauri/src/migrate.rs).

use crate::support::paths::data_dir;
use std::fs;
use std::path::Path;

/// The data folder's name before 1.4.3.
pub const LEGACY_DATA: &str = "player";

/// A copy of electron/main.cjs's rename step, for an install updated
/// straight from 1.4.2 or older: the old folder is copied, not moved, so a
/// Version control revert to such a release still finds its settings.
pub fn adopt_legacy_folder() {
    let now = data_dir();
    let before = now.with_file_name(LEGACY_DATA);
    if now.exists() || !before.exists() {
        return;
    }
    if let Err(e) = copy_dir(&before, &now, &["lockfile"]) {
        eprintln!("[migrate] could not copy the old settings: {e}");
    }
}

pub fn copy_dir(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.iter().any(|s| name == *s) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target, skip)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
