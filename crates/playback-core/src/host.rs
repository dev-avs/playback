//! What the shared backend needs from the app around it (the Tauri build,
//! the GPUI build): telling its windows, its version, what is playing, and
//! quitting.

use serde_json::Value;

pub trait Host: Send + Sync + 'static {
    /// An event to every window.
    fn emit(&self, event: &str, payload: Value);
    /// An event to the widget only.
    fn emit_widget(&self, event: &str, payload: Value);
    /// This build's version, e.g. "1.6.0".
    fn version(&self) -> String;
    /// What the media session reports now (`media::Media::latest`).
    fn media_latest(&self) -> Option<Value>;
    /// Quits the app (an installer is taking over).
    fn exit(&self, code: i32);
}
