//! Playback's UI logic for the GPUI build (Project Phoenix, port.md 2.2).
//!
//! Each module is a function-for-function translation of a TypeScript file
//! in `src/`, in the same order and with the same names (snake_case). Quirks
//! are ported, not fixed. `tests/golden.rs` checks every module against the
//! JSON that `scripts/golden/run.mts` writes from the TypeScript originals.

pub mod cubic_spline;
pub mod lyric_animator;
pub mod line_state;
pub mod lyrics;
pub mod objpack;
pub mod pinyin;
pub mod romanize;
pub mod settings;
pub mod spicy;
pub mod uroman;
