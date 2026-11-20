//! Where Playback keeps its files: %APPDATA%\Playback, the folder the
//! Electron build uses, so both read the same tokens, options and recorded
//! version.

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    roaming.join("Playback")
}

pub fn data_file(name: &str) -> PathBuf {
    data_dir().join(name)
}
