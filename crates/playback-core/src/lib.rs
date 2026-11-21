//! Playback's backend, shared by the Tauri build (src-tauri) and the GPUI
//! build (src-gpui). Each build implements `host::Host` and calls
//! `rt::init` at launch; everything else here is the same for both.
//!
//! - `media`    what Windows is playing, its buttons and the app's volume
//! - `lyrics`   synced lyrics from LRCMux, NetEase and LrcLib
//! - `art`      the cover: catalogues, YouTube, the session's thumbnail
//! - `spotify`  a Spotify token: sign-in, pasted, kept on disk
//! - `spicy`    Spicy Lyrics: its API, the lyrics cache, the user's key
//! - `bridge`   what the Spicetify extension inside Spotify sends
//! - `spicetify` the Spotify bridge in Spicetify (Setup)
//! - `updates`  updates and version control, from the GitHub releases
//! - `install`  the installed app: detached installers, uninstalling
//! - `migrate`  the data folder from before the rename
//! - `fonts`    the fonts installed on this PC
//! - `support`  HTTP client, bounded cache, text and JSON helpers

pub mod art;
pub mod bridge;
pub mod fonts;
pub mod host;
pub mod install;
pub mod lyrics;
pub mod media;
pub mod migrate;
pub mod rt;
pub mod spicetify;
pub mod spicy;
pub mod spotify;
pub mod support;
pub mod updates;
