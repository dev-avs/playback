//! `--fake-state <file>`: the fake player state shared with the Electron snapshot harness
//! (`fixtures/fake-state.json`, read there by `scripts/snapshot/main.cjs` and handed to
//! `scripts/snapshot/preload.cjs`). It replaces SMTC, the bridge and the lyrics fetch.
//! The `responses` block (lyrics, updates, setup, ...) is kept as JSON until the screens that read it exist.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Caps {
    pub play: bool,
    pub pause: bool,
    pub next: bool,
    pub prev: bool,
    pub seek: bool,
    pub shuffle: bool,
    pub repeat: bool,
}

/// The SMTC event the widget gets (`src/types/player.d.ts`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub r#type: String,
    pub app_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_key: String,
    pub status: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub shuffle: bool,
    pub repeat: String,
    pub volume: f64,
    pub caps: Caps,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FakeState {
    pub state: NowPlaying,
    /// File name of the cover, next to the JSON file.
    pub cover: String,
    #[serde(default)]
    pub responses: Value,
    /// Resolved from `cover` and the JSON file's folder by `load`.
    #[serde(skip)]
    pub cover_path: PathBuf,
}

impl FakeState {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut s: FakeState = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        s.cover_path = path.parent().unwrap_or(Path::new(".")).join(&s.cover);
        if !s.cover_path.is_file() {
            return Err(format!("cover {} not found", s.cover_path.display()));
        }
        Ok(s)
    }

    /// One line for `--fake-state`, to show it loaded.
    pub fn summary(&self) -> String {
        format!(
            "fake-state: ok: \"{}\" by {} ({} / {:.1}s of {:.0}s, {}), cover {}, responses: {} keys",
            self.state.title,
            self.state.artist,
            self.state.status,
            self.state.position,
            self.state.duration,
            self.state.app_id,
            self.cover_path.display(),
            self.responses.as_object().map_or(0, |o| o.len()),
        )
    }
}
